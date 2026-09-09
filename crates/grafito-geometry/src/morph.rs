//! Transform general entre formas 2D (F2a, estilo Manim `Transform`).
//!
//! Hoy el morph univariado `y = (1-s)·A + s·B` vive en
//! `grafito-anim/src/parametric.rs` y solo interpola funciones. Este módulo
//! generaliza la idea a CUALQUIER par de formas 2D (polígono→polígono,
//! curva→curva, punto→figura) por correspondencia de puntos:
//!
//! 1. **Resample uniforme por longitud de arco** (`resample_polyline`): ambas
//!    formas se remuestrean a `samples` puntos equiespaciados sobre su
//!    perímetro, así el punto `i` de A se corresponde con el punto `i` de B.
//! 2. **Alineación de inicio** (`align_start`): en figuras cerradas se rota B
//!    para que su punto 0 sea el más cercano al punto 0 de A (evita el giro
//!    de media vuelta); en abiertas se invierte B si queda al revés.
//! 3. **Interpolación con easing**: cada frame `f` interpola `A + (B-A)·e(s)`
//!    con `s = f/(frames-1)` pasado por uno de los 8 easings de
//!    `grafito-ui/src/animation.rs` (reimplementados aquí en `f64` porque el
//!    cerebro no puede depender de la piel; ver `MorphEasing`).
//!
//! Figuras cerradas vs abiertas: honesto con el flag `closed` de
//! `MorphConfig`. Una cerrada necesita ≥3 puntos (o 1 si es un punto que
//! crece); con 2 puntos se devuelve `Err` en vez de cerrar en silencio.
//! Un punto solo (1 punto) se interpreta como punto→figura estilo Manim: se
//! replica al primer punto y la figura "crece" desde ahí.
//!
//! Todo acotado: `samples` en 2..=`MORPH_MAX_SAMPLES` (512), `frames` en
//! 1..=`MORPH_MAX_FRAMES` (48, igual que `PARAMETRIC_MAX_FRAMES`), entradas
//! de hasta `MORPH_MAX_INPUT_POINTS` puntos. Todo `checked`, cero `unwrap`
//! en prod. Formas incompatibles (vacías, >cap, no finitas) → `Err`
//! honesto en rioplatense, jamás deforma en silencio.
//!
//! Puro, sin I/O, sin egui, sin red.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_geometry::morph::{MorphConfig, morph_shapes};
//! use grafito_geometry::Point2;
//!
//! let cuadrada = [
//!     Point2::new(0.0, 0.0),
//!     Point2::new(1.0, 0.0),
//!     Point2::new(1.0, 1.0),
//!     Point2::new(0.0, 1.0),
//! ];
//! let triangulo = [
//!     Point2::new(0.0, 0.0),
//!     Point2::new(1.0, 0.0),
//!     Point2::new(0.5, 1.0),
//! ];
//! let cfg = MorphConfig::try_new(8, 4, false, true, Default::default()).unwrap();
//! let frames = morph_shapes(&cuadrada, &triangulo, &cfg).unwrap();
//! assert_eq!(frames.len(), 4);
//! assert!(frames.iter().all(|f| f.len() == 8));
//! ```

use crate::types::Point2;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Tope de puntos remuestreados por forma (cota anti-OOM).
pub const MORPH_MAX_SAMPLES: usize = 512;
/// Tope de fotogramas por morph (igual que `PARAMETRIC_MAX_FRAMES`).
pub const MORPH_MAX_FRAMES: usize = 48;
/// Tope de puntos de cada forma de entrada (cota anti-OOM).
pub const MORPH_MAX_INPUT_POINTS: usize = 4096;
/// Muestras por defecto (calidad linda sin pasarse de memoria).
pub const MORPH_DEFAULT_SAMPLES: usize = 64;
/// Fotogramas por defecto (igual que `PARAMETRIC_DEFAULT_FRAMES`).
pub const MORPH_DEFAULT_FRAMES: usize = 24;
/// Tope de bytes del set (`samples*frames*16`, dos `f64` por punto).
pub const MORPH_MAX_BYTES: usize = 1024 * 1024;

/// Easing para la interpolación (los 8 de `grafito-ui/src/animation.rs`,
/// acá en `f64` porque el cerebro no depende de la piel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MorphEasing {
    /// Progresión constante.
    Linear,
    /// Acelera desde cero (`t²`).
    QuadraticIn,
    /// Desacelera hasta el final (`t·(2-t)`).
    QuadraticOut,
    /// Acelera cúbico (`t³`).
    CubicIn,
    /// Desacelera cúbico.
    CubicOut,
    /// Acelera y desacelera cúbico (default, el más Manim).
    #[default]
    CubicInOut,
    /// Suave sinusoidal.
    SinInOut,
    /// Sobrepasa un poco la meta y vuelve (rebote; el único que sale de 0..1).
    EaseOutBack,
}

