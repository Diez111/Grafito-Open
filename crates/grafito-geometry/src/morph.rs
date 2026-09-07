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
}
