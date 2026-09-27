//! Escenas de álgebra lineal nivel 3Blue1Brown (Essence of Linear Algebra).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! (solo `std`). Cada plantilla es una escena didáctica con setup +
//! animación + etiqueta, en la paleta oscura de 3b1b (fondo azul noche,
//! grilla tenue, vectores ámbar/verde/rojo).
//!
//! ## Escenas (ver [`TEMPLATE_IDS`])
//!
//! | Id | Capítulo 3b1b | Idea |
//! |---|---|---|
//! | `vectores-combinacion-lineal` | Cap. 1–2 (vectores, span) | `s·v + t·w` barriendo la recta `s·v + 0.6·w` |
//! | `matriz-transformacion` | Cap. 3 (matrices como transformaciones) | la grilla se deforma con `M(t)` de `I` a `A` |
//! | `determinante-area` | Cap. 6 (determinante) | el cuadrado unidad se vuelve paralelogramo de área `det` |
//! | `eigenvectores` | Cap. 14 (eigenvectores) | direcciones que quedan en su span (`Av = λv`) |
//! | `cambio-de-base` | Cap. 13 (cambio de base) | misma flecha, grilla de Jennifer, otras coordenadas |
//! | `producto-cruz` | Cap. 10–11 (producto cruz) | `|v×w|` como área con signo del paralelogramo |
//!
//! Fuentes: lecciones de 3Blue1Brown (`3blue1brown.com/lessons/`):
//! `span`, `linear-transformations`, `determinant`, `eigenvalues`,
//! `change-of-basis`, `cross-products`.
//!
//! ## Formato de frame
//!
//! [`RgbaFrame`] es RGBA 8 bits por canal, fila-mayor, origen arriba-izq
//! (igual que el `buf` que `anim_native` mete en
//! `egui::ColorImage::from_rgba_unmultiplied`). La app cablea copiando
//! `pixels` tal cual; acá no se depende de egui a propósito.
//!
//! ## Presupuestos (paridad con `protocol.rs`, sin importarlo: este módulo
//! es autocontenido y compila con `rustc --test` standalone)
//!
//! - Frames por set: 1..=64 en preview ([`LINALG_MAX_FRAMES`], paridad
//!   `PREVIEW_SHORT_MAX_FRAMES`), hasta 1500 en largo
//!   ([`LINALG_LONGFORM_MAX_FRAMES`], 50 s a 30 fps ≤ 60 s del timeline).
//! - Set en RAM ≤ 64 MiB ([`LINALG_CHUNK_MAX_BYTES`]): si excede es `Err`
//!   ([`LinalgError::SetDemasiadoGrande`]) y el frente compone por rangos
//!   con [`render_linalg_rango`] drenando a disco.
//! - Lienzo 64..=4096 por lado (paridad `Resolution`).
//! - Samplers sin allocs por frame: la matemática por frame devuelve
//!   `Copy` o escribe en buffers del llamador; [`Scratch`] se prepara una
//!   vez en el setup y se muestrea por índice. La única alloc por frame es
//!   el propio píxel-buffer de salida (inherente: cada frame es dueño).

// ── Ids estables ──────────────────────────────────────────────────────────

/// Ids estables (kebab-case) de las 6 plantillas de álgebra lineal.
/// El frente los cablea al dispatcher nativo; acá ya los atiende
/// [`render_linalg_frames`].
pub const TEMPLATE_IDS: &[&str] = &[
    "vectores-combinacion-lineal",
    "matriz-transformacion",
    "determinante-area",
    "eigenvectores",
    "cambio-de-base",
    "producto-cruz",
];

// ── Presupuestos ──────────────────────────────────────────────────────────

/// Ancho default del set (paridad con el canónico del chat 480×360).
pub const LINALG_DEFAULT_W: u32 = 480;
/// Alto default del set.
pub const LINALG_DEFAULT_H: u32 = 360;
/// Frames default por escena (paridad `NATIVE_ANIM_FRAME_COUNT`).
pub const LINALG_DEFAULT_FRAMES: usize = 48;
/// Frames mínimos por pedido.
pub const LINALG_MIN_FRAMES: usize = 1;
/// Tope de frames en preview (paridad `PREVIEW_SHORT_MAX_FRAMES`).
pub const LINALG_MAX_FRAMES: usize = 64;
/// Tope de frames en largo (paridad `VIDEO_LONGFORM_MAX_FRAMES`: 50 s a 30 fps).
pub const LINALG_LONGFORM_MAX_FRAMES: usize = 1500;
/// Tope de bytes RGBA del set en RAM (paridad `LONGFORM_CHUNK_MAX_BYTES`).
pub const LINALG_CHUNK_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Lado mínimo del lienzo (paridad `Resolution`).
pub const LINALG_CANVAS_MIN: u32 = 64;
/// Lado máximo del lienzo (paridad `Resolution`).
pub const LINALG_CANVAS_MAX: u32 = 4096;
/// Bytes por píxel RGBA.
pub const LINALG_BYTES_POR_PIXEL: usize = 4;

// ── Error ─────────────────────────────────────────────────────────────────

/// Error honesto de las escenas (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinalgError {
    /// Lienzo fuera de 64..=4096 por lado (o cero).
    LienzoInvalido { w: u32, h: u32 },
    /// Conteo de frames fuera de 1..=1500.
    FramesFueraDeRango { got: usize },
    /// Plantilla que no es de este módulo.
    PlantillaDesconocida { got: String },
    /// El set excede 64 MiB: bajar frames/lienzo o componer por rangos.
    SetDemasiadoGrande { bytes: usize },
}

impl std::fmt::Display for LinalgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LienzoInvalido { w, h } => write!(
                f,
                "lienzo {w}x{h} inválido: usá {LINALG_CANVAS_MIN}..={LINALG_CANVAS_MAX} por lado"
            ),
            Self::FramesFueraDeRango { got } => write!(
                f,
                "{got} frames fuera de {LINALG_MIN_FRAMES}..={LINALG_LONGFORM_MAX_FRAMES}: \
                 preview hasta {LINALG_MAX_FRAMES}, largo por rangos"
            ),
            Self::PlantillaDesconocida { got } => write!(
                f,
                "plantilla {got:?} desconocida: elegí una de {TEMPLATE_IDS:?}"
            ),
            Self::SetDemasiadoGrande { bytes } => write!(
                f,
                "set de {bytes} bytes excede {LINALG_CHUNK_MAX_BYTES}: \
                 bajá frames/lienzo o componé por rangos con render_linalg_rango"
            ),
        }
    }
}

impl std::error::Error for LinalgError {}

// ── Frame RGBA ────────────────────────────────────────────────────────────

/// Un frame RGBA dueño de sus píxeles (fila-mayor, origen arriba-izq).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    /// Ancho en px.
    pub width: u32,
    /// Alto en px.
    pub height: u32,
    /// `width*height*4` bytes RGBA.
    pub pixels: Vec<u8>,
}

impl RgbaFrame {
    /// Constructor validado (lienzo 64..=4096; `Err` honesto si no).
    pub fn nuevo(width: u32, height: u32) -> Result<Self, LinalgError> {
        valida_lienzo(width, height)?;
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(LINALG_BYTES_POR_PIXEL));
        match len {
            Some(n) => Ok(Self {
                width,
                height,
                pixels: vec![0u8; n],
            }),
            None => Err(LinalgError::SetDemasiadoGrande { bytes: usize::MAX }),
        }
    }

    /// Píxel en `(x, y)` (`None` honesto si está fuera).
    pub fn pixel_en(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(LINALG_BYTES_POR_PIXEL)?;
        let p = self.pixels.get(i..i + LINALG_BYTES_POR_PIXEL)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// Lienzo válido 64..=4096 por lado. Puro.
fn valida_lienzo(w: u32, h: u32) -> Result<(), LinalgError> {
    if !(LINALG_CANVAS_MIN..=LINALG_CANVAS_MAX).contains(&w)
        || !(LINALG_CANVAS_MIN..=LINALG_CANVAS_MAX).contains(&h)
    {
        return Err(LinalgError::LienzoInvalido { w, h });
    }
    Ok(())
}

// ── Matemática pura (samplers: Copy o buffers del llamador, sin allocs) ───

/// Matriz 2×2 por columnas: `i-hat → (a, c)`, `j-hat → (b, d)`.
/// (Como en 3b1b cap. 3: la transformación queda descrita por a dónde van
/// los dos vectores base.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat2 {
    /// Fila 0, columna 0 (x de `i-hat`).
    pub a: f64,
    /// Fila 0, columna 1 (x de `j-hat`).
    pub b: f64,
    /// Fila 1, columna 0 (y de `i-hat`).
    pub c: f64,
    /// Fila 1, columna 1 (y de `j-hat`).
    pub d: f64,
}