impl MorphEasing {
    /// Nombre estable para wire/logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::QuadraticIn => "quadratic_in",
            Self::QuadraticOut => "quadratic_out",
            Self::CubicIn => "cubic_in",
            Self::CubicOut => "cubic_out",
            Self::CubicInOut => "cubic_in_out",
            Self::SinInOut => "sin_in_out",
            Self::EaseOutBack => "ease_out_back",
        }
    }

    /// Los 8 nombres aceptados por `from_name` (minúsculas, con/sin guiones).
    pub fn from_name(raw: &str) -> Option<Self> {
        let norm: String = raw
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        match norm.as_str() {
            "linear" | "lineal" => Some(Self::Linear),
            "quadraticin" => Some(Self::QuadraticIn),
            "quadraticout" => Some(Self::QuadraticOut),
            "cubicin" => Some(Self::CubicIn),
            "cubicout" => Some(Self::CubicOut),
            "cubicinout" => Some(Self::CubicInOut),
            "sininout" | "sineinout" | "sinusoidal" => Some(Self::SinInOut),
            "easeoutback" | "back" | "rebote" => Some(Self::EaseOutBack),
            _ => None,
        }
    }

    /// Aplica el easing a `t` en 0..1 (clamp + guardia finita, sin pánicos).
    pub fn apply(self, t: f64) -> f64 {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let v = match self {
            Self::Linear => t,
            Self::QuadraticIn => t * t,
            Self::QuadraticOut => t * (2.0 - t),
            Self::CubicIn => t * t * t,
            Self::CubicOut => {
                let u = t - 1.0;
                u * u * u + 1.0
            }
            Self::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = t - 1.0;
                    4.0 * u * u * u + 1.0
                }
            }
            Self::SinInOut => -((std::f64::consts::PI * t).cos() - 1.0) * 0.5,
            Self::EaseOutBack => {
                let c1 = 1.70158_f64;
                let c3 = c1 + 1.0;
                let u = t - 1.0;
                1.0 + c3 * u * u * u + c1 * u * u
            }
        };
        if v.is_finite() {
            v
        } else {
            t
        }
    }
}

/// Configuración validada del morph (`try_new` falla honesto si está mal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorphConfig {
    /// Puntos por forma remuestreada (2..=512).
    pub samples: usize,
    /// Fotogramas del set (1..=48; 1 frame = estado final B).
    pub frames: usize,
    /// `true` = polígono cerrado (se recorre el perímetro con vuelta).
    pub closed: bool,
    /// `true` = alinear el inicio de B con el de A (rotar/invertir).
    pub align_start: bool,
    /// Curva de easing entre frames.
    pub easing: MorphEasing,
}

impl Default for MorphConfig {
    fn default() -> Self {
        Self {
            samples: MORPH_DEFAULT_SAMPLES,
            frames: MORPH_DEFAULT_FRAMES,
            closed: false,
            align_start: true,
            easing: MorphEasing::CubicInOut,
        }
    }
}

impl MorphConfig {
    /// Constructor validado (todo `Err` es honesto, sin pánicos).
    pub fn try_new(
        samples: usize,
        frames: usize,
        closed: bool,
        align_start: bool,
        easing: MorphEasing,
    ) -> Result<Self, MorphError> {
        if !(2..=MORPH_MAX_SAMPLES).contains(&samples) {
            return Err(MorphError::MuestrasFueraDeRango {
                got: samples,
                max: MORPH_MAX_SAMPLES,
            });
        }
        if frames == 0 || frames > MORPH_MAX_FRAMES {
            return Err(MorphError::FramesFueraDeRango {
                got: frames,
                max: MORPH_MAX_FRAMES,
            });
        }
        // Presupuesto del set con `checked` (con las cotas no puede
        // desbordar, pero honesto ante todo).
        let bytes = samples
            .checked_mul(frames)
            .and_then(|v| v.checked_mul(16))
            .ok_or(MorphError::PresupuestoExcedido {
                bytes: usize::MAX,
                max: MORPH_MAX_BYTES,
            })?;
        if bytes > MORPH_MAX_BYTES {
            return Err(MorphError::PresupuestoExcedido {
                bytes,
                max: MORPH_MAX_BYTES,
            });
        }
        Ok(Self {
            samples,
            frames,
            closed,
            align_start,
            easing,
        })
    }
}

/// Error honesto del morph (siempre dice qué está mal, en rioplatense).
#[derive(Debug, Clone, PartialEq)]
pub enum MorphError {
    /// Alguna forma vino vacía.
    FormaVacia { cual: &'static str },
    /// Algún punto no es finito (NaN/inf).
    PuntoNoFinito { cual: &'static str, indice: usize },
    /// La entrada supera la cota anti-OOM.
    DemasiadosPuntos {
        cual: &'static str,
        got: usize,
        max: usize,
    },
    /// `samples` fuera de 2..=512.
    MuestrasFueraDeRango { got: usize, max: usize },
    /// `frames` fuera de 1..=48.
    FramesFueraDeRango { got: usize, max: usize },
    /// Cerrada con 2 puntos (ni polígono ni punto).
    CerradaNecesitaTres { cual: &'static str, got: usize },
    /// El set excede el tope de bytes o desborda `usize`.
    PresupuestoExcedido { bytes: usize, max: usize },
}

impl Display for MorphError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::FormaVacia { cual } => write!(
                f,
                "la forma {cual} está vacía: pasame al menos 1 punto (2 si es abierta, 3 si es cerrada)"
            ),
            Self::PuntoNoFinito { cual, indice } => write!(
                f,
                "la forma {cual} tiene un punto no finito en el índice {indice}: revisá NaN o infinitos"
            ),
            Self::DemasiadosPuntos { cual, got, max } => write!(
                f,
                "la forma {cual} tiene {got} puntos y el tope es {max}: simplificala o dividila"
            ),
            Self::MuestrasFueraDeRango { got, max } => write!(
                f,
                "muestras fuera de rango: pediste {got}, usá entre 2 y {max}"
            ),
            Self::FramesFueraDeRango { got, max } => write!(
                f,
                "fotogramas fuera de rango: pediste {got}, usá entre 1 y {max}"
            ),
            Self::CerradaNecesitaTres { cual, got } => write!(
                f,
                "la forma {cual} cerrada necesita al menos 3 puntos (o 1 si es un punto que crece); pasaste {got}"
            ),
            Self::PresupuestoExcedido { bytes, max } => write!(
                f,
                "el set estimado ({bytes} bytes) excede el tope de {max} bytes: bajá muestras o fotogramas"
            ),
        }
    }
}

impl Error for MorphError {}

/// Valida una forma de entrada (no vacía, acotada, toda finita).
fn valida_forma(pts: &[Point2], cual: &'static str, closed: bool) -> Result<(), MorphError> {
    if pts.is_empty() {
        return Err(MorphError::FormaVacia { cual });
    }
    if pts.len() > MORPH_MAX_INPUT_POINTS {
        return Err(MorphError::DemasiadosPuntos {
            cual,
            got: pts.len(),
            max: MORPH_MAX_INPUT_POINTS,
        });
    }
    for (i, p) in pts.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(MorphError::PuntoNoFinito { cual, indice: i });
        }
    }
    if closed && pts.len() == 2 {
        return Err(MorphError::CerradaNecesitaTres { cual, got: 2 });
    }
    Ok(())
}

/// Remuestrea una polilínea a `samples` puntos equiespaciados por longitud de
/// arco. Si `closed`, recorre el perímetro con vuelta (muestra `j` en
/// `total·j/samples`, sin duplicar el cierre); si no, incluye ambos extremos
/// (`total·j/(samples-1)`). Degenerada (largo cero o 1 punto) → replica el
/// primer punto estilo Manim punto→figura. Puro, sin pánicos.
pub fn resample_polyline(
    pts: &[Point2],
    samples: usize,
    closed: bool,
) -> Result<Vec<Point2>, MorphError> {
    if !(2..=MORPH_MAX_SAMPLES).contains(&samples) {
        return Err(MorphError::MuestrasFueraDeRango {
            got: samples,
            max: MORPH_MAX_SAMPLES,
        });
    }
    valida_forma(pts, "A", closed)?;
    let n = pts.len();
    if n == 1 {
        // Punto→figura: el punto crece replicado (honesto y documentado).
        let mut out = Vec::with_capacity(samples);
        for _ in 0..samples {
            out.push(pts[0]);
        }
        return Ok(out);
    }
    let segmentos = if closed { n } else { n.saturating_sub(1) };
    if segmentos == 0 {
        let mut out = Vec::with_capacity(samples);
        for _ in 0..samples {
            out.push(pts[0]);
        }
        return Ok(out);
    }
    // Longitudes acumuladas (`cum[0] = 0`, `cum[k]` = largo hasta el nodo k).
    let mut cum: Vec<f64> = Vec::with_capacity(segmentos.saturating_add(1));
    cum.push(0.0);
    let mut total = 0.0_f64;
    for k in 0..segmentos {
        let (p0, p1) = if closed {
            (pts[k % n], pts[(k + 1) % n])
        } else {
            (pts[k], pts[k + 1])
        };
        let dx = p1.x - p0.x;
        let dy = p1.y - p0.y;
        let d = (dx * dx + dy * dy).sqrt();
        if d.is_finite() && d > 0.0 {
            total += d;
        }
        cum.push(total);
    }
    if !total.is_finite() || total <= 0.0 {
        // Todos los puntos coinciden: replica el primero (punto→figura).
        let mut out = Vec::with_capacity(samples);
        for _ in 0..samples {
            out.push(pts[0]);
        }
        return Ok(out);
    }
    let denom = if closed {
        samples as f64
    } else {
        (samples.saturating_sub(1)) as f64
    };
    if !denom.is_finite() || denom <= 0.0 {
        return Err(MorphError::MuestrasFueraDeRango {
            got: samples,
            max: MORPH_MAX_SAMPLES,
        });
    }
    let mut out = Vec::with_capacity(samples);
    let mut seg = 0_usize;
    for j in 0..samples {
        let objetivo = total * (j as f64) / denom;
        while seg + 1 < cum.len() && cum[seg + 1] < objetivo {
            seg += 1;
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
        let x = p0.x + (p1.x - p0.x) * u;
        let y = p0.y + (p1.y - p0.y) * u;
        if x.is_finite() && y.is_finite() {
            out.push(Point2::new(x, y));
        } else {
            out.push(p0);
        }
    }
    Ok(out)
}

/// Rota `b` (cerrada, ya remuestreada) para que su punto 0 sea el más cercano
/// al punto 0 de `a`. Evita el giro de media vuelta al interpolar.
fn alinea_cerrada(a0: Point2, b: &[Point2]) -> Vec<Point2> {
    let n = b.len();
    if n < 2 {
        return b.to_vec();
    }
    let mut mejor = 0_usize;
    let mut mejor_d = f64::INFINITY;
    for (k, p) in b.iter().enumerate() {
        let dx = p.x - a0.x;
        let dy = p.y - a0.y;
        let d = dx * dx + dy * dy;
        if d.is_finite() && d < mejor_d {
            mejor_d = d;
            mejor = k;
        }
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(b[(mejor + i) % n]);
    }
    out
}

/// Si la polilínea abierta B quedó al revés (sus extremos calzan cruzados con
/// los de A), la invierte. Comparación honesta por extremos, sin heurísticas.
fn orienta_abierta(a: &[Point2], b: &[Point2]) -> Vec<Point2> {
    if a.len() < 2 || b.len() < 2 || a.len() != b.len() {
        return b.to_vec();
    }
    let primero = 0_usize;
    let ultimo_a = a.len().saturating_sub(1);
    let ultimo_b = b.len().saturating_sub(1);
    let directo = a[primero].distance(&b[primero]) + a[ultimo_a].distance(&b[ultimo_b]);
    let cruzado = a[primero].distance(&b[ultimo_b]) + a[ultimo_a].distance(&b[primero]);
    if cruzado.is_finite() && directo.is_finite() && cruzado < directo {
        let mut rev = b.to_vec();
        rev.reverse();
        rev
    } else {
        b.to_vec()
    }
}

/// Interpola CUALQUIER par de formas 2D por correspondencia de puntos.
///
/// Remuestrea A y B a `cfg.samples` puntos, alinea el inicio si
/// `cfg.align_start` y devuelve `cfg.frames` fotogramas con el easing de
/// `cfg`. El frame 0 es A remuestreada y el último es B (salvo
/// `EaseOutBack`, que sobrepasa a propósito). Puro y acotado, sin pánicos;
/// lo incompatible es `Err`, jamás deforma en silencio.
pub fn morph_shapes(
    a: &[Point2],
    b: &[Point2],
    cfg: &MorphConfig,
) -> Result<Vec<Vec<Point2>>, MorphError> {
    if cfg.samples < 2 || cfg.samples > MORPH_MAX_SAMPLES {
        return Err(MorphError::MuestrasFueraDeRango {
            got: cfg.samples,
            max: MORPH_MAX_SAMPLES,
        });
    }
    if cfg.frames == 0 || cfg.frames > MORPH_MAX_FRAMES {
        return Err(MorphError::FramesFueraDeRango {
            got: cfg.frames,
            max: MORPH_MAX_FRAMES,
        });
    }
    valida_forma(a, "A", cfg.closed)?;
    valida_forma(b, "B", cfg.closed)?;
    let ra = resample_polyline(a, cfg.samples, cfg.closed)?;
    let mut rb = resample_polyline(b, cfg.samples, cfg.closed)?;
    if cfg.align_start && ra.len() == rb.len() && !ra.is_empty() {
        if cfg.closed {
            rb = alinea_cerrada(ra[0], &rb);
        } else {
            rb = orienta_abierta(&ra, &rb);
        }
    }
    if ra.len() != rb.len() || ra.is_empty() {
        return Err(MorphError::FormaVacia { cual: "A/B" });
    }
    let mut frames: Vec<Vec<Point2>> = Vec::with_capacity(cfg.frames);
    for fi in 0..cfg.frames {
        let s = if cfg.frames <= 1 {
            1.0
        } else {
            (fi as f64) / ((cfg.frames.saturating_sub(1)) as f64)
        };
        let e = cfg.easing.apply(s);
        let mut fila = Vec::with_capacity(ra.len());
        for (pa, pb) in ra.iter().zip(rb.iter()) {
            let x = pa.x + (pb.x - pa.x) * e;
            let y = pa.y + (pb.y - pa.y) * e;
            if x.is_finite() && y.is_finite() {
                fila.push(Point2::new(x, y));
            } else {
                fila.push(*pa);
            }
        }
        frames.push(fila);
    }
    Ok(frames)
}

// ── M4: `.animate` encadenable mínimo estilo Manim ─────────────────────────
// Builder puro sobre formas: `rotate/shift/scale/set_fill/set_opacity`
// encadenables (con alias rioplatenses `rotar/mover/escalar/relleno/opacidad`)
// que componen UN estado destino y materializan frames interpolados con
// easing vía `morph_shapes`/`MorphEasing` (la misma cuenta del morph general,
// sin duplicarla).
//
// Seam elegido (menor): vive acá, en el cerebro puro, sin UI nueva. Se usa
// desde `ParametricAnim`/playlist o comando tomando `puntos()` y dibujando
// cada `Vec<Point2>` como polilínea con el renderer existente
// (`draw_curve_gaps` en `grafito-app/src/anim_native.rs`); ningún panel ni
// comando nuevo fue necesario para el mínimo.
//
// Semántica Manim: cada eslabón compone el destino (`rotate` y `scale`
// pivotan sobre el centroide actual del destino, `shift` traslada); la
// interpolación es inicio→destino con el easing dado. Entradas no finitas
// (`NaN`/`inf`) o `scale <= 0` son no-op honestas (el encadenado sigue
// infalible); lo estructural (vacío, no finito en el origen, cerrada con 2)
// es `Err` en `nuevo`/`fotogramas`, jamás deforma en silencio.
// Presupuestos intactos: `samples` 2..=512, `frames` 1..=48, tope de bytes
// vía `MorphConfig::try_new`. Sin I/O, sin `unwrap` en prod.

/// Estilo interpolable de una forma (canales RGBA 0..=255 + opacidad 0..1).
///
/// `relleno` y `opacidad` son canales independientes (Manim `set_fill` /
/// `opacity`): el renderer los combina como
/// `alfa_efectivo = relleno[3]/255 · opacidad`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstiloForma {
    /// Color de relleno RGBA (cada canal 0..=255).
    pub relleno: [u8; 4],
    /// Opacidad global 0..=1 (1.0 = opaco).
    pub opacidad: f32,
}