impl Mat2 {
    /// Identidad (grilla sin deformar).
    pub const IDENTIDAD: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
    };

    /// Constructor (guarda finita: `NaN`/inf → 0 honesto, sin pánico).
    pub fn nuevo(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self {
            a: finito_o_cero(a),
            b: finito_o_cero(b),
            c: finito_o_cero(c),
            d: finito_o_cero(d),
        }
    }

    /// Aplica la matriz a un punto (`A·p`); salida no finita → `[0, 0]`.
    pub fn aplica(self, p: [f64; 2]) -> [f64; 2] {
        let x = self.a * p[0] + self.b * p[1];
        let y = self.c * p[0] + self.d * p[1];
        if x.is_finite() && y.is_finite() {
            [x, y]
        } else {
            [0.0, 0.0]
        }
    }

    /// Determinante `a·d − b·c` (factor de escala de áreas, cap. 6).
    /// No finito → 0 honesto.
    pub fn det(self) -> f64 {
        let v = self.a * self.d - self.b * self.c;
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }

    /// Interpolación lineal `self → otro` con `t` clamp 0..1 (el morph de
    /// la grilla entre identidad y `A`). Pura.
    pub fn interpola(self, otro: Self, t: f64) -> Self {
        let u = t.clamp(0.0, 1.0);
        let u = if u.is_finite() { u } else { 0.0 };
        Self {
            a: self.a + (otro.a - self.a) * u,
            b: self.b + (otro.b - self.b) * u,
            c: self.c + (otro.c - self.c) * u,
            d: self.d + (otro.d - self.d) * u,
        }
    }
}