impl Default for EstiloForma {
    fn default() -> Self {
        Self {
            relleno: [255, 255, 255, 255],
            opacidad: 1.0,
        }
    }
}

/// Un fotograma animado: puntos interpolados + estilo interpolado.
#[derive(Debug, Clone, PartialEq)]
pub struct FotogramaForma {
    /// Puntos del frame (largo = `samples` del builder).
    pub puntos: Vec<Point2>,
    /// Relleno interpolado del frame.
    pub relleno: [u8; 4],
    /// Opacidad interpolada del frame (0..=1).
    pub opacidad: f32,
}

/// Builder `.animate` mínimo sobre una forma 2D.
#[derive(Debug, Clone)]
pub struct AnimarForma {
    inicio: Vec<Point2>,
    destino: Vec<Point2>,
    relleno_ini: [u8; 4],
    relleno_fin: [u8; 4],
    opacidad_ini: f32,
    opacidad_fin: f32,
    samples: usize,
    frames: usize,
    closed: bool,
    align_start: bool,
    easing: MorphEasing,
}

/// Centroide (promedio) de puntos finitos; `(0,0)` si no hay ninguno.
///
/// Los constructores ya rechazan lo no finito, así que el fallback solo
/// cubre el imposible defensivo. Puro, sin pánicos.
fn centroide(pts: &[Point2]) -> Point2 {
    let mut sx = 0.0_f64;
    let mut sy = 0.0_f64;
    let mut n = 0_usize;
    for p in pts {
        if p.x.is_finite() && p.y.is_finite() {
            sx += p.x;
            sy += p.y;
            n += 1;
        }
    }
    if n == 0 {
        return Point2::new(0.0, 0.0);
    }
    let x = sx / (n as f64);
    let y = sy / (n as f64);
    if x.is_finite() && y.is_finite() {
        Point2::new(x, y)
    } else {
        Point2::new(0.0, 0.0)
    }
}

/// Interpola un canal 0..=255 con el easing ya evaluado `e` (clamp honesto:
// `EaseOutBack` sobrepasa a propósito en geometría, en color se recorta).
fn lerp_canal(a: u8, b: u8, e: f64) -> u8 {
    let v = f64::from(a) + (f64::from(b) - f64::from(a)) * e;
    if !v.is_finite() {
        return a;
    }
    v.round().clamp(0.0, 255.0) as u8
}

/// Interpola la opacidad 0..=1 con el easing ya evaluado `e` (clamp honesto).
fn lerp_opacidad(a: f32, b: f32, e: f64) -> f32 {
    let v = f64::from(a) + (f64::from(b) - f64::from(a)) * e;
    if !v.is_finite() {
        return a;
    }
    v.clamp(0.0, 1.0) as f32
}

impl AnimarForma {
    /// Arranca el builder desde una forma (copia inicio y destino).
    ///
    /// Valida como abierta (el flag `closed` se elige después con
    /// `closed()`; si la cerrás con 2 puntos, `fotogramas` falla honesto).
    pub fn nuevo(forma: &[Point2]) -> Result<Self, MorphError> {
        valida_forma(forma, "A", false)?;
        Ok(Self {
            inicio: forma.to_vec(),
            destino: forma.to_vec(),
            relleno_ini: EstiloForma::default().relleno,
            relleno_fin: EstiloForma::default().relleno,
            opacidad_ini: 1.0,
            opacidad_fin: 1.0,
            samples: MORPH_DEFAULT_SAMPLES,
            frames: MORPH_DEFAULT_FRAMES,
            closed: false,
            align_start: true,
            easing: MorphEasing::default(),
        })
    }

    /// `true` = polígono cerrado (se recorre con vuelta).
    pub fn closed(mut self, v: bool) -> Self {
        self.closed = v;
        self
    }

    /// Puntos remuestreados por frame (2..=512; se valida en `fotogramas`).
    pub fn samples(mut self, s: usize) -> Self {
        self.samples = s;
        self
    }

    /// Fotogramas del set (1..=48; se valida en `fotogramas`).
    pub fn frames(mut self, n: usize) -> Self {
        self.frames = n;
        self
    }

    /// Curva de easing entre frames.
    pub fn easing(mut self, e: MorphEasing) -> Self {
        self.easing = e;
        self
    }

    /// `true` = alinear el inicio de B con el de A (rotar/invertir).
    pub fn align_start(mut self, v: bool) -> Self {
        self.align_start = v;
        self
    }

    /// Rota el destino `grados` antihorarios sobre su centroide actual.
    ///
    /// `grados` no finito = no-op (el encadenado sigue infalible).
    pub fn rotate(mut self, grados: f64) -> Self {
        if !grados.is_finite() {
            return self;
        }
        let rad = grados.to_radians();
        if !rad.is_finite() {
            return self;
        }
        let (co, si) = (rad.cos(), rad.sin());
        if !co.is_finite() || !si.is_finite() {
            return self;
        }
        let c = centroide(&self.destino);
        for p in &mut self.destino {
            let dx = p.x - c.x;
            let dy = p.y - c.y;
            let nx = c.x + dx * co - dy * si;
            let ny = c.y + dx * si + dy * co;
            if nx.is_finite() && ny.is_finite() {
                p.x = nx;
                p.y = ny;
            }
        }
        self
    }

    /// Traslada el destino `(dx, dy)` en mundo.
    ///
    /// Algún componente no finito = no-op.
    pub fn shift(mut self, dx: f64, dy: f64) -> Self {
        if !dx.is_finite() || !dy.is_finite() {
            return self;
        }
        for p in &mut self.destino {
            let nx = p.x + dx;
            let ny = p.y + dy;
            if nx.is_finite() && ny.is_finite() {
                p.x = nx;
                p.y = ny;
            }
        }
        self
    }