/// Finito o cero (guarda de los samplers). Puro.
fn finito_o_cero(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Smoothstep `3t²−2t³` (paridad `anims::smooth` / `RateFunc::Smooth`).
/// Clamp 0..1 + guardia finita. Puro.
pub fn suave(t: f64) -> f64 {
    let u = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    u * u * (3.0 - 2.0 * u)
}

/// Tramo de timeline: 0 antes de `a`, 1 después de `b`, rampa lineal
/// entremedio (`a == b` → escalón honesto, sin división por cero). Puro.
pub fn tramo(t: f64, a: f64, b: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    if b <= a {
        return if t < a { 0.0 } else { 1.0 };
    }
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

/// Progreso global 0..1 del frame `indice` en un set de `total`
/// (`total <= 1` → 0; índice pasado → clamp, sin pánicos). Puro.
pub fn progreso_en(indice: usize, total: usize) -> f64 {
    if total <= 1 {
        return 0.0;
    }
    let i = indice.min(total - 1) as f64;
    let n = (total - 1) as f64;
    if n > 0.0 {
        (i / n).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Producto cruz 2D con signo `v.x·w.y − v.y·w.x` (= área con signo del
/// paralelogramo; cap. 10: positivo si `w` está a la izq. de `v`). Puro.
pub fn cruz2(v: [f64; 2], w: [f64; 2]) -> f64 {
    let r = v[0] * w[1] - v[1] * w[0];
    if r.is_finite() {
        r
    } else {
        0.0
    }
}

/// Coordenadas de `v` en la base `(b1, b2)` por Cramer.
/// `None` honesto si la base es degenerada (`det ≈ 0`) o no finita. Puro.
pub fn coords_en_base(b1: [f64; 2], b2: [f64; 2], v: [f64; 2]) -> Option<[f64; 2]> {
    if !(b1[0].is_finite()
        && b1[1].is_finite()
        && b2[0].is_finite()
        && b2[1].is_finite()
        && v[0].is_finite()
        && v[1].is_finite())
    {
        return None;
    }
    let det = b1[0] * b2[1] - b2[0] * b1[1];
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    let c1 = (v[0] * b2[1] - b2[0] * v[1]) / det;
    let c2 = (b1[0] * v[1] - v[0] * b1[1]) / det;
    if c1.is_finite() && c2.is_finite() {
        Some([c1, c2])
    } else {
        None
    }
}

// ── Presupuestos puros ────────────────────────────────────────────────────

/// Bytes RGBA del set (`w·h·4·frames`); `None` si desborda. Puro, sin allocs.
pub fn estima_bytes(w: u32, h: u32, frames: usize) -> Option<usize> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(LINALG_BYTES_POR_PIXEL))
        .and_then(|v| v.checked_mul(frames))
}

/// ¿Cuántos frames de `w`×`h` entran en 64 MiB? Lados 0 o desborde → 0
/// honesto. Puro. (Pineado: 1280×720→18, 640×480→54, como en el protocolo.)
pub fn frames_por_chunk(w: u32, h: u32) -> usize {
    let por_frame = (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(LINALG_BYTES_POR_PIXEL))
        .unwrap_or(0);
    if por_frame == 0 {
        return 0;
    }
    LINALG_CHUNK_MAX_BYTES / por_frame
}

/// Frames para una duración a `fps` (`duracion_ms·fps/1000`, saturado;
/// `fps` 0 → 0 honesto). Puro. (Pineado: 50 s a 30 fps = 1500.)
pub fn frames_para_duracion(duracion_ms: u64, fps: u32) -> u64 {
    if fps == 0 {
        return 0;
    }
    duracion_ms.saturating_mul(u64::from(fps)) / 1000
}

/// Valida un pedido completo (lienzo + 1..=1500 frames + set ≤ 64 MiB).
pub fn valida_pedido(w: u32, h: u32, frames: usize) -> Result<(), LinalgError> {
    valida_lienzo(w, h)?;
    if !(LINALG_MIN_FRAMES..=LINALG_LONGFORM_MAX_FRAMES).contains(&frames) {
        return Err(LinalgError::FramesFueraDeRango { got: frames });
    }
    match estima_bytes(w, h, frames) {
        Some(n) if n <= LINALG_CHUNK_MAX_BYTES => Ok(()),
        Some(n) => Err(LinalgError::SetDemasiadoGrande { bytes: n }),
        None => Err(LinalgError::SetDemasiadoGrande { bytes: usize::MAX }),
    }
}

// ── Scratch reutilizable (setup una vez, muestreo sin allocs) ─────────────

/// Buffer reutilizable del sampler: la recta `base + s·dir` (`s ∈ [−1, 1]`)
/// se precalcula en el setup y cada frame se muestrea por índice.
/// Cero allocs por frame una vez crecido.
#[derive(Debug, Clone, Default)]
pub struct Scratch {
    curva: Vec<[f64; 2]>,
}

impl Scratch {
    /// Nuevo (reserva 96 puntos: el caso común sin regrow).
    pub fn nuevo() -> Self {
        Self {
            curva: Vec::with_capacity(96),
        }
    }

    /// Prepara la recta en el setup (`2..=256` puntos; degenerada → 2
    /// puntos en `base`). La única alloc del sampler vive acá.
    pub fn prepara_recta(&mut self, base: [f64; 2], dir: [f64; 2], n: usize) {
        self.curva.clear();
        let n = n.clamp(2, 256);
        let b0 = [finito_o_cero(base[0]), finito_o_cero(base[1])];
        let d = [finito_o_cero(dir[0]), finito_o_cero(dir[1])];
        for k in 0..n {
            let s = -1.0 + 2.0 * (k as f64) / ((n - 1) as f64);
            self.curva.push([b0[0] + d[0] * s, b0[1] + d[1] * s]);
        }
    }

    /// Punto en `u ∈ [0, 1]` (clamp; vacío → `[0, 0]`). Sin allocs.
    pub fn punto_en(&self, u: f64) -> [f64; 2] {
        if self.curva.is_empty() {
            return [0.0, 0.0];
        }
        let u = if u.is_finite() {
            u.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let idx = (u * ((self.curva.len() - 1) as f64)).round() as usize;
        self.curva[idx.min(self.curva.len() - 1)]
    }

    /// Puntos precalculados.
    pub fn len(&self) -> usize {
        self.curva.len()
    }

    /// ¿Sin puntos?
    pub fn is_empty(&self) -> bool {
        self.curva.is_empty()
    }
}

// ── Paleta 3b1b ───────────────────────────────────────────────────────────

const FONDO: [u8; 4] = [15, 19, 28, 255];
const BARRA: [u8; 4] = [10, 14, 22, 255];
const GRILLA: [u8; 4] = [36, 46, 64, 255];
const GRILLA_FANTASMA: [u8; 4] = [24, 31, 45, 255];
const EJE: [u8; 4] = [110, 130, 165, 255];
const BLANCO: [u8; 4] = [235, 238, 245, 255];
const AMARILLO: [u8; 4] = [255, 205, 70, 255];
const VERDE: [u8; 4] = [88, 196, 130, 255];
const ROJO: [u8; 4] = [250, 110, 110, 255];
const AZUL: [u8; 4] = [110, 170, 255, 255];
const NARANJA: [u8; 4] = [255, 160, 70, 255];
const GRIS: [u8; 4] = [140, 150, 170, 255];

// ── Datos de las escenas ──────────────────────────────────────────────────

/// `v` de combinaciones lineales (cap. 2).
const VEC_V: [f64; 2] = [2.0, 1.0];
/// `w` de combinaciones lineales.
const VEC_W: [f64; 2] = [-1.0, 2.0];
/// Coeficiente fijo sobre `w` mientras `s` barre `s·v + T·w`.
const VEC_T_FIJO: f64 = 0.6;
/// Matriz de la transformación (cap. 3; `det = 1`: conserva áreas).
const TRANS_A: Mat2 = Mat2 {
    a: 1.5,
    b: 1.0,
    c: 0.5,
    d: 1.0,
};
/// Vector testigo de la transformación.
const TRANS_V: [f64; 2] = [1.5, 1.0];
/// Matriz del determinante (cap. 6; `det = 2·1.5 − 1·0.5 = 2.5` exacto).
const DET_A: Mat2 = Mat2 {
    a: 2.0,
    b: 1.0,
    c: 0.5,
    d: 1.5,
};
/// Matriz de eigenvectores (cap. 14; `λ = 3` en `(1,1)`, `λ = 1` en `(1,−1)`).
const EIG_A: Mat2 = Mat2 {
    a: 2.0,
    b: 1.0,
    c: 1.0,
    d: 2.0,
};
/// Base de Jennifer (cap. 13): `b1 = (2,1)`, `b2 = (−1,1)`.
const BASE_B1: [f64; 2] = [2.0, 1.0];
/// Segunda base de Jennifer.
const BASE_B2: [f64; 2] = [-1.0, 1.0];
/// Flecha que no se mueve (std `(−1,2)` = base `(1/3, 5/3)`).
const BASE_V: [f64; 2] = [-1.0, 2.0];
/// `v` del producto cruz (cap. 10).
const CRUZ_V: [f64; 2] = [2.5, 0.5];
/// `w` final del producto cruz (`v×w = 2.5·2 − 0.5·0.8 = 4.6`).
const CRUZ_W: [f64; 2] = [0.8, 2.0];

// ── Raster CPU (puro, con chequeo de bordes, sin pánicos) ─────────────────

/// Vista mundo→píxel (`y` hacia arriba en mundo, abajo en píxel).
struct Vista {
    w: usize,
    h: usize,
    escala: f64,
}

/// Vista con `±4.5` unidades en la dimensión menor. Pura.
fn vista_de(w: u32, h: u32) -> Vista {
    let menor = (w.min(h) as f64).max(1.0);
    Vista {
        w: w as usize,
        h: h as usize,
        escala: menor / 9.0,
    }
}

/// Mundo → píxel (`None` si no finito). El llamador recorta a bordes.
fn mundo_a_px(v: &Vista, x: f64, y: f64) -> Option<(i32, i32)> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let px = v.w as f64 / 2.0 + x * v.escala;
    let py = v.h as f64 / 2.0 - y * v.escala;
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    Some((px.round() as i32, py.round() as i32))
}

/// Píxel opaco (silencioso si está fuera).
fn pinta(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = (y as usize) * w + (x as usize);
    let i = i.saturating_mul(LINALG_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + LINALG_BYTES_POR_PIXEL) {
        p.copy_from_slice(&c);
    }
}

/// Mezcla `src` sobre el destino con alfa 0..1 (no finito → 0).
fn mezcla(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, c: [u8; 4], alfa: f32) {
    let a = if alfa.is_finite() {
        alfa.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if a <= 0.0 {
        return;
    }
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    if a >= 1.0 {
        pinta(buf, w, h, x, y, c);
        return;
    }
    let i = ((y as usize) * w + (x as usize)).saturating_mul(LINALG_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + LINALG_BYTES_POR_PIXEL) {
        for k in 0..3 {
            let fondo = f32::from(p[k]);
            let tinta = f32::from(c[k]);
            p[k] = (fondo + (tinta - fondo) * a).round().clamp(0.0, 255.0) as u8;
        }
        p[3] = 255;
    }
}

/// Disco relleno de radio `r` (gráfica de líneas y puntos). Puro.
fn disco(buf: &mut [u8], w: usize, h: usize, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    if r <= 0 {
        pinta(buf, w, h, cx, cy, c);
        return;
    }
    let r = r.min(64);
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                pinta(buf, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Segmento en píxeles con grosor (DDA determinista). Puro.
#[allow(clippy::too_many_arguments)]
fn segmento_px(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    grosor: i32,
    c: [u8; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let pasos = dx.abs().max(dy.abs()).max(0);
    if pasos == 0 {
        disco(buf, w, h, x0, y0, grosor / 2, c);
        return;
    }
    let radio = (grosor / 2).clamp(0, 64);
    for s in 0..=pasos {
        let x = x0 + dx.saturating_mul(s) / pasos;
        let y = y0 + dy.saturating_mul(s) / pasos;
        if radio == 0 {
            pinta(buf, w, h, x, y, c);
        } else {
            disco(buf, w, h, x, y, radio, c);
        }
    }
}

/// Segmento mundo→píxel (silencioso si un extremo no es finito).
#[allow(clippy::too_many_arguments)]
fn segmento(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    grosor: i32,
    c: [u8; 4],
) {
    if let (Some((ax, ay)), Some((bx, by))) = (mundo_a_px(v, x0, y0), mundo_a_px(v, x1, y1)) {
        segmento_px(buf, w, h, ax, ay, bx, by, grosor, c);
    }
}

/// Flecha mundo (línea + punta triangular). Pura.
#[allow(clippy::too_many_arguments)]
fn flecha(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    desde: [f64; 2],
    hasta: [f64; 2],
    grosor: i32,
    c: [u8; 4],
) {
    let (ax, ay, bx, by) = match (
        mundo_a_px(v, desde[0], desde[1]),
        mundo_a_px(v, hasta[0], hasta[1]),
    ) {
        (Some((ax, ay)), Some((bx, by))) => (ax, ay, bx, by),
        _ => return,
    };
    segmento_px(buf, w, h, ax, ay, bx, by, grosor, c);
    let dx = (bx - ax) as f64;
    let dy = (by - ay) as f64;
    let largo = (dx * dx + dy * dy).sqrt();
    if !largo.is_finite() || largo < 6.0 {
        disco(buf, w, h, bx, by, grosor / 2 + 1, c);
        return;
    }
    let ux = dx / largo;
    let uy = dy / largo;
    // Punta: base a `hl` de la punta + alas perpendiculares.
    let hl = (8.0 + 2.0 * grosor as f64).min(24.0);
    let al = (hl * 0.45).min(12.0);
    let basex = bx as f64 - ux * hl;
    let basey = by as f64 - uy * hl;
    let perpx = -uy;
    let perpy = ux;
    segmento_px(
        buf,
        w,
        h,
        bx,
        by,
        (basex + perpx * al).round() as i32,
        (basey + perpy * al).round() as i32,
        grosor.max(1),
        c,
    );
    segmento_px(
        buf,
        w,
        h,
        bx,
        by,
        (basex - perpx * al).round() as i32,
        (basey - perpy * al).round() as i32,
        grosor.max(1),
        c,
    );
}

/// Rellena un cuadrilátero convexo (scanline, sin allocs: 4 aristas fijas).
fn rellena_cuad(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    p: [[f64; 2]; 4],
    c: [u8; 4],
    alfa: f32,
) {
    let mut q = [(0.0f64, 0.0f64); 4];
    for (k, punto) in p.iter().enumerate() {
        let (px, py) = match mundo_a_px(v, punto[0], punto[1]) {
            Some((x, y)) => (x as f64, y as f64),
            None => return,
        };
        q[k] = (px, py);
    }
    let mut ymin = i32::MAX;
    let mut ymax = i32::MIN;
    for punto in &q {
        ymin = ymin.min(punto.1.floor() as i32);
        ymax = ymax.max(punto.1.ceil() as i32);
    }
    ymin = ymin.clamp(0, h as i32 - 1);
    ymax = ymax.clamp(0, h as i32 - 1);
    if ymin > ymax {
        return;
    }
    for y in ymin..=ymax {
        let yf = f64::from(y) + 0.5;
        let mut xs = [0.0f64; 4];
        let mut n = 0usize;
        for k in 0..4 {
            let a = q[k];
            let b = q[(k + 1) % 4];
            if (a.1 <= yf && yf < b.1) || (b.1 <= yf && yf < a.1) {
                let t = (yf - a.1) / (b.1 - a.1);
                if t.is_finite() {
                    let x = a.0 + (b.0 - a.0) * t;
                    if x.is_finite() && n < 4 {
                        xs[n] = x;
                        n += 1;
                    }
                }
            }
        }
        // Ordena hasta 4 valores (inserción, sin allocs).
        for i in 1..n {
            let mut j = i;
            while j > 0 && xs[j - 1] > xs[j] {
                xs.swap(j, j - 1);
                j -= 1;
            }
        }
        let mut k = 0;
        while k + 1 < n {
            let x0 = (xs[k].floor() as i32).clamp(0, w as i32 - 1);
            let x1 = (xs[k + 1].ceil() as i32).clamp(0, w as i32 - 1);
            for x in x0..=x1 {
                mezcla(buf, w, h, x, y, c, alfa);
            }
            k += 2;
        }
    }
}

/// Rectángulo lleno en píxeles (barra de título). Puro.
#[allow(clippy::too_many_arguments)]
fn rect_lleno(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32, c: [u8; 4]) {
    for dy in 0..rh.max(0) {
        for dx in 0..rw.max(0) {
            pinta(buf, w, h, x + dx, y + dy, c);
        }
    }
}

/// Fondo + grilla identidad tenue + ejes. El setup de todas las escenas.
fn fondo(buf: &mut [u8], w: usize, h: usize, v: &Vista) {
    rect_lleno(buf, w, h, 0, 0, w as i32, h as i32, FONDO);
    dibuja_grilla(buf, w, h, v, Mat2::IDENTIDAD, 4, GRILLA);
    dibuja_ejes(buf, w, h, v, 4.5, EJE);
}

/// Grilla `x = k`, `y = k` (`k ∈ −ext..=ext`) deformada por `m`.
/// Subdivide cada recta en 18 tramos (curvas futuras quedan polilíneas).
fn dibuja_grilla(buf: &mut [u8], w: usize, h: usize, v: &Vista, m: Mat2, ext: i32, c: [u8; 4]) {
    let ext = ext.clamp(1, 8);
    let limite = f64::from(ext) + 0.5;
    let tramos = 18usize;
    for k in -ext..=ext {
        let kf = f64::from(k);
        // Recta x = k.
        let mut anterior: Option<(i32, i32)> = None;
        for s in 0..=tramos {
            let y = -limite + 2.0 * limite * (s as f64) / (tramos as f64);
            let pt = mundo_a_px(v, m.aplica([kf, y])[0], m.aplica([kf, y])[1]);
            if let (Some(a), Some(b)) = (anterior, pt) {
                segmento_px(buf, w, h, a.0, a.1, b.0, b.1, 0, c);
            }
            anterior = pt;
        }
        // Recta y = k.
        anterior = None;
        for s in 0..=tramos {
            let x = -limite + 2.0 * limite * (s as f64) / (tramos as f64);
            let pt = mundo_a_px(v, m.aplica([x, kf])[0], m.aplica([x, kf])[1]);
            if let (Some(a), Some(b)) = (anterior, pt) {
                segmento_px(buf, w, h, a.0, a.1, b.0, b.1, 0, c);
            }
            anterior = pt;
        }
    }
}

/// Ejes con ticks enteros (referencia identidad, aun con grilla deformada).
fn dibuja_ejes(buf: &mut [u8], w: usize, h: usize, v: &Vista, ext: f64, c: [u8; 4]) {
    let ext = ext.clamp(1.0, 8.0);
    segmento(buf, w, h, v, -ext, 0.0, ext, 0.0, 1, c);
    segmento(buf, w, h, v, 0.0, -ext, 0.0, ext, 1, c);
    let mut k = -ext.floor() as i32;
    while f64::from(k) <= ext {
        if k != 0 {
            let kf = f64::from(k);
            segmento(buf, w, h, v, kf, -0.09, kf, 0.09, 1, c);
            segmento(buf, w, h, v, -0.09, kf, 0.09, kf, 1, c);
        }
        k += 1;
    }
}

// ── Fuente bitmap 5×7 (mayúsculas; la etiqueta se normaliza acá) ──────────

/// Glifo 5×7 como 7 filas de 5 bits (bit 4 = izq.). `None` si no existe.
fn glifo(b: u8) -> Option<[u8; 7]> {
    match b {
        b'A' => Some([0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        b'B' => Some([0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E]),
        b'C' => Some([0x0F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0F]),
        b'D' => Some([0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E]),
        b'E' => Some([0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F]),
        b'F' => Some([0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10]),
        b'G' => Some([0x0F, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0E]),
        b'H' => Some([0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        b'I' => Some([0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F]),
        b'J' => Some([0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        b'K' => Some([0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]),
        b'L' => Some([0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F]),
        b'M' => Some([0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11]),
        b'N' => Some([0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11]),
        b'O' => Some([0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        b'P' => Some([0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10]),
        b'Q' => Some([0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D]),
        b'R' => Some([0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11]),
        b'S' => Some([0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E]),
        b'T' => Some([0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        b'U' => Some([0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        b'V' => Some([0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04]),
        b'W' => Some([0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11]),
        b'X' => Some([0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11]),
        b'Y' => Some([0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04]),
        b'Z' => Some([0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F]),
        b'0' => Some([0x0E, 0x13, 0x15, 0x15, 0x19, 0x11, 0x0E]),
        b'1' => Some([0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        b'2' => Some([0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F]),
        b'3' => Some([0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E]),
        b'4' => Some([0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
        b'5' => Some([0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
        b'6' => Some([0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
        b'7' => Some([0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        b'8' => Some([0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
        b'9' => Some([0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E]),
        b' ' => Some([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        b'.' => Some([0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06]),
        b',' => Some([0x00, 0x00, 0x00, 0x00, 0x06, 0x06, 0x08]),
        b':' => Some([0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x00]),
        b'=' => Some([0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00]),
        b'+' => Some([0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00]),
        b'-' => Some([0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00]),
        b'|' => Some([0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        b'(' => Some([0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02]),
        b')' => Some([0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08]),
        b'?' => Some([0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04]),
        _ => None,
    }
}

/// Minúsculas, acentos y `ñ` → ASCII mayús (`None` = se salta el char).
fn mayus_ascii(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' => Some((u32::from(ch) - u32::from('a') + u32::from('A')) as u8),
        'A'..='Z' => Some(ch as u8),
        '0'..='9' => Some(ch as u8),
        ' ' | '.' | ',' | ':' | '=' | '+' | '-' | '|' | '(' | ')' | '?' => Some(ch as u8),
        'á' | 'à' | 'ä' | 'â' => Some(b'A'),
        'é' | 'è' | 'ë' | 'ê' => Some(b'E'),
        'í' | 'ì' | 'ï' | 'î' => Some(b'I'),
        'ó' | 'ò' | 'ö' | 'ô' => Some(b'O'),
        'ú' | 'ù' | 'ü' | 'û' => Some(b'U'),
        'ñ' | 'Ñ' => Some(b'N'),
        'ç' | 'Ç' => Some(b'C'),
        _ => None,
    }
}

/// Pinta un glifo en `(x, y)` con escala entera. Devuelve el avance en px.
#[allow(clippy::too_many_arguments)]
fn pinta_glifo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    g: [u8; 7],
) -> i32 {
    let esc = (esc.clamp(1, 4)) as i32;
    for (fila, bits) in g.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                for dy in 0..esc {
                    for dx in 0..esc {
                        pinta(buf, w, h, x + col * esc + dx, y + fila as i32 * esc + dy, c);
                    }
                }
            }
        }
    }
    6 * esc
}

/// Texto ASCII (normalizado a mayúsculas) en píxeles. Devuelve la `x` final.
#[allow(clippy::too_many_arguments)]
fn texto(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    s: &str,
) -> i32 {
    let mut cursor = x;
    for ch in s.chars() {
        if let Some(b) = mayus_ascii(ch) {
            if let Some(g) = glifo(b) {
                cursor += pinta_glifo(buf, w, h, cursor, y, esc, c, g);
            } else {
                cursor += 6 * (esc.clamp(1, 4) as i32);
            }
        }
    }
    cursor
}

/// Bytes ya ASCII (dígitos de [`formatea_1_decimal`]). Devuelve la `x` final.
#[allow(clippy::too_many_arguments)]
fn texto_bytes(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    s: &[u8],
) -> i32 {
    let mut cursor = x;
    for b in s {
        if let Some(g) = glifo(*b) {
            cursor += pinta_glifo(buf, w, h, cursor, y, esc, c, g);
        } else {
            cursor += 6 * (esc.clamp(1, 4) as i32);
        }
    }
    cursor
}

/// Un decimal con redondeo (`2.5`, `-0.4`, `?` si no finito) en stack,
/// sin heap. Devuelve el largo usado.
fn formatea_1_decimal(v: f64, salida: &mut [u8; 10]) -> usize {
    if !v.is_finite() {
        salida[0] = b'?';
        return 1;
    }
    let mut n = 0usize;
    let mut q = (v * 10.0).round();
    if q < 0.0 {
        if n < salida.len() {
            salida[n] = b'-';
            n += 1;
        }
        q = -q;
    }
    if q > 999_999.0 {
        q = 999_999.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let qi = q as i64;
    let ent = qi / 10;
    let dec = (qi % 10) as u8;
    // Dígitos del entero (invertidos en stack, sin heap).
    let mut tmp = [0u8; 8];
    let mut m = 0usize;
    let mut e = ent.max(0);
    loop {
        if m < tmp.len() {
            tmp[m] = b'0' + (e % 10) as u8;
            m += 1;
        }
        e /= 10;
        if e == 0 {
            break;
        }
    }
    while m > 0 {
        m -= 1;
        if n < salida.len() {
            salida[n] = tmp[m];
            n += 1;
        }
    }
    if n < salida.len() {
        salida[n] = b'.';
        n += 1;
    }
    if n < salida.len() {
        salida[n] = b'0' + dec;
        n += 1;
    }
    n
}

/// `(x, y)` con un decimal (`(0.3,1.7)`) en stack, sin heap.
fn formatea_par(c: [f64; 2], salida: &mut [u8; 24]) -> usize {
    let mut n = 0usize;
    if n < salida.len() {
        salida[n] = b'(';
        n += 1;
    }
    let mut num = [0u8; 10];
    let l0 = formatea_1_decimal(c[0], &mut num);
    for b in num.iter().take(l0) {
        if n < salida.len() {
            salida[n] = *b;
            n += 1;
        }
    }
    if n < salida.len() {
        salida[n] = b',';
        n += 1;
    }
    let l1 = formatea_1_decimal(c[1], &mut num);
    for b in num.iter().take(l1) {
        if n < salida.len() {
            salida[n] = *b;
            n += 1;
        }
    }
    if n < salida.len() {
        salida[n] = b')';
        n += 1;
    }
    n
}

/// Escala de texto según lienzo (1 en chicos, 2 desde 300 px).
fn escala_texto(w: u32, h: u32) -> usize {
    if w.min(h) >= 300 {
        2
    } else {
        1
    }
}

/// Barra de título + etiqueta. Devuelve el alto de la barra en px.
fn rotulo(buf: &mut [u8], w: usize, h: usize, esc: usize, etiqueta: &str) -> i32 {
    let alto = 7 * esc.clamp(1, 4) as i32 + 8;
    rect_lleno(buf, w, h, 0, 0, w as i32, alto, BARRA);
    texto(buf, w, h, 6, 4, esc, BLANCO, etiqueta);
    for x in 0..w as i32 {
        pinta(buf, w, h, x, alto, GRILLA);
    }
    alto + 1
}

// ── Registro público ──────────────────────────────────────────────────────

/// ¿Atiende este módulo la plantilla (case-insensitive, con trim)?
pub fn es_plantilla_linalg(template: &str) -> bool {
    etiqueta_de(template).is_some()
}

/// Etiqueta en español de la escena (`None` si no es de este módulo).
pub fn etiqueta_de(template: &str) -> Option<&'static str> {
    match template.trim().to_lowercase().as_str() {
        "vectores-combinacion-lineal" => Some("COMBINACION LINEAL: S.V+T.W"),
        "matriz-transformacion" => Some("A.X MUEVE LA GRILLA"),
        "determinante-area" => Some("DET(A)=AREA"),
        "eigenvectores" => Some("AUTOVECTORES: AV=L.V"),
        "cambio-de-base" => Some("CAMBIO DE BASE: MISMA FLECHA"),
        "producto-cruz" => Some("PRODUCTO CRUZ: |VXW|=AREA"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escena {
    Vectores,
    Transformacion,
    Determinante,
    Eigen,
    Base,
    Cruz,
}

/// Resuelve el id a escena (`Err` honesto si no es de este módulo).
fn escena_de(template: &str) -> Result<Escena, LinalgError> {
    match template.trim().to_lowercase().as_str() {
        "vectores-combinacion-lineal" => Ok(Escena::Vectores),
        "matriz-transformacion" => Ok(Escena::Transformacion),
        "determinante-area" => Ok(Escena::Determinante),
        "eigenvectores" => Ok(Escena::Eigen),
        "cambio-de-base" => Ok(Escena::Base),
        "producto-cruz" => Ok(Escena::Cruz),
        _ => Err(LinalgError::PlantillaDesconocida {
            got: template.trim().to_string(),
        }),
    }
}

// ── Escenas (setup + animación + etiqueta) ────────────────────────────────

/// Escena de vectores y combinaciones lineales (cap. 1–2): `v` y `w` crecen,
/// el paralelogramo que generan se rellena y un punto barre la recta
/// `s·v + 0.6·w` (`s ∈ [−1, 1]`, precalculada en `scratch`).
fn dibuja_vectores(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, scratch: &Scratch) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let gv = tramo(t, 0.05, 0.30);
    let gw = tramo(t, 0.20, 0.45);
    let u = suave(tramo(t, 0.45, 0.90));
    // Paralelogramo (origen, v, v+w, w) con alfa en rampa.
    let alfa = 0.30 * tramo(t, 0.50, 0.70) as f32;
    if gv > 0.0 && gw > 0.0 {
        let vv = [VEC_V[0] * gv, VEC_V[1] * gv];
        let ww = [VEC_W[0] * gw, VEC_W[1] * gw];
        rellena_cuad(
            buf,
            w,
            h,
            v,
            [[0.0, 0.0], vv, [vv[0] + ww[0], vv[1] + ww[1]], ww],
            AMARILLO,
            alfa,
        );
    }
    // Estela: puntos del scratch hasta `u` (sin allocs, por índice).
    if !scratch.is_empty() && u > 0.0 {
        let hasta = (u * ((scratch.len() - 1) as f64)).round() as usize;
        let mut anterior: Option<(i32, i32)> = None;
        for k in 0..=hasta.min(scratch.len() - 1) {
            let p = scratch.punto_en(k as f64 / ((scratch.len() - 1).max(1) as f64));
            if let Some((x, y)) = mundo_a_px(v, p[0], p[1]) {
                if let Some((ax, ay)) = anterior {
                    segmento_px(buf, w, h, ax, ay, x, y, 2, NARANJA);
                }
                anterior = Some((x, y));
            }
        }
        let p = scratch.punto_en(u);
        if let Some((x, y)) = mundo_a_px(v, p[0], p[1]) {
            disco(buf, w, h, x, y, 4, NARANJA);
        }
    }
    // Vectores con crecimiento.
    if gv > 0.0 {
        flecha(
            buf,
            w,
            h,
            v,
            [0.0, 0.0],
            [VEC_V[0] * gv, VEC_V[1] * gv],
            3,
            AMARILLO,
        );
    }
    if gw > 0.0 {
        flecha(
            buf,
            w,
            h,
            v,
            [0.0, 0.0],
            [VEC_W[0] * gw, VEC_W[1] * gw],
            3,
            AZUL,
        );
    }
}

/// Escena de matrices como transformaciones de la grilla (cap. 3): `M(t)` va de
/// `I` a `A`; la grilla copiada se deforma (paralela y equiespaciada),
/// `i-hat`/`j-hat` viajan a las columnas y el testigo `v` a `A·v`.
fn dibuja_transformacion(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64) {
    // 1. Setup: fondo + grilla fantasma (original) + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let aparece = tramo(t, 0.0, 0.15);
    let m = Mat2::IDENTIDAD.interpola(TRANS_A, suave(tramo(t, 0.15, 0.85)));
    dibuja_grilla(buf, w, h, v, m, 4, GRILLA);
    let iv = m.aplica([1.0, 0.0]);
    let jv = m.aplica([0.0, 1.0]);
    if aparece > 0.0 {
        flecha(
            buf,
            w,
            h,
            v,
            [0.0, 0.0],
            [iv[0] * aparece, iv[1] * aparece],
            3,
            VERDE,
        );
        flecha(
            buf,
            w,
            h,
            v,
            [0.0, 0.0],
            [jv[0] * aparece, jv[1] * aparece],
            3,
            ROJO,
        );
        let img = m.aplica(TRANS_V);
        flecha(
            buf,
            w,
            h,
            v,
            [0.0, 0.0],
            [TRANS_V[0] * aparece, TRANS_V[1] * aparece],
            2,
            GRIS,
        );
        flecha(buf, w, h, v, [0.0, 0.0], img, 3, AMARILLO);
    }
}

/// Escena de determinante como área (cap. 6): el cuadrado unidad viaja con `M(t)`
/// y su área crece `1 → 2.5`; el valor vivo `det(M)` se dibuja con un
/// decimal (aritmética en stack, sin heap).
fn dibuja_determinante(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let m = Mat2::IDENTIDAD.interpola(DET_A, suave(tramo(t, 0.15, 0.85)));
    let p00 = m.aplica([0.0, 0.0]);
    let p10 = m.aplica([1.0, 0.0]);
    let p11 = m.aplica([1.0, 1.0]);
    let p01 = m.aplica([0.0, 1.0]);
    rellena_cuad(buf, w, h, v, [p00, p10, p11, p01], AMARILLO, 0.35);
    // Cuadrado unidad original (referencia punteada: segmentos cortos).
    for k in 0..4 {
        let a = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]][k];
        let b = [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]][k];
        segmento(
            buf,
            w,
            h,
            v,
            a[0],
            a[1],
            (a[0] + b[0]) / 2.0,
            (a[1] + b[1]) / 2.0,
            1,
            GRIS,
        );
        segmento(
            buf,
            w,
            h,
            v,
            (a[0] + b[0]) / 2.0,
            (a[1] + b[1]) / 2.0,
            b[0],
            b[1],
            1,
            FONDO,
        );
    }
    segmento(buf, w, h, v, p00[0], p00[1], p10[0], p10[1], 2, AMARILLO);
    segmento(buf, w, h, v, p10[0], p10[1], p11[0], p11[1], 2, AMARILLO);
    segmento(buf, w, h, v, p11[0], p11[1], p01[0], p01[1], 2, AMARILLO);
    segmento(buf, w, h, v, p01[0], p01[1], p00[0], p00[1], 2, AMARILLO);
    // Valor vivo del determinante.
    let det = m.det();
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(det, &mut num);
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc, BLANCO, "DET=");
    texto_bytes(buf, w, h, x, y, esc, AMARILLO, &num[..ln]);
}

/// Escena de eigenvectores (cap. 14): un abanico de direcciones viaja con `M(t)`;
/// las de 45° y −45° caen sobre su propio span (`λ = 3` y `λ = 1`);
/// el resto se sale (el testigo gris lo muestra).
fn dibuja_eigen(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let m = Mat2::IDENTIDAD.interpola(EIG_A, suave(tramo(t, 0.15, 0.85)));
    dibuja_grilla(buf, w, h, v, m, 4, GRILLA_FANTASMA);
    // Abanico cada 22.5° (incluye ±45°): segmentos `±2.2·d` mapeados.
    for k in -3..=4 {
        let ang = f64::from(k) * std::f64::consts::PI / 8.0;
        let d = [ang.cos(), ang.sin()];
        let es_propio = k == 2 || k == -2;
        let a = m.aplica([-2.2 * d[0], -2.2 * d[1]]);
        let b = m.aplica([2.2 * d[0], 2.2 * d[1]]);
        let color = if es_propio { AMARILLO } else { GRILLA };
        segmento(
            buf,
            w,
            h,
            v,
            a[0],
            a[1],
            b[0],
            b[1],
            if es_propio { 2 } else { 1 },
            color,
        );
    }
    // Direcciones originales ±45° (referencia).
    let sq = std::f64::consts::FRAC_1_SQRT_2;
    segmento(
        buf,
        w,
        h,
        v,
        -3.2 * sq,
        -3.2 * sq,
        3.2 * sq,
        3.2 * sq,
        1,
        GRIS,
    );
    segmento(
        buf,
        w,
        h,
        v,
        -3.2 * sq,
        3.2 * sq,
        3.2 * sq,
        -3.2 * sq,
        1,
        GRIS,
    );
    // Flechas propias + testigo genérico.
    let e1 = m.aplica([2.0 * sq, 2.0 * sq]);
    let e2 = m.aplica([2.0 * sq, -2.0 * sq]);
    flecha(buf, w, h, v, [0.0, 0.0], e1, 3, AMARILLO);
    flecha(buf, w, h, v, [0.0, 0.0], e2, 3, VERDE);
    let g = [2.2, 0.4];
    let mg = m.aplica(g);
    segmento(buf, w, h, v, 0.0, 0.0, g[0], g[1], 1, GRIS);
    flecha(buf, w, h, v, [0.0, 0.0], mg, 2, NARANJA);
    // Rótulos λ cerca de las puntas.
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc, AMARILLO, "L=3 ");
    texto(buf, w, h, x, y, esc, VERDE, "L=1");
}

/// Escena de cambio de base (cap. 13): la grilla muta `I → B` pero la flecha
/// `v = (−1, 2)` no se mueve; el cartel pasa de coords std a coords en
/// `B = (1/3, 5/3)` a mitad de timeline (calculadas por Cramer, no a mano).
fn dibuja_base(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + ejes (la grilla la pone la animación).
    rect_lleno(buf, w, h, 0, 0, w as i32, h as i32, FONDO);
    // 2. Animación.
    let b = Mat2::nuevo(BASE_B1[0], BASE_B2[0], BASE_B1[1], BASE_B2[1]);
    let m = Mat2::IDENTIDAD.interpola(b, suave(tramo(t, 0.15, 0.85)));
    dibuja_grilla(buf, w, h, v, m, 4, AZUL);
    dibuja_ejes(buf, w, h, v, 4.5, EJE);
    let b1 = m.aplica([1.0, 0.0]);
    let b2 = m.aplica([0.0, 1.0]);
    flecha(buf, w, h, v, [0.0, 0.0], b1, 3, VERDE);
    flecha(buf, w, h, v, [0.0, 0.0], b2, 3, ROJO);
    // La flecha: misma en ambos sistemas.
    flecha(buf, w, h, v, [0.0, 0.0], BASE_V, 3, AMARILLO);
    // Cartel de coordenadas (std primero, base B después).
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    if t < 0.5 {
        let x = texto(buf, w, h, 8, y, esc, BLANCO, "STD ");
        let mut num = [0u8; 24];
        let ln = formatea_par([-1.0, 2.0], &mut num);
        texto_bytes(buf, w, h, x, y, esc, GRIS, &num[..ln]);
    } else {
        let x = texto(buf, w, h, 8, y, esc, BLANCO, "BASE ");
        let coords = coords_en_base(BASE_B1, BASE_B2, BASE_V).unwrap_or([0.0, 0.0]);
        let mut num = [0u8; 24];
        let ln = formatea_par(coords, &mut num);
        texto_bytes(buf, w, h, x, y, esc, AMARILLO, &num[..ln]);
    }
}

/// Escena de producto cruz (cap. 10–11): `v` fijo, `w` barre desde `v` (área 0)
/// hasta su lugar; el paralelogramo se rellena con el área y el valor
/// vivo `|v×w|` se dibuja; el testigo sale de la pantalla (signo +).
fn dibuja_cruz(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: `w` rota de `v` a su ángulo final (módulo constante).
    let u = suave(tramo(t, 0.15, 0.85));
    let ang_v = CRUZ_V[1].atan2(CRUZ_V[0]);
    let ang_w = CRUZ_W[1].atan2(CRUZ_W[0]);
    let modulo = (CRUZ_W[0] * CRUZ_W[0] + CRUZ_W[1] * CRUZ_W[1]).sqrt();
    let modulo = if modulo.is_finite() && modulo > 0.0 {
        modulo
    } else {
        1.0
    };
    let ang = ang_v + (ang_w - ang_v) * u;
    let wmov = [modulo * ang.cos(), modulo * ang.sin()];
    let area = cruz2(CRUZ_V, wmov).abs();
    let area_max = cruz2(CRUZ_V, CRUZ_W).abs().max(0.01);
    let alfa = (0.40 * (area / area_max).clamp(0.0, 1.0)) as f32;
    rellena_cuad(
        buf,
        w,
        h,
        v,
        [
            [0.0, 0.0],
            CRUZ_V,
            [CRUZ_V[0] + wmov[0], CRUZ_V[1] + wmov[1]],
            wmov,
        ],
        NARANJA,
        alfa,
    );
    flecha(buf, w, h, v, [0.0, 0.0], CRUZ_V, 3, AMARILLO);
    flecha(buf, w, h, v, [0.0, 0.0], wmov, 3, AZUL);
    // Testigo de la normal: círculo con punto (sale, signo +).
    let cx = w as i32 - 26;
    let cy = h as i32 - 26;
    for dy in -9..=9 {
        for dx in -9..=9 {
            let d2 = dx * dx + dy * dy;
            if (49..=81).contains(&d2) {
                pinta(buf, w, h, cx + dx, cy + dy, BLANCO);
            }
        }
    }
    disco(buf, w, h, cx, cy, 2, BLANCO);
    // Valor vivo.
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(area, &mut num);
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc, BLANCO, "|VXW|=");
    texto_bytes(buf, w, h, x, y, esc, NARANJA, &num[..ln]);
}

/// Un frame de la escena en `t` global (setup del scratch ya hecho).
#[allow(clippy::too_many_arguments)]
fn dibuja_frame(
    esc: Escena,
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    etiqueta: &str,
    esc_texto: usize,
) {
    match esc {
        Escena::Vectores => dibuja_vectores(buf, w, h, v, t, scratch),
        Escena::Transformacion => dibuja_transformacion(buf, w, h, v, t),
        Escena::Determinante => dibuja_determinante(buf, w, h, v, t, esc_texto),
        Escena::Eigen => dibuja_eigen(buf, w, h, v, t, esc_texto),
        Escena::Base => dibuja_base(buf, w, h, v, t, esc_texto),
        Escena::Cruz => dibuja_cruz(buf, w, h, v, t, esc_texto),
    }
    // 3. Etiqueta (siempre, arriba).
    rotulo(buf, w, h, esc_texto, etiqueta);
}

// ── Renderers públicos ────────────────────────────────────────────────────

/// Núcleo: rango `[desde, desde+cantidad)` de un set de `total` frames
/// (el `t` global sale del índice global: el corte es empalme exacto).
/// Valida lienzo, rango y que el chunk entre en 64 MiB.
pub fn render_linalg_rango(
    template: &str,
    width: u32,
    height: u32,
    total: usize,
    desde: usize,
    cantidad: usize,
) -> Result<Vec<RgbaFrame>, LinalgError> {
    let esc = escena_de(template)?;
    let etiqueta = etiqueta_de(template).unwrap_or("ALGEBRA LINEAL");
    valida_lienzo(width, height)?;
    if !(LINALG_MIN_FRAMES..=LINALG_LONGFORM_MAX_FRAMES).contains(&total) {
        return Err(LinalgError::FramesFueraDeRango { got: total });
    }
    if cantidad == 0 || desde.checked_add(cantidad).is_none_or(|fin| fin > total) {
        return Err(LinalgError::FramesFueraDeRango { got: cantidad });
    }
    match estima_bytes(width, height, cantidad) {
        Some(n) if n <= LINALG_CHUNK_MAX_BYTES => {}
        Some(n) => return Err(LinalgError::SetDemasiadoGrande { bytes: n }),
        None => return Err(LinalgError::SetDemasiadoGrande { bytes: usize::MAX }),
    }
    let vista = vista_de(width, height);
    let esc_texto = escala_texto(width, height);
    // Setup del sampler (una vez, no por frame).
    let mut scratch = Scratch::nuevo();
    if esc == Escena::Vectores {
        scratch.prepara_recta([VEC_W[0] * VEC_T_FIJO, VEC_W[1] * VEC_T_FIJO], VEC_V, 65);
    }
    let mut frames = Vec::new();
    if frames.try_reserve_exact(cantidad).is_err() {
        return Err(LinalgError::SetDemasiadoGrande { bytes: usize::MAX });
    }
    for g in desde..desde.saturating_add(cantidad) {
        let t = progreso_en(g, total);
        let mut frame = RgbaFrame::nuevo(width, height)?;
        dibuja_frame(
            esc,
            &mut frame.pixels,
            vista.w,
            vista.h,
            &vista,
            t,
            &scratch,
            etiqueta,
            esc_texto,
        );
        frames.push(frame);
    }
    Ok(frames)
}

/// Set completo de `frames` para la plantilla (valida el presupuesto de
/// 64 MiB: lo que no entra se pide por rangos con [`render_linalg_rango`]).
pub fn render_linalg_frames(
    template: &str,
    width: u32,
    height: u32,
    frames: usize,
) -> Result<Vec<RgbaFrame>, LinalgError> {
    valida_pedido(width, height, frames)?;
    render_linalg_rango(template, width, height, frames, 0, frames)
}

/// Vectores y combinaciones lineales (48 frames default).
pub fn render_vectores_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames(
        "vectores-combinacion-lineal",
        width,
        height,
        LINALG_DEFAULT_FRAMES,
    )
}

/// Matrices como transformaciones de la grilla (48 frames default).
pub fn render_transformacion_frames(
    width: u32,
    height: u32,
) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames(
        "matriz-transformacion",
        width,
        height,
        LINALG_DEFAULT_FRAMES,
    )
}

/// Determinante como área (48 frames default).
pub fn render_determinante_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames("determinante-area", width, height, LINALG_DEFAULT_FRAMES)
}

/// Eigenvectores (48 frames default).
pub fn render_eigenvectores_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames("eigenvectores", width, height, LINALG_DEFAULT_FRAMES)
}

/// Cambio de base (48 frames default).
pub fn render_cambio_base_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames("cambio-de-base", width, height, LINALG_DEFAULT_FRAMES)
}

/// Producto cruz (48 frames default).
pub fn render_producto_cruz_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, LinalgError> {
    render_linalg_frames("producto-cruz", width, height, LINALG_DEFAULT_FRAMES)
}

// ── Tests inline ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn set_ok(id: &str, w: u32, h: u32, n: usize) -> Vec<RgbaFrame> {
        let r = render_linalg_frames(id, w, h, n);
        assert!(r.is_ok(), "{id} falló: {:?}", r.err());
        if let Ok(set) = r {
            assert_eq!(set.len(), n, "{id}: len");
            for f in &set {
                assert_eq!((f.width, f.height), (w, h), "{id}: dims");
                assert_eq!(f.pixels.len(), w as usize * h as usize * 4, "{id}: px");
            }
            set
        } else {
            Vec::new()
        }
    }

    #[test]
    fn registro_seis_ids_kebab_unicos() {
        assert_eq!(TEMPLATE_IDS.len(), 6);
        let mut vistos = Vec::new();
        for id in TEMPLATE_IDS {
            assert!(!id.is_empty(), "id vacío");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "no kebab: {id}"
            );
            assert!(!vistos.contains(id), "duplicado: {id}");
            vistos.push(*id);
            assert!(es_plantilla_linalg(id), "{id} no reconocido");
            assert!(
                etiqueta_de(id).is_some_and(|e| !e.is_empty()),
                "{id} sin etiqueta"
            );
            assert!(escena_de(id).is_ok(), "{id} sin escena");
        }
        assert!(!es_plantilla_linalg("taylor-series"));
        assert!(!es_plantilla_linalg(""));
        assert!(etiqueta_de("taylor-series").is_none());
        assert!(escena_de("taylor-series").is_err());
        // Case-insensitive con trim (como el dispatcher nativo).
        assert!(es_plantilla_linalg("  Eigenvectores "));
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(LINALG_DEFAULT_FRAMES, 48);
        assert_eq!((LINALG_MIN_FRAMES, LINALG_MAX_FRAMES), (1, 64));
        assert_eq!(LINALG_LONGFORM_MAX_FRAMES, 1500);
        assert_eq!(LINALG_CHUNK_MAX_BYTES, 64 * 1024 * 1024);
        assert_eq!((LINALG_CANVAS_MIN, LINALG_CANVAS_MAX), (64, 4096));
        assert_eq!(estima_bytes(480, 360, 48), Some(33_177_600));
        assert!(estima_bytes(480, 360, 48).is_some_and(|n| n <= LINALG_CHUNK_MAX_BYTES));
        assert_eq!(frames_por_chunk(1280, 720), 18);
        assert_eq!(frames_por_chunk(640, 480), 54);
        assert_eq!(frames_por_chunk(0, 480), 0);
        assert_eq!(frames_para_duracion(50_000, 30), 1500);
        assert_eq!(frames_para_duracion(2000, 12), 24);
        assert_eq!(frames_para_duracion(1000, 0), 0);
        assert!(valida_pedido(480, 360, 48).is_ok());
        assert!(valida_pedido(64, 64, 1500).is_ok());
        assert!(valida_pedido(480, 360, 0).is_err());
        assert!(valida_pedido(480, 360, 1501).is_err());
        assert!(valida_pedido(63, 360, 48).is_err());
        assert!(valida_pedido(480, 4097, 48).is_err());
        assert!(valida_pedido(4096, 4096, 48).is_err());
    }

    #[test]
    fn matematica_exacta() {
        assert_eq!(suave(0.0), 0.0);
        assert_eq!(suave(1.0), 1.0);
        assert!((suave(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(suave(f64::NAN), 0.0);
        assert_eq!(tramo(0.5, 0.0, 1.0), 0.5);
        assert_eq!(tramo(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(tramo(2.0, 0.0, 1.0), 1.0);
        assert_eq!(tramo(0.5, 1.0, 1.0), 0.0);
        assert_eq!(progreso_en(0, 48), 0.0);
        assert_eq!(progreso_en(47, 48), 1.0);
        assert_eq!(progreso_en(0, 1), 0.0);
        let m = Mat2::IDENTIDAD.interpola(DET_A, 0.0);
        assert_eq!(m, Mat2::IDENTIDAD);
        let m = Mat2::IDENTIDAD.interpola(DET_A, 1.0);
        assert!((m.det() - 2.5).abs() < 1e-12, "det={}", m.det());
        let p = DET_A.aplica([1.0, 1.0]);
        assert!((p[0] - 3.0).abs() < 1e-12 && (p[1] - 2.0).abs() < 1e-12);
        assert!((TRANS_A.det() - 1.0).abs() < 1e-12, "shear conserva área");
        // Base de Jennifer: (−1,2) = 1/3·b1 + 5/3·b2.
        let c = coords_en_base(BASE_B1, BASE_B2, BASE_V);
        assert!(c.is_some(), "Cramer falló con base válida");
        if let Some(c) = c {
            assert!((c[0] - 1.0 / 3.0).abs() < 1e-9, "c1={}", c[0]);
            assert!((c[1] - 5.0 / 3.0).abs() < 1e-9, "c2={}", c[1]);
        }
        assert!(coords_en_base([1.0, 1.0], [2.0, 2.0], [1.0, 0.0]).is_none());
        assert!((cruz2(CRUZ_V, CRUZ_W) - 4.6).abs() < 1e-9);
        assert!(cruz2(CRUZ_V, CRUZ_W) > 0.0, "testigo sale de pantalla");
        // Eigenvalores de EIG_A: 3 y 1 (traza 4, det 3).
        assert!((EIG_A.det() - 3.0).abs() < 1e-12);
        let e1 = EIG_A.aplica([1.0, 1.0]);
        assert!((e1[0] - 3.0).abs() < 1e-12 && (e1[1] - 3.0).abs() < 1e-12);
        let e2 = EIG_A.aplica([1.0, -1.0]);
        assert!((e2[0] - 1.0).abs() < 1e-12 && (e2[1] + 1.0).abs() < 1e-12);
    }

    #[test]
    fn scratch_setup_una_vez_muestreo_sin_allocs() {
        let mut s = Scratch::nuevo();
        assert!(s.is_empty());
        s.prepara_recta([0.0, 1.2], [2.0, 1.0], 65);
        assert_eq!(s.len(), 65);
        let p0 = s.punto_en(0.0);
        assert!((p0[0] + 2.0).abs() < 1e-12 && (p0[1] - 0.2).abs() < 1e-12);
        let medio = s.punto_en(0.5);
        assert!((medio[0]).abs() < 1e-12 && (medio[1] - 1.2).abs() < 1e-12);
        assert_eq!(s.punto_en(1.0), [2.0, 2.2]);
        // Determinista y clamp (misma API que usa el loop por frame).
        assert_eq!(s.punto_en(0.5), medio);
        assert_eq!(s.punto_en(99.0), s.punto_en(1.0));
        let vacio = Scratch::nuevo();
        assert_eq!(vacio.punto_en(0.5), [0.0, 0.0]);
    }

    #[test]
    fn fuente_cubre_todas_las_etiquetas_y_valores() {
        for id in TEMPLATE_IDS {
            if let Some(e) = etiqueta_de(id) {
                for ch in e.chars() {
                    let b = mayus_ascii(ch);
                    assert!(b.is_some(), "{id}: char sin mapa {ch:?}");
                    if let Some(byte) = b {
                        assert!(glifo(byte).is_some(), "{id}: sin glifo {ch:?}");
                    }
                }
            }
        }
        // Charset dinámico (coords, dets, áreas, lambdas).
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:;=+-|()? ".chars() {
            if let Some(b) = mayus_ascii(ch) {
                assert!(glifo(b).is_some(), "sin glifo {ch:?}");
            }
        }
        let mut num = [0u8; 10];
        assert_eq!(formatea_1_decimal(2.5, &mut num), 3);
        assert_eq!(&num[..3], b"2.5");
        let mut num = [0u8; 10];
        let l = formatea_1_decimal(-0.05, &mut num);
        assert_eq!(&num[..l], b"-0.1");
        let mut num = [0u8; 10];
        assert_eq!(formatea_1_decimal(f64::NAN, &mut num), 1);
        let mut par = [0u8; 24];
        let l = formatea_par([1.0 / 3.0, 5.0 / 3.0], &mut par);
        assert_eq!(&par[..l], b"(0.3,1.7)");
    }

    #[test]
    fn defaults_48_frames_en_80x64() {
        let r = render_vectores_frames(80, 64);
        assert!(r.is_ok());
        if let Ok(set) = r {
            assert_eq!(set.len(), 48);
        }
        let r = render_transformacion_frames(80, 64);
        assert!(r.is_ok());
        let r = render_determinante_frames(80, 64);
        assert!(r.is_ok());
        let r = render_eigenvectores_frames(80, 64);
        assert!(r.is_ok());
        let r = render_cambio_base_frames(80, 64);
        assert!(r.is_ok());
        let r = render_producto_cruz_frames(80, 64);
        assert!(r.is_ok());
    }

    #[test]
    fn contenido_determinista_anima_y_rotula() {
        for id in TEMPLATE_IDS {
            let a = set_ok(id, 96, 64, 6);
            let b = set_ok(id, 96, 64, 6);
            assert_eq!(a, b, "{id} no determinista");
            let difieren = match (a.first(), a.last()) {
                (Some(primero), Some(ultimo)) => primero.pixels != ultimo.pixels,
                _ => false,
            };
            assert!(difieren, "{id} no anima");
            // Etiqueta: la barra superior tiene píxeles no-fondo.
            if let Some(primero) = a.first() {
                let mut tinta = 0usize;
                for y in 0..20 {
                    for x in 0..96 {
                        if let Some(p) = primero.pixel_en(x, y) {
                            if [p[0], p[1], p[2]] != [FONDO[0], FONDO[1], FONDO[2]]
                                && [p[0], p[1], p[2]] != [BARRA[0], BARRA[1], BARRA[2]]
                            {
                                tinta += 1;
                            }
                        }
                    }
                }
                assert!(tinta > 20, "{id} sin etiqueta visible ({tinta})");
            }
            // Un frame tiene fondo real (no todo negro).
            if let Some(medio) = a.get(3) {
                let fondo_px = medio
                    .pixels
                    .chunks(4)
                    .filter(|p| p[0] == FONDO[0] && p[1] == FONDO[1] && p[2] == FONDO[2]);
                assert!(fondo_px.count() > 0, "{id} sin fondo");
            }
        }
    }

    #[test]
    fn rango_empalma_exact() {
        let total = 8usize;
        let mut completo = Vec::new();
        for id in TEMPLATE_IDS {
            let r = render_linalg_frames(id, 96, 64, total);
            assert!(r.is_ok());
            if let Ok(set) = r {
                completo = set;
            }
            let r1 = render_linalg_rango(id, 96, 64, total, 0, 3);
            let r2 = render_linalg_rango(id, 96, 64, total, 3, 5);
            assert!(r1.is_ok() && r2.is_ok());
            if let (Ok(a), Ok(b)) = (r1, r2) {
                let mut pegado = a;
                pegado.extend(b);
                assert_eq!(pegado, completo, "{id} rango != completo");
            }
        }
    }

    #[test]
    fn errores_honestos() {
        assert!(render_linalg_frames("taylor-series", 96, 64, 4).is_err());
        assert!(render_linalg_frames(TEMPLATE_IDS[0], 0, 64, 4).is_err());
        assert!(render_linalg_frames(TEMPLATE_IDS[0], 96, 64, 0).is_err());
        assert!(render_linalg_frames(TEMPLATE_IDS[0], 96, 64, 1501).is_err());
        assert!(render_linalg_frames(TEMPLATE_IDS[0], 4096, 4096, 64).is_err());
        assert!(render_linalg_rango(TEMPLATE_IDS[0], 96, 64, 8, 5, 4).is_err());
        assert!(render_linalg_rango(TEMPLATE_IDS[0], 96, 64, 8, 0, 0).is_err());
        let r = RgbaFrame::nuevo(63, 64);
        assert!(r.is_err());
        let r = RgbaFrame::nuevo(96, 64);
        assert!(r.is_ok());
        if let Ok(f) = r {
            assert!(f.pixel_en(96, 0).is_none());
            assert!(f.pixel_en(0, 64).is_none());
            assert!(f.pixel_en(0, 0).is_some());
        }
    }
}