    /// Escala el destino `factor` sobre su centroide actual.
    ///
    /// `factor` no finito o `<= 0` = no-op (no se espeja ni colapsa en
    /// silencio: pedí el factor válido de nuevo).
    pub fn scale(mut self, factor: f64) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self;
        }
        let c = centroide(&self.destino);
        for p in &mut self.destino {
            let nx = c.x + (p.x - c.x) * factor;
            let ny = c.y + (p.y - c.y) * factor;
            if nx.is_finite() && ny.is_finite() {
                p.x = nx;
                p.y = ny;
            }
        }
        self
    }

    /// Fija el relleno destino (el frame 0 usa el default neutro).
    pub fn set_fill(mut self, rgba: [u8; 4]) -> Self {
        self.relleno_fin = rgba;
        self
    }

    /// Fija la opacidad destino 0..=1 (fuera de rango se clampe; no finita
    /// = no-op).
    pub fn set_opacity(mut self, alpha: f32) -> Self {
        if !alpha.is_finite() {
            return self;
        }
        self.opacidad_fin = alpha.clamp(0.0, 1.0);
        self
    }

    /// Alias rioplatense de `rotate`.
    pub fn rotar(self, grados: f64) -> Self {
        self.rotate(grados)
    }

    /// Alias rioplatense de `shift`.
    pub fn mover(self, dx: f64, dy: f64) -> Self {
        self.shift(dx, dy)
    }

    /// Alias rioplatense de `scale`.
    pub fn escalar(self, factor: f64) -> Self {
        self.scale(factor)
    }

    /// Alias rioplatense de `set_fill`.
    pub fn relleno(self, rgba: [u8; 4]) -> Self {
        self.set_fill(rgba)
    }

    /// Alias rioplatense de `set_opacity`.
    pub fn opacidad(self, alpha: f32) -> Self {
        self.set_opacity(alpha)
    }

    /// Estado destino compuesto hasta ahora (solo lectura).
    pub fn objetivo(&self) -> &[Point2] {
        &self.destino
    }

    /// Estilo destino compuesto hasta ahora.
    pub fn estilo_objetivo(&self) -> EstiloForma {
        EstiloForma {
            relleno: self.relleno_fin,
            opacidad: self.opacidad_fin,
        }
    }

    /// Materializa los frames: geometría vía `morph_shapes` + estilo con el
    /// mismo easing. Todo `Err` es honesto (presupuesto, cerrada con 2,
    /// formas incompatibles); jamás deforma en silencio.
    pub fn fotogramas(&self) -> Result<Vec<FotogramaForma>, MorphError> {
        let cfg = MorphConfig::try_new(
            self.samples,
            self.frames,
            self.closed,
            self.align_start,
            self.easing,
        )?;
        let sets = morph_shapes(&self.inicio, &self.destino, &cfg)?;
        let n = sets.len();
        let mut out = Vec::with_capacity(n);
        for (fi, fila) in sets.into_iter().enumerate() {
            let s = if n <= 1 {
                1.0
            } else {
                (fi as f64) / ((n.saturating_sub(1)) as f64)
            };
            let e = self.easing.apply(s);
            let mut rel = [0_u8; 4];
            for (k, slot) in rel.iter_mut().enumerate() {
                if let (Some(a), Some(b)) = (self.relleno_ini.get(k), self.relleno_fin.get(k)) {
                    *slot = lerp_canal(*a, *b, e);
                }
            }
            out.push(FotogramaForma {
                puntos: fila,
                relleno: rel,
                opacidad: lerp_opacidad(self.opacidad_ini, self.opacidad_fin, e),
            });
        }
        Ok(out)
    }

    /// Atajo inglés de `fotogramas` (paridad Manim en el nombre).
    pub fn build(&self) -> Result<Vec<FotogramaForma>, MorphError> {
        self.fotogramas()
    }

    /// Solo los puntos por frame (para dibujar como polilínea con el
    /// renderer existente, sin estilo).
    pub fn puntos(&self) -> Result<Vec<Vec<Point2>>, MorphError> {
        let frames = self.fotogramas()?;
        Ok(frames.into_iter().map(|f| f.puntos).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuadrada() -> Vec<Point2> {
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ]
    }

    fn triangulo() -> Vec<Point2> {
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
        ]
    }

    #[test]
    fn config_acota_muestras_frames_y_presupuesto() {
        assert!(MorphConfig::try_new(1, 4, false, true, MorphEasing::Linear).is_err());
        assert!(MorphConfig::try_new(513, 4, false, true, MorphEasing::Linear).is_err());
        assert!(MorphConfig::try_new(8, 0, false, true, MorphEasing::Linear).is_err());
        assert!(MorphConfig::try_new(8, 49, false, true, MorphEasing::Linear).is_err());
        assert!(MorphConfig::try_new(2, 1, false, true, MorphEasing::Linear).is_ok());
        assert!(MorphConfig::try_new(512, 48, false, true, MorphEasing::Linear).is_ok());
    }

    #[test]
    fn formas_incompatibles_fallan_honesto() {
        let cfg = MorphConfig::try_new(8, 4, false, true, MorphEasing::Linear).unwrap();
        let vacia: Vec<Point2> = vec![];
        assert_eq!(
            morph_shapes(&vacia, &cuadrada(), &cfg),
            Err(MorphError::FormaVacia { cual: "A" })
        );
        assert_eq!(
            morph_shapes(&cuadrada(), &vacia, &cfg),
            Err(MorphError::FormaVacia { cual: "B" })
        );
        // Cerrada con 2 puntos: ni polígono ni punto.
        let dos = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
        let cfg_c = MorphConfig::try_new(8, 4, true, true, MorphEasing::Linear).unwrap();
        assert_eq!(
            morph_shapes(&dos, &cuadrada(), &cfg_c),
            Err(MorphError::CerradaNecesitaTres { cual: "A", got: 2 })
        );
        // No finito.
        let nan = vec![Point2::new(f64::NAN, 0.0), Point2::new(1.0, 0.0)];
        assert!(morph_shapes(&nan, &cuadrada(), &cfg).is_err());
        // Gigante (>cap).
        let big = vec![Point2::new(0.0, 0.0); MORPH_MAX_INPUT_POINTS + 1];
        assert!(morph_shapes(&big, &cuadrada(), &cfg).is_err());
    }

    #[test]
    fn extremos_son_a_y_b_con_easing_lineal() {
        let cfg = MorphConfig::try_new(16, 5, false, true, MorphEasing::Linear).unwrap();
        let frames = morph_shapes(&cuadrada(), &triangulo(), &cfg).unwrap();
        assert_eq!(frames.len(), 5);
        assert!(frames.iter().all(|f| f.len() == 16));
        // Frame 0 ≈ A remuestreada, último ≈ B remuestreada.
        let ra = resample_polyline(&cuadrada(), 16, false).unwrap();
        let rb = resample_polyline(&triangulo(), 16, false).unwrap();
        for (p, q) in frames[0].iter().zip(ra.iter()) {
            assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9);
        }
        for (p, q) in frames[4].iter().zip(rb.iter()) {
            assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9);
        }
        // Punto medio con linear: promedio exacto.
        for ((p, pa), pb) in frames[2].iter().zip(ra.iter()).zip(rb.iter()) {
            assert!((p.x - (pa.x + pb.x) * 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn punto_a_figura_crece_desde_el_punto() {
        let punto = vec![Point2::new(0.5, 0.5)];
        let cfg = MorphConfig::try_new(8, 3, false, true, MorphEasing::Linear).unwrap();
        let frames = morph_shapes(&punto, &cuadrada(), &cfg).unwrap();
        assert_eq!(frames.len(), 3);
        // Frame 0: todo el punto replicado.
        assert!(frames[0]
            .iter()
            .all(|p| (p.x - 0.5).abs() < 1e-12 && (p.y - 0.5).abs() < 1e-12));
        // Último: la figura remuestreada.
        let rb = resample_polyline(&cuadrada(), 8, false).unwrap();
        for (p, q) in frames[2].iter().zip(rb.iter()) {
            assert!((p.x - q.x).abs() < 1e-9);
        }
    }

    #[test]
    fn cerrada_alinea_inicio_por_punto_mas_cercano() {
        // Mismo cuadrado pero B arranca en otro vértice: con align se rota.
        let b_rotada = vec![
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
        ];
        let cfg_on = MorphConfig::try_new(4, 2, true, true, MorphEasing::Linear).unwrap();
        let cfg_off = MorphConfig::try_new(4, 2, true, false, MorphEasing::Linear).unwrap();
        let f_on = morph_shapes(&cuadrada(), &b_rotada, &cfg_on).unwrap();
        let f_off = morph_shapes(&cuadrada(), &b_rotada, &cfg_off).unwrap();
        // Con alineación, el último frame arranca en (0,0) como A.
        assert!((f_on[1][0].x).abs() < 1e-9 && (f_on[1][0].y).abs() < 1e-9);
        // Sin alineación conserva el inicio rotado (1,1).
        assert!((f_off[1][0].x - 1.0).abs() < 1e-9 && (f_off[1][0].y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn abierta_invierte_b_si_viene_al_reves() {
        let a = vec![Point2::new(0.0, 0.0), Point2::new(2.0, 0.0)];
        let b_rev = vec![Point2::new(3.0, 1.0), Point2::new(1.0, 1.0)];
        let cfg = MorphConfig::try_new(2, 2, false, true, MorphEasing::Linear).unwrap();
        let frames = morph_shapes(&a, &b_rev, &cfg).unwrap();
        // Orientada: primer punto de B interpolado queda cerca de x=1.
        assert!((frames[1][0].x - 1.0).abs() < 1e-9);
    }

    #[test]
    fn resample_uniforme_por_longitud_de_arco() {
        // Segmento 0→4 en dos tramos (0→1, 1→4): el punto medio del
        // remuestreo cae en x=2, no en el vértice x=1.
        let a = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
        let b = vec![Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)];
        let r = resample_polyline(&b, 3, false).unwrap();
        assert!((r[1].x - 2.0).abs() < 1e-9, "r[1]={:?}", r[1]);
        let _ = a;
    }

    #[test]
    fn easings_coinciden_con_piel_y_son_finitos() {
        // Extremos fijos en los 8 (salvo el rebote, que arranca/termina igual).
        for e in [
            MorphEasing::Linear,
            MorphEasing::QuadraticIn,
            MorphEasing::QuadraticOut,
            MorphEasing::CubicIn,
            MorphEasing::CubicOut,
            MorphEasing::CubicInOut,
            MorphEasing::SinInOut,
            MorphEasing::EaseOutBack,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-12, "{:?}", e);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9, "{:?}", e);
            assert!((e.apply(0.5) - 0.5).abs() < 0.6, "{:?}", e);
            assert!(e.apply(f64::NAN).is_finite());
            assert!(e.apply(f64::INFINITY).is_finite());
        }
        // Valores conocidos estilo piel.
        assert!((MorphEasing::QuadraticIn.apply(0.5) - 0.25).abs() < 1e-12);
        assert!((MorphEasing::QuadraticOut.apply(0.5) - 0.75).abs() < 1e-12);
        assert!((MorphEasing::CubicIn.apply(0.5) - 0.125).abs() < 1e-12);
        assert!((MorphEasing::SinInOut.apply(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(
            MorphEasing::from_name("cubic_in_out"),
            Some(MorphEasing::CubicInOut)
        );
        assert_eq!(MorphEasing::from_name("??"), None);
        assert_eq!(MorphEasing::CubicInOut.as_str(), "cubic_in_out");
    }

    #[test]
    fn prosa_rioplatense_en_errores() {
        let e = MorphError::FormaVacia { cual: "A" };
        assert!(e.to_string().contains("vacía"));
        let e2 = MorphError::MuestrasFueraDeRango { got: 999, max: 512 };
        assert!(e2.to_string().contains("999"));
    }

    // ── M4: `.animate` encadenable ──────────────────────────────────────
    fn centroide_y(frames: &[FotogramaForma]) -> Vec<f64> {
        frames
            .iter()
            .map(|f| {
                let n = f.puntos.len() as f64;
                f.puntos.iter().map(|p| p.y).sum::<f64>() / n
            })
            .collect()
    }

    #[test]
    fn animate_rotate_shift_da_frames_monotonos() {
        // Pedido M4 textual: `cuadrado.rotate(45°).shift(arriba)`.
        let frames = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .closed(true)
            .samples(8)
            .frames(5)
            .easing(MorphEasing::Linear)
            .rotate(45.0)
            .shift(0.0, 1.0)
            .fotogramas()
            .unwrap();
        assert_eq!(frames.len(), 5);
        assert!(frames.iter().all(|f| f.puntos.len() == 8));
        // El centroide sube 0.5 → 1.5 monótono (rotar no mueve el centroide,
        // trasladar +1 en y sí; easing lineal = pasos iguales).
        let ys = centroide_y(&frames);
        assert!((ys[0] - 0.5).abs() < 1e-6, "arranca en 0.5, got {}", ys[0]);
        assert!((ys[4] - 1.5).abs() < 1e-6, "termina en 1.5, got {}", ys[4]);
        for w in ys.windows(2) {
            assert!(w[1] > w[0], "monótono creciente: {ys:?}");
            assert!((w[1] - w[0] - 0.25).abs() < 1e-6, "paso lineal: {ys:?}");
        }
        // El destino compuesto es el cuadrado rotado + trasladado.
        let obj = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .closed(true)
            .rotate(45.0)
            .shift(0.0, 1.0)
            .objetivo()
            .to_vec();
        let c = centroide(&obj);
        assert!((c.x - 0.5).abs() < 1e-9 && (c.y - 1.5).abs() < 1e-9);
    }

    #[test]
    fn animate_escala_relleno_opacidad_encadenan() {
        let frames = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .samples(8)
            .frames(4)
            .easing(MorphEasing::Linear)
            .scale(2.0)
            .set_fill([255, 0, 0, 255])
            .set_opacity(0.2)
            .fotogramas()
            .unwrap();
        assert_eq!(frames.len(), 4);
        // Opacidad 1.0 → 0.2 monótona decreciente.
        assert!((frames[0].opacidad - 1.0).abs() < 1e-6);
        assert!((frames[3].opacidad - 0.2).abs() < 1e-6);
        for w in frames.windows(2) {
            assert!(w[1].opacidad < w[0].opacidad);
        }
        // Relleno neutro → rojo.
        assert_eq!(frames[0].relleno, [255, 255, 255, 255]);
        assert_eq!(frames[3].relleno, [255, 0, 0, 255]);
        // Escalar ×2 duplica el ancho del destino aprox.
        let obj = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .scale(2.0)
            .objetivo()
            .to_vec();
        let min_x = obj.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = obj.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        assert!((max_x - min_x - 2.0).abs() < 1e-9);
        // Alias rioplatenses dan lo mismo.
        let a = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .rotar(30.0)
            .mover(1.0, 2.0)
            .escalar(1.5)
            .objetivo()
            .to_vec();
        let b = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .rotate(30.0)
            .shift(1.0, 2.0)
            .scale(1.5)
            .objetivo()
            .to_vec();
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert!((pa.x - pb.x).abs() < 1e-12 && (pa.y - pb.y).abs() < 1e-12);
        }
    }

    #[test]
    fn animate_entradas_raras_no_rompen_y_falla_honesto() {
        // No finitos / escala inválida = no-op, el set sale igual al quieto.
        let quieto = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .samples(8)
            .frames(3)
            .fotogramas()
            .unwrap();
        let raro = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .samples(8)
            .frames(3)
            .rotate(f64::NAN)
            .shift(f64::INFINITY, 0.0)
            .scale(0.0)
            .scale(-2.0)
            .scale(f64::NAN)
            .set_opacity(f32::NAN)
            .fotogramas()
            .unwrap();
        assert_eq!(quieto.len(), raro.len());
        for (a, b) in quieto.iter().zip(raro.iter()) {
            assert_eq!(a.puntos.len(), b.puntos.len());
            for (pa, pb) in a.puntos.iter().zip(b.puntos.iter()) {
                assert!((pa.x - pb.x).abs() < 1e-12 && (pa.y - pb.y).abs() < 1e-12);
            }
        }
        // Cerrada con 2 puntos falla honesto (no cierra en silencio).
        let dos = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
        let err = AnimarForma::nuevo(&dos)
            .unwrap()
            .closed(true)
            .fotogramas()
            .unwrap_err();
        assert_eq!(err, MorphError::CerradaNecesitaTres { cual: "A", got: 2 });
        // Vacía y gigante también.
        assert!(AnimarForma::nuevo(&[]).is_err());
        let big = vec![Point2::new(0.0, 0.0); MORPH_MAX_INPUT_POINTS + 1];
        assert!(AnimarForma::nuevo(&big).is_err());
        // `puntos()` sirve al renderer de polilíneas existente.
        let pts = AnimarForma::nuevo(&cuadrada())
            .unwrap()
            .samples(8)
            .frames(3)
            .rotate(10.0)
            .puntos()
            .unwrap();
        assert_eq!(pts.len(), 3);
        assert!(pts.iter().all(|f| f.len() == 8));
    }
}
