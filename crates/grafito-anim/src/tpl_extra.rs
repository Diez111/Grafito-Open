//! Escenas extra de álgebra lineal nivel 3Blue1Brown (top-10 de paridad).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! (solo `std`). Autocontenido como `tpl_linalg.rs`: compila con
//! `rustc --test` standalone, sin cableado en `lib.rs`.
//!
//! ## Escenas (ver [`TEMPLATE_IDS`])
//!
//! | Id | Capítulo 3b1b | Idea |
//! |---|---|---|
//! | `matriz-inversa-nucleo` | EoLA 7 (inversas, columna, núcleo) | la grilla va `I→A→I`: `A⁻¹` deshace `A`; núcleo = `{0}` |
//! | `matriz-no-cuadrada` | EoLA 8 (matrices no cuadradas) | cubo de `R³` colapsa a `R²` a lo largo del núcleo |
//! | `producto-punto-dualidad` | EoLA 9 (punto y dualidad) | `v·w` como proyección con signo; `w` dualiza vectores en números |
//!
//! ## Formato de frame
//!
//! [`RgbaFrame`] es RGBA 8 bits por canal, fila-mayor, origen arriba-izq
//! (igual que el `buf` que `anim_native` mete en
//! `egui::ColorImage::from_rgba_unmultiplied`).
//!
//! ## Presupuestos (paridad con `protocol.rs`, sin importarlo)
//!
//! - Frames por set: 1..=64 en preview ([`EXTRA_MAX_FRAMES`]), hasta 1500
//!   en largo ([`EXTRA_LONGFORM_MAX_FRAMES`], 50 s a 30 fps).
//! - Set en RAM ≤ 64 MiB ([`EXTRA_CHUNK_MAX_BYTES`]): si excede es `Err`
//!   ([`ExtraError::SetDemasiadoGrande`]) y el frente compone por rangos
//!   con [`render_extra_rango`] drenando a disco.
//! - Lienzo 64..=4096 por lado (paridad `Resolution`).
//! - Samplers sin allocs por frame: la matemática por frame devuelve
//!   `Copy`; la única alloc por frame es el propio píxel-buffer de salida.

// ── Ids estables ──────────────────────────────────────────────────────────

/// Ids estables (kebab-case) de las 3 plantillas extra.
/// Sin colisión con las 65 canónicas de `protocol.rs::CANONICAL_TEMPLATES`
/// (verificado por lectura: ningún id de acá figura ahí).
pub const TEMPLATE_IDS: &[&str] = &[
    "matriz-inversa-nucleo",
    "matriz-no-cuadrada",
    "producto-punto-dualidad",
];

// ── Presupuestos ──────────────────────────────────────────────────────────

/// Ancho default del set (paridad con el canónico del chat 480×360).
pub const EXTRA_DEFAULT_W: u32 = 480;
/// Alto default del set.
pub const EXTRA_DEFAULT_H: u32 = 360;
/// Frames default por escena (paridad `NATIVE_ANIM_FRAME_COUNT`).
pub const EXTRA_DEFAULT_FRAMES: usize = 48;
/// Frames mínimos por pedido.
pub const EXTRA_MIN_FRAMES: usize = 1;
/// Tope de frames en preview (tope local GIF; el protocolo admite 180 en PNG).
pub const EXTRA_MAX_FRAMES: usize = 64;
/// Tope de frames en largo (tope local por llamada, 50 s a 30 fps; el
/// protocolo admite 3600 multi-chunk: lo que excede se pide por rangos).
pub const EXTRA_LONGFORM_MAX_FRAMES: usize = 1500;
/// Tope de bytes RGBA del set en RAM (tope local por chunk; el protocolo
/// compone chunks de 256 MiB).
pub const EXTRA_CHUNK_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Lado mínimo del lienzo (paridad `Resolution`).
pub const EXTRA_CANVAS_MIN: u32 = 64;
/// Lado máximo del lienzo (paridad `Resolution`).
pub const EXTRA_CANVAS_MAX: u32 = 4096;
/// Bytes por píxel RGBA.
pub const EXTRA_BYTES_POR_PIXEL: usize = 4;

// ── Error ─────────────────────────────────────────────────────────────────

/// Error honesto de las escenas (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtraError {
    /// Lienzo fuera de 64..=4096 por lado (o cero).
    LienzoInvalido { w: u32, h: u32 },
    /// Conteo de frames fuera de 1..=1500.
    FramesFueraDeRango { got: usize },
    /// Plantilla que no es de este módulo.
    PlantillaDesconocida { got: String },
    /// El set excede 64 MiB: bajar frames/lienzo o componer por rangos.
    SetDemasiadoGrande { bytes: usize },
    /// Parámetro matemático inválido (matriz singular, vector nulo, NaN…).
    ParametroFueraDeRango { detalle: String },
}

impl std::fmt::Display for ExtraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LienzoInvalido { w, h } => write!(
                f,
                "lienzo {w}x{h} inválido: usá {EXTRA_CANVAS_MIN}..={EXTRA_CANVAS_MAX} por lado"
            ),
            Self::FramesFueraDeRango { got } => write!(
                f,
                "{got} frames fuera de {EXTRA_MIN_FRAMES}..={EXTRA_LONGFORM_MAX_FRAMES}: \
                 preview hasta {EXTRA_MAX_FRAMES}, largo por rangos"
            ),
            Self::PlantillaDesconocida { got } => write!(
                f,
                "plantilla {got:?} desconocida: elegí una de {TEMPLATE_IDS:?}"
            ),
            Self::SetDemasiadoGrande { bytes } => write!(
                f,
                "set de {bytes} bytes excede {EXTRA_CHUNK_MAX_BYTES}: \
                 bajá frames/lienzo o componé por rangos con render_extra_rango"
            ),
            Self::ParametroFueraDeRango { detalle } => {
                write!(f, "parámetro fuera de rango: {detalle}")
            }
        }
    }
}

impl std::error::Error for ExtraError {}

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
    pub fn nuevo(width: u32, height: u32) -> Result<Self, ExtraError> {
        valida_lienzo(width, height)?;
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(EXTRA_BYTES_POR_PIXEL));
        match len {
            Some(n) => Ok(Self {
                width,
                height,
                pixels: vec![0u8; n],
            }),
            None => Err(ExtraError::SetDemasiadoGrande { bytes: usize::MAX }),
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
            .checked_mul(EXTRA_BYTES_POR_PIXEL)?;
        let p = self.pixels.get(i..i + EXTRA_BYTES_POR_PIXEL)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// Lienzo válido 64..=4096 por lado. Puro.
fn valida_lienzo(w: u32, h: u32) -> Result<(), ExtraError> {
    if !(EXTRA_CANVAS_MIN..=EXTRA_CANVAS_MAX).contains(&w)
        || !(EXTRA_CANVAS_MIN..=EXTRA_CANVAS_MAX).contains(&h)
    {
        return Err(ExtraError::LienzoInvalido { w, h });
    }
    Ok(())
}

// ── Matemática pura (todo Copy, sin allocs) ───────────────────────────────

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

/// Matriz 2×2 por columnas: `i-hat → (a, c)`, `j-hat → (b, d)`.
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

    /// Determinante `a·d − b·c`. No finito → 0 honesto.
    pub fn det(self) -> f64 {
        let v = self.a * self.d - self.b * self.c;
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }

    /// Interpolación lineal `self → otro` con `t` clamp 0..1. Pura.
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

/// Inversa de `m` por la fórmula cerrada 2×2.
/// `None` honesto si `det ≈ 0` o no finito (núcleo no trivial: no hay inversa).
pub fn inversa_de(m: Mat2) -> Option<Mat2> {
    let det = m.det();
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    let inv = Mat2 {
        a: m.d / det,
        b: -m.b / det,
        c: -m.c / det,
        d: m.a / det,
    };
    if inv.a.is_finite() && inv.b.is_finite() && inv.c.is_finite() && inv.d.is_finite() {
        Some(inv)
    } else {
        None
    }
}

// ── 1. Inversa / columna / núcleo (EoLA 7) ────────────────────────────────
//
// `A = [[2, 1], [0.5, 1.5]]`, `det = 2.5`, inversa exacta
// `A⁻¹ = [[0.6, −0.4], [−0.2, 0.8]]`. La animación deforma la grilla
// `I → A → I`: la vuelta la deshace la inversa, y el núcleo es `{0}`
// (solo el origen cae en el origen: la matriz es invertible).

/// Escena de la inversa: guarda `A` y su `A⁻¹` precalculada.
#[derive(Debug, Clone, Copy)]
pub struct InversaAnim {
    a: Mat2,
    ainv: Mat2,
}

impl InversaAnim {
    /// Constructor validado (entradas finitas, `|det| ≥ 1e-9`).
    pub fn try_new(a: f64, b: f64, c: f64, d: f64) -> Result<Self, ExtraError> {
        for (nombre, v) in [("a", a), ("b", b), ("c", c), ("d", d)] {
            if !v.is_finite() {
                return Err(ExtraError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: la matriz debe ser finita"),
                });
            }
        }
        let m = Mat2::nuevo(a, b, c, d);
        match inversa_de(m) {
            Some(ainv) => Ok(Self { a: m, ainv }),
            None => Err(ExtraError::ParametroFueraDeRango {
                detalle: format!(
                    "det={}: matriz singular (núcleo no trivial): elegí det distinto de 0",
                    m.det()
                ),
            }),
        }
    }

    /// Matriz directa.
    pub fn matriz(&self) -> Mat2 {
        self.a
    }

    /// Matriz inversa precalculada.
    pub fn inversa(&self) -> Mat2 {
        self.ainv
    }

    /// Determinante de `A`.
    pub fn det(&self) -> f64 {
        self.a.det()
    }

    /// Determinante de `A⁻¹` (= `1/det`).
    pub fn det_inv(&self) -> f64 {
        self.ainv.det()
    }

    /// Error máximo de `A·A⁻¹ − I` sobre la base canónica (debería ser ~0).
    /// Puro: la prueba de que la inversa deshace la directa.
    pub fn error_identidad(&self) -> f64 {
        let mut e = 0.0f64;
        for p in [[1.0, 0.0], [0.0, 1.0]] {
            let q = self.ainv.aplica(self.a.aplica(p));
            e = e.max((q[0] - p[0]).abs()).max((q[1] - p[1]).abs());
        }
        e
    }

    /// Matriz del morph en `t` global: `I → A` (ida) y `A → I` (vuelta
    /// que deshace la inversa). En `t = 0` y `t = 1` es la identidad.
    pub fn matriz_en(&self, t: f64) -> Mat2 {
        if t < 0.5 {
            Mat2::IDENTIDAD.interpola(self.a, suave(tramo(t, 0.10, 0.50)))
        } else {
            self.a
                .interpola(Mat2::IDENTIDAD, suave(tramo(t, 0.55, 0.95)))
        }
    }

    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        EXTRA_DEFAULT_FRAMES
    }
}

// ── 2. Matrices no cuadradas (EoLA 8) ─────────────────────────────────────
//
// `B = [[1, 0, 1], [0, 1, 1]]`: `B·(x, y, z) = (x+z, y+z)`.
// Rango 2 (filas independientes), núcleo = recta `span{(−1, −1, 1)}`
// (se calcula como `r1 × r2` normalizado: el núcleo es ortogonal a las
// filas). El cubo unidad de `R³` colapsa a `R²` a lo largo de esa recta.

/// Escena no cuadrada 2×3 (filas como vectores de `R³`).
#[derive(Debug, Clone, Copy)]
pub struct NoCuadradaAnim {
    filas: [[f64; 3]; 2],
}

impl NoCuadradaAnim {
    /// Constructor validado (entradas finitas, filas independientes:
    /// `|r1 × r2| ≥ 1e-9`, o sea rango 2 honesto).
    pub fn try_new(
        m11: f64,
        m12: f64,
        m13: f64,
        m21: f64,
        m22: f64,
        m23: f64,
    ) -> Result<Self, ExtraError> {
        for (nombre, v) in [
            ("m11", m11),
            ("m12", m12),
            ("m13", m13),
            ("m21", m21),
            ("m22", m22),
            ("m23", m23),
        ] {
            if !v.is_finite() {
                return Err(ExtraError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: la matriz debe ser finita"),
                });
            }
        }
        let filas = [[m11, m12, m13], [m21, m22, m23]];
        let n = cruz3(filas[0], filas[1]);
        let norma = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !norma.is_finite() || norma < 1e-9 {
            return Err(ExtraError::ParametroFueraDeRango {
                detalle: "filas dependientes (rango < 2): el núcleo no es una recta".to_string(),
            });
        }
        Ok(Self { filas })
    }

    /// Filas de la matriz.
    pub fn filas(&self) -> [[f64; 3]; 2] {
        self.filas
    }

    /// Rango (siempre 2: lo garantiza el constructor).
    pub fn rango(&self) -> usize {
        2
    }

    /// Aplica `B·p` con `p ∈ R³`; salida no finita → `[0, 0]`.
    pub fn aplica3(&self, p: [f64; 3]) -> [f64; 2] {
        let x = self.filas[0][0] * p[0] + self.filas[0][1] * p[1] + self.filas[0][2] * p[2];
        let y = self.filas[1][0] * p[0] + self.filas[1][1] * p[1] + self.filas[1][2] * p[2];
        if x.is_finite() && y.is_finite() {
            [x, y]
        } else {
            [0.0, 0.0]
        }
    }

    /// Dirección unitaria del núcleo (`r1 × r2` normalizada).
    /// `B·n = 0` exacto salvo redondeo. Pura.
    pub fn nucleo_dir(&self) -> [f64; 3] {
        let n = cruz3(self.filas[0], self.filas[1]);
        let norma = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !norma.is_finite() || norma <= 0.0 {
            return [0.0, 0.0, 0.0];
        }
        [n[0] / norma, n[1] / norma, n[2] / norma]
    }

    /// Ángulo de giro del cubo en `t` global (media vuelta: el colapso se ve
    /// desde todos lados). Puro.
    pub fn angulo_en(&self, t: f64) -> f64 {
        suave(tramo(t, 0.10, 0.90)) * std::f64::consts::PI
    }

    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        EXTRA_DEFAULT_FRAMES
    }
}

/// Producto cruz en `R³` (el núcleo como `r1 × r2`). No finito → cero.
fn cruz3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let r = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    if r[0].is_finite() && r[1].is_finite() && r[2].is_finite() {
        r
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// Giro rígido alrededor del eje `z` (el cubo rota sin deformarse). Puro.
pub fn rota_z(p: [f64; 3], th: f64) -> [f64; 3] {
    if !th.is_finite() {
        return [
            finito_o_cero(p[0]),
            finito_o_cero(p[1]),
            finito_o_cero(p[2]),
        ];
    }
    let (s, c) = th.sin_cos();
    [
        c * p[0] - s * p[1],
        s * p[0] + c * p[1],
        finito_o_cero(p[2]),
    ]
}

/// Proyección isométrica barata `R³ → R²` (solo para dibujar el dominio).
/// Pura.
fn iso(p: [f64; 3]) -> [f64; 2] {
    [p[0] - 0.4 * p[2], p[1] - 0.4 * p[2]]
}

/// Esquina `i` del cubo `[-1, 1]³` (bit 0/1/2 = signo de x/y/z). Pura.
fn esquina(i: usize) -> [f64; 3] {
    [
        if i & 1 == 0 { -1.0 } else { 1.0 },
        if i & 2 == 0 { -1.0 } else { 1.0 },
        if i & 4 == 0 { -1.0 } else { 1.0 },
    ]
}

/// Las 12 aristas del cubo como pares de esquinas.
const ARISTAS_CUBO: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

// ── 3. Producto punto y dualidad (EoLA 9) ─────────────────────────────────
//
// `w = (2, 1)` fijo: `|w| = √5`. `v(θ)` unitario barre la circunferencia y
// `v·w = |w|·cos(θ − φ)` oscila con signo (la proyección cae "atrás" cuando
// el ángulo pasa los 90°). Dualidad: `w` convierte vectores en números.

/// Producto punto `a·b` con guarda finita. Puro.
pub fn punto(a: [f64; 2], b: [f64; 2]) -> f64 {
    let v = a[0] * b[0] + a[1] * b[1];
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Escena del producto punto con `w` fijo no nulo.
#[derive(Debug, Clone, Copy)]
pub struct PuntoDualAnim {
    w: [f64; 2],
    w2: f64,
}

impl PuntoDualAnim {
    /// Constructor validado (`w` finito y no nulo: `|w| ≥ 1e-9`).
    pub fn try_new(wx: f64, wy: f64) -> Result<Self, ExtraError> {
        if !wx.is_finite() || !wy.is_finite() {
            return Err(ExtraError::ParametroFueraDeRango {
                detalle: format!("w=({wx}, {wy}): el vector dual debe ser finito"),
            });
        }
        let w2 = wx * wx + wy * wy;
        if !w2.is_finite() || w2 < 1e-18 {
            return Err(ExtraError::ParametroFueraDeRango {
                detalle: "w=0 no dualiza nada: elegí un vector no nulo".to_string(),
            });
        }
        Ok(Self { w: [wx, wy], w2 })
    }

    /// Vector dual fijo.
    pub fn w(&self) -> [f64; 2] {
        self.w
    }

    /// Norma de `w`.
    pub fn norma_w(&self) -> f64 {
        self.w2.sqrt()
    }

    /// `v·w` (el número que la dualidad asigna a `v`).
    pub fn prod(&self, v: [f64; 2]) -> f64 {
        punto(v, self.w)
    }

    /// Pie de la proyección de `v` sobre `w`: `((v·w)/(w·w))·w`. Puro.
    pub fn proyeccion(&self, v: [f64; 2]) -> [f64; 2] {
        if !v[0].is_finite() || !v[1].is_finite() || self.w2 <= 0.0 {
            return [0.0, 0.0];
        }
        let s = punto(v, self.w) / self.w2;
        if s.is_finite() {
            [s * self.w[0], s * self.w[1]]
        } else {
            [0.0, 0.0]
        }
    }

    /// `v` unitario en `t` global (radio [`PUNTO_RADIO`], vuelta completa).
    pub fn v_en(&self, t: f64) -> [f64; 2] {
        let th = suave(tramo(t, 0.10, 0.90)) * 2.0 * std::f64::consts::PI;
        let (s, c) = th.sin_cos();
        [PUNTO_RADIO * c, PUNTO_RADIO * s]
    }

    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        EXTRA_DEFAULT_FRAMES
    }
}

/// Radio de la circunferencia que barre `v` (entra en la vista ±4.5 con `w`).
pub const PUNTO_RADIO: f64 = 1.8;

// ── Presupuestos puros ────────────────────────────────────────────────────

/// Bytes RGBA del set (`w·h·4·frames`); `None` si desborda. Puro, sin allocs.
pub fn estima_bytes(w: u32, h: u32, frames: usize) -> Option<usize> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(EXTRA_BYTES_POR_PIXEL))
        .and_then(|v| v.checked_mul(frames))
}

/// ¿Cuántos frames de `w`×`h` entran en 64 MiB? Lados 0 o desborde → 0
/// honesto. Puro. (Pineado local con chunk de 64 MiB: 1280×720→18, 640×480→54; el protocolo compone chunks de 256 MiB y da 72/218.)
pub fn frames_por_chunk(w: u32, h: u32) -> usize {
    let por_frame = (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(EXTRA_BYTES_POR_PIXEL))
        .unwrap_or(0);
    if por_frame == 0 {
        return 0;
    }
    EXTRA_CHUNK_MAX_BYTES / por_frame
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
pub fn valida_pedido(w: u32, h: u32, frames: usize) -> Result<(), ExtraError> {
    valida_lienzo(w, h)?;
    if !(EXTRA_MIN_FRAMES..=EXTRA_LONGFORM_MAX_FRAMES).contains(&frames) {
        return Err(ExtraError::FramesFueraDeRango { got: frames });
    }
    match estima_bytes(w, h, frames) {
        Some(n) if n <= EXTRA_CHUNK_MAX_BYTES => Ok(()),
        Some(n) => Err(ExtraError::SetDemasiadoGrande { bytes: n }),
        None => Err(ExtraError::SetDemasiadoGrande { bytes: usize::MAX }),
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
const GRIS: [u8; 4] = [140, 150, 170, 255];

// ── Datos de las escenas ──────────────────────────────────────────────────

/// `A` de la inversa (EoLA 7; `det = 2·1.5 − 1·0.5 = 2.5` exacto).
const INV_A: Mat2 = Mat2 {
    a: 2.0,
    b: 1.0,
    c: 0.5,
    d: 1.5,
};
/// `B` no cuadrada (EoLA 8; filas independientes, rango 2).
const NOCUAD_FILAS: [[f64; 3]; 2] = [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
/// `w` dual fijo (EoLA 9; `|w| = √5`).
const PUNTO_W: [f64; 2] = [2.0, 1.0];

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
    let i = i.saturating_mul(EXTRA_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + EXTRA_BYTES_POR_PIXEL) {
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
    let i = ((y as usize) * w + (x as usize)).saturating_mul(EXTRA_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + EXTRA_BYTES_POR_PIXEL) {
        for k in 0..3 {
            let fondo = f32::from(p[k]);
            let tinta = f32::from(c[k]);
            p[k] = (fondo + (tinta - fondo) * a).round().clamp(0.0, 255.0) as u8;
        }
        p[3] = 255;
    }
}

/// Disco relleno de radio `r`. Puro.
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
    // Fast path: cubre todo el buffer → bulk fill sin chequeo por píxel.
    // Bit-idéntico al loop (mismos bytes que píxel a píxel).
    if x <= 0
        && y <= 0
        && (x as i64) + (rw as i64) >= (w as i64)
        && (y as i64) + (rh as i64) >= (h as i64)
        && buf.len() == w.saturating_mul(h).saturating_mul(4)
    {
        for px in buf.chunks_exact_mut(4) {
            px.copy_from_slice(&c);
        }
        return;
    }
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
/// Subdivide cada recta en 18 tramos.
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

/// Ejes con ticks enteros. Pura.
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
pub fn es_plantilla_extra(template: &str) -> bool {
    etiqueta_de(template).is_some()
}

/// Etiqueta en español de la escena (`None` si no es de este módulo).
pub fn etiqueta_de(template: &str) -> Option<&'static str> {
    match template.trim().to_lowercase().as_str() {
        "matriz-inversa-nucleo" => Some("INVERSA: A VUELVE ATRAS"),
        "matriz-no-cuadrada" => Some("NO CUADRADA: R3 A R2"),
        "producto-punto-dualidad" => Some("PUNTO: V.W DUAL"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escena {
    Inversa,
    NoCuadrada,
    Punto,
}

/// Resuelve el id a escena (`Err` honesto si no es de este módulo).
fn escena_de(template: &str) -> Result<Escena, ExtraError> {
    match template.trim().to_lowercase().as_str() {
        "matriz-inversa-nucleo" => Ok(Escena::Inversa),
        "matriz-no-cuadrada" => Ok(Escena::NoCuadrada),
        "producto-punto-dualidad" => Ok(Escena::Punto),
        _ => Err(ExtraError::PlantillaDesconocida {
            got: template.trim().to_string(),
        }),
    }
}

// ── Escenas (setup + animación + etiqueta) ────────────────────────────────

/// Escena de la inversa (EoLA 7): la grilla viaja `I → A → I`; las columnas
/// de `M(t)` (el espacio columna vivo) se dibujan, el paralelogramo que
/// generan se rellena, y el núcleo —un punto rojo en el origen— no se mueve
/// (solo el cero cae en el cero: hay inversa). El `DET` vivo vuelve a 1.
fn dibuja_inversa(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let anim = match InversaAnim::try_new(INV_A.a, INV_A.b, INV_A.c, INV_A.d) {
        Ok(a) => a,
        Err(_) => return,
    };
    let m = anim.matriz_en(t);
    dibuja_grilla(buf, w, h, v, m, 4, GRILLA_FANTASMA);
    let rampa = tramo(t, 0.10, 0.30);
    if rampa > 0.0 {
        let c1 = m.aplica([1.0, 0.0]);
        let c2 = m.aplica([0.0, 1.0]);
        let alfa = (0.35 * rampa) as f32;
        rellena_cuad(
            buf,
            w,
            h,
            v,
            [[0.0, 0.0], c1, [c1[0] + c2[0], c1[1] + c2[1]], c2],
            AMARILLO,
            alfa,
        );
        // Espacio columna: las columnas de M(t) en vivo.
        flecha(buf, w, h, v, [0.0, 0.0], c1, 3, VERDE);
        flecha(buf, w, h, v, [0.0, 0.0], c2, 3, ROJO);
    }
    // Núcleo = {0}: disco rojo fijo en el origen (no viaja con M).
    if let Some((ox, oy)) = mundo_a_px(v, 0.0, 0.0) {
        disco(buf, w, h, ox, oy, 3, ROJO);
    }
    // Valores vivos: det(M) y cartel del núcleo.
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(m.det(), &mut num);
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc, BLANCO, "DET=");
    texto_bytes(buf, w, h, x, y, esc, AMARILLO, &num[..ln]);
    texto(
        buf,
        w,
        h,
        8,
        y + 9 * esc.clamp(1, 4) as i32,
        esc,
        ROJO,
        "NUCLEO=0",
    );
}

/// Escena no cuadrada (EoLA 8): el cubo `[-1, 1]³` rota media vuelta; su
/// fantasma gris (proyección iso del dominio) convive con su imagen ámbar
/// `B·p` en el plano. La recta roja del núcleo colapsa al origen: tres
/// dimensiones entran en dos porque una se pierde por el núcleo.
fn dibuja_no_cuadrada(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let anim = match NoCuadradaAnim::try_new(
        NOCUAD_FILAS[0][0],
        NOCUAD_FILAS[0][1],
        NOCUAD_FILAS[0][2],
        NOCUAD_FILAS[1][0],
        NOCUAD_FILAS[1][1],
        NOCUAD_FILAS[1][2],
    ) {
        Ok(a) => a,
        Err(_) => return,
    };
    let th = anim.angulo_en(t);
    // Dominio (fantasma gris): cubo rotado en proyección iso.
    for arista in ARISTAS_CUBO {
        let a = iso(rota_z(esquina(arista[0]), th));
        let b = iso(rota_z(esquina(arista[1]), th));
        segmento(buf, w, h, v, a[0], a[1], b[0], b[1], 1, GRIS);
    }
    // Imagen (ámbar): B aplicada al cubo rotado.
    for arista in ARISTAS_CUBO {
        let a = anim.aplica3(rota_z(esquina(arista[0]), th));
        let b = anim.aplica3(rota_z(esquina(arista[1]), th));
        segmento(buf, w, h, v, a[0], a[1], b[0], b[1], 1, AMARILLO);
    }
    // Núcleo (rojo): la recta que el mapa aplasta al origen.
    let n = anim.nucleo_dir();
    let mut anterior: Option<(i32, i32)> = None;
    for k in -4..=4 {
        let s = f64::from(k) * 0.5;
        let p = [n[0] * s, n[1] * s, n[2] * s];
        let q = iso(rota_z(p, th));
        let px = mundo_a_px(v, q[0], q[1]);
        if let (Some(a), Some(b)) = (anterior, px) {
            segmento_px(buf, w, h, a.0, a.1, b.0, b.1, 2, ROJO);
        }
        anterior = px;
    }
    // La imagen del núcleo: un punto en el origen.
    if let Some((ox, oy)) = mundo_a_px(v, 0.0, 0.0) {
        disco(buf, w, h, ox, oy, 3, ROJO);
    }
    // Carteles fijos (rango 2 honesto por construcción).
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    texto(buf, w, h, 8, y, esc, BLANCO, "RANGO=2");
    texto(
        buf,
        w,
        h,
        8,
        y + 9 * esc.clamp(1, 4) as i32,
        esc,
        ROJO,
        "NUCLEO=RECTA",
    );
}

/// Escena del producto punto (EoLA 9): `w` fijo azul, `v` ámbar barriendo la
/// circunferencia; el pie verde es la proyección `((v·w)/(w·w))·w`, la
/// perpendicular punteada gris muestra la "sombra", y el valor vivo `V.W`
/// cambia de signo al pasar los 90° (dualidad: `w` vuelve vectores números).
fn dibuja_punto(buf: &mut [u8], w: usize, h: usize, v: &Vista, t: f64, esc: usize) {
    // 1. Setup: fondo + grilla + ejes.
    fondo(buf, w, h, v);
    // 2. Animación.
    let anim = match PuntoDualAnim::try_new(PUNTO_W[0], PUNTO_W[1]) {
        Ok(a) => a,
        Err(_) => return,
    };
    // Arco barrido hasta el ángulo actual (revelado progresivo: el
    // frame 0 muestra solo el punto y el último casi el círculo entero;
    // dibujar el círculo testigo completo haría f0 == f47).
    let th = {
        let u = suave(tramo(t, 0.10, 0.90));
        u * 2.0 * std::f64::consts::PI
    };
    let mut k = 0;
    while k < 64 {
        let a0 = 2.0 * std::f64::consts::PI * f64::from(k) / 64.0;
        if a0 >= th {
            break;
        }
        let a1 = 2.0 * std::f64::consts::PI * f64::from(k + 1) / 64.0;
        let a1 = if a1 > th { th } else { a1 };
        segmento(
            buf,
            w,
            h,
            v,
            PUNTO_RADIO * a0.cos(),
            PUNTO_RADIO * a0.sin(),
            PUNTO_RADIO * a1.cos(),
            PUNTO_RADIO * a1.sin(),
            0,
            GRILLA,
        );
        k += 1;
    }
    let vv = anim.v_en(t);
    let pie = anim.proyeccion(vv);
    // Proyección (origen → pie) y perpendicular punteada (pie → punta).
    segmento(buf, w, h, v, 0.0, 0.0, pie[0], pie[1], 2, VERDE);
    for k in 0..10 {
        if k % 2 == 0 {
            let s0 = f64::from(k) / 10.0;
            let s1 = f64::from(k + 1) / 10.0;
            segmento(
                buf,
                w,
                h,
                v,
                pie[0] + (vv[0] - pie[0]) * s0,
                pie[1] + (vv[1] - pie[1]) * s0,
                pie[0] + (vv[0] - pie[0]) * s1,
                pie[1] + (vv[1] - pie[1]) * s1,
                1,
                GRIS,
            );
        }
    }
    flecha(buf, w, h, v, [0.0, 0.0], anim.w(), 3, AZUL);
    flecha(buf, w, h, v, [0.0, 0.0], vv, 3, AMARILLO);
    if let Some((fx, fy)) = mundo_a_px(v, pie[0], pie[1]) {
        disco(buf, w, h, fx, fy, 3, VERDE);
    }
    // Valor vivo con signo.
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(anim.prod(vv), &mut num);
    let y = 7 * esc.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc, BLANCO, "V.W=");
    texto_bytes(buf, w, h, x, y, esc, VERDE, &num[..ln]);
    texto(
        buf,
        w,
        h,
        8,
        y + 9 * esc.clamp(1, 4) as i32,
        esc,
        GRIS,
        "V.W=W.V",
    );
}

/// Un frame de la escena en `t` global.
#[allow(clippy::too_many_arguments)]
fn dibuja_frame(
    esc: Escena,
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    etiqueta: &str,
    esc_texto: usize,
) {
    match esc {
        Escena::Inversa => dibuja_inversa(buf, w, h, v, t, esc_texto),
        Escena::NoCuadrada => dibuja_no_cuadrada(buf, w, h, v, t, esc_texto),
        Escena::Punto => dibuja_punto(buf, w, h, v, t, esc_texto),
    }
    // 3. Etiqueta (siempre, arriba).
    rotulo(buf, w, h, esc_texto, etiqueta);
}

// ── Renderers públicos ────────────────────────────────────────────────────

/// Núcleo: rango `[desde, desde+cantidad)` de un set de `total` frames
/// (el `t` global sale del índice global: el corte es empalme exacto).
/// Valida lienzo, rango y que el chunk entre en 64 MiB.
pub fn render_extra_rango(
    template: &str,
    width: u32,
    height: u32,
    total: usize,
    desde: usize,
    cantidad: usize,
) -> Result<Vec<RgbaFrame>, ExtraError> {
    let esc = escena_de(template)?;
    let etiqueta = etiqueta_de(template).unwrap_or("ALGEBRA LINEAL");
    valida_lienzo(width, height)?;
    if !(EXTRA_MIN_FRAMES..=EXTRA_LONGFORM_MAX_FRAMES).contains(&total) {
        return Err(ExtraError::FramesFueraDeRango { got: total });
    }
    if cantidad == 0 || desde.checked_add(cantidad).is_none_or(|fin| fin > total) {
        return Err(ExtraError::FramesFueraDeRango { got: cantidad });
    }
    match estima_bytes(width, height, cantidad) {
        Some(n) if n <= EXTRA_CHUNK_MAX_BYTES => {}
        Some(n) => return Err(ExtraError::SetDemasiadoGrande { bytes: n }),
        None => return Err(ExtraError::SetDemasiadoGrande { bytes: usize::MAX }),
    }
    let vista = vista_de(width, height);
    let esc_texto = escala_texto(width, height);
    let mut frames = Vec::new();
    if frames.try_reserve_exact(cantidad).is_err() {
        return Err(ExtraError::SetDemasiadoGrande { bytes: usize::MAX });
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
            etiqueta,
            esc_texto,
        );
        frames.push(frame);
    }
    Ok(frames)
}

/// Set completo de `frames` para la plantilla (valida el presupuesto de
/// 64 MiB: lo que no entra se pide por rangos con [`render_extra_rango`]).
pub fn render_extra_frames(
    template: &str,
    width: u32,
    height: u32,
    frames: usize,
) -> Result<Vec<RgbaFrame>, ExtraError> {
    valida_pedido(width, height, frames)?;
    render_extra_rango(template, width, height, frames, 0, frames)
}

/// Inversa / columna / núcleo (48 frames default).
pub fn render_inversa_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, ExtraError> {
    render_extra_frames("matriz-inversa-nucleo", width, height, EXTRA_DEFAULT_FRAMES)
}

/// Matrices no cuadradas (48 frames default).
pub fn render_no_cuadrada_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, ExtraError> {
    render_extra_frames("matriz-no-cuadrada", width, height, EXTRA_DEFAULT_FRAMES)
}

/// Producto punto y dualidad (48 frames default).
pub fn render_punto_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, ExtraError> {
    render_extra_frames(
        "producto-punto-dualidad",
        width,
        height,
        EXTRA_DEFAULT_FRAMES,
    )
}

// ── Tests inline ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Las 66 canónicas de `protocol.rs::CANONICAL_TEMPLATES` (copia pineada
    /// 2026-09-27, +sup-laplace-3d el 2026-09-28, para afirmar no-colisión; el dueño actualiza el registro
    /// al cablear `lib.rs` + `protocol.rs`).
    const EXISTENTES_66: &[&str] = &[
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
        "cambio-de-base",
        "determinante-area",
        "eigenvectores",
        "matriz-transformacion",
        "producto-cruz",
        "vectores-combinacion-lineal",
        "chain-rule",
        "epsilon-delta",
        "improper-integral",
        "ode-slope-field",
        "riemann-sums",
        "taylor-remainder",
        "double-integral",
        "gradient-descent",
        "green-stokes",
        "jacobian",
        "lagrange-multipliers",
        "partial-derivatives",
        "chaos-bifurcacion-barrido",
        "chaos-julia-morph",
        "chaos-lorenz",
        "chaos-mandelbrot-zoom",
        "chaos-pendulo-doble",
        "edo-calor-onda",
        "edo-campo-direcciones",
        "edo-convolucion",
        "edo-fourier-epiciclos",
        "edo-laplace",
        "backprop-flujo",
        "descenso-gradiente-3d",
        "distribuciones",
        "limite-central",
        "pca-rotacion",
        "perceptron-mlp",
        "regresion-lineal",
        "teorema-bayes",
        "sup-campo-vectorial",
        "sup-interseccion",
        "sup-laplace-3d",
        "sup-onda-3d",
        "sup-paraboloide-tangente",
        "sup-silla-descenso",
        "sup-toro-rotante",
        "celda-24",
        "estereografica",
        "hipercubo-corte",
        "simplex-nd",
        "tesseract-xw",
        "bfs-animado",
        "camino-minimo",
        "force-directed",
        "moser-spindle-coloreo",
        "unit-distance",
    ];

    fn set_ok(id: &str, w: u32, h: u32, n: usize) -> Vec<RgbaFrame> {
        let r = render_extra_frames(id, w, h, n);
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
    fn registro_tres_ids_kebab_sin_colision() {
        assert_eq!(
            TEMPLATE_IDS,
            &[
                "matriz-inversa-nucleo",
                "matriz-no-cuadrada",
                "producto-punto-dualidad",
            ]
        );
        assert_eq!(EXISTENTES_66.len(), 66, "la copia pineada debe ser 66");
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
            assert!(
                !EXISTENTES_66.contains(id),
                "{id} colisiona con una canónica existente"
            );
            assert!(es_plantilla_extra(id), "{id} no reconocido");
            assert!(
                etiqueta_de(id).is_some_and(|e| !e.is_empty()),
                "{id} sin etiqueta"
            );
            assert!(escena_de(id).is_ok(), "{id} sin escena");
        }
        assert!(!es_plantilla_extra("matriz-transformacion"));
        assert!(!es_plantilla_extra("producto-cruz"));
        assert!(!es_plantilla_extra(""));
        assert!(etiqueta_de("taylor-series").is_none());
        assert!(escena_de("taylor-series").is_err());
        // Case-insensitive con trim (como el dispatcher nativo).
        assert!(es_plantilla_extra("  Producto-Punto-Dualidad "));
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(EXTRA_DEFAULT_FRAMES, 48);
        assert_eq!((EXTRA_MIN_FRAMES, EXTRA_MAX_FRAMES), (1, 64));
        assert_eq!(EXTRA_LONGFORM_MAX_FRAMES, 1500);
        assert_eq!(EXTRA_CHUNK_MAX_BYTES, 64 * 1024 * 1024);
        assert_eq!((EXTRA_CANVAS_MIN, EXTRA_CANVAS_MAX), (64, 4096));
        assert_eq!(estima_bytes(480, 360, 48), Some(33_177_600));
        assert!(estima_bytes(480, 360, 48).is_some_and(|n| n <= EXTRA_CHUNK_MAX_BYTES));
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
        match InversaAnim::try_new(2.0, 1.0, 0.5, 1.5) {
            Ok(a) => assert_eq!(a.frames(), 48),
            Err(e) => panic!("A válida rechazada: {e}"),
        }
        match NoCuadradaAnim::try_new(1.0, 0.0, 1.0, 0.0, 1.0, 1.0) {
            Ok(a) => assert_eq!(a.frames(), 48),
            Err(e) => panic!("B válida rechazada: {e}"),
        }
        match PuntoDualAnim::try_new(2.0, 1.0) {
            Ok(a) => assert_eq!(a.frames(), 48),
            Err(e) => panic!("w válido rechazado: {e}"),
        }
    }

    #[test]
    fn matematica_inversa_exacta() {
        let inv = match InversaAnim::try_new(2.0, 1.0, 0.5, 1.5) {
            Ok(a) => a,
            Err(e) => panic!("A válida rechazada: {e}"),
        };
        // det = 2·1.5 − 1·0.5 = 2.5 exacto.
        assert!((inv.det() - 2.5).abs() < 1e-12, "det={}", inv.det());
        // A⁻¹ = [[0.6, −0.4], [−0.2, 0.8]] exacto.
        let ai = inv.inversa();
        assert!((ai.a - 0.6).abs() < 1e-12, "a={}", ai.a);
        assert!((ai.b + 0.4).abs() < 1e-12, "b={}", ai.b);
        assert!((ai.c + 0.2).abs() < 1e-12, "c={}", ai.c);
        assert!((ai.d - 0.8).abs() < 1e-12, "d={}", ai.d);
        // det(A⁻¹) = 1/det(A) = 0.4.
        assert!(
            (inv.det_inv() - 0.4).abs() < 1e-12,
            "det_inv={}",
            inv.det_inv()
        );
        // A·A⁻¹ = I (la inversa deshace la directa).
        assert!(
            inv.error_identidad() < 1e-12,
            "err={}",
            inv.error_identidad()
        );
        // Morph: arranca y vuelve a la identidad, pasando por A.
        let m0 = inv.matriz_en(0.0);
        assert_eq!(m0, Mat2::IDENTIDAD);
        let m1 = inv.matriz_en(1.0);
        assert!(
            (m1.det() - 1.0).abs() < 1e-12,
            "vuelve a I: det={}",
            m1.det()
        );
        let mm = inv.matriz_en(0.5);
        assert!(
            (mm.det() - 2.5).abs() < 1e-9,
            "pasa por A: det={}",
            mm.det()
        );
        // Bordes honestos: singular, NaN e infinito no pasan.
        assert!(InversaAnim::try_new(1.0, 2.0, 2.0, 4.0).is_err());
        assert!(InversaAnim::try_new(0.0, 0.0, 0.0, 0.0).is_err());
        assert!(InversaAnim::try_new(f64::NAN, 1.0, 0.5, 1.5).is_err());
        assert!(InversaAnim::try_new(2.0, 1.0, 0.5, f64::INFINITY).is_err());
        assert!(inversa_de(Mat2::nuevo(1.0, 2.0, 2.0, 4.0)).is_none());
    }

    #[test]
    fn matematica_no_cuadrada_exacta() {
        let b = match NoCuadradaAnim::try_new(1.0, 0.0, 1.0, 0.0, 1.0, 1.0) {
            Ok(v) => v,
            Err(e) => panic!("B válida rechazada: {e}"),
        };
        assert_eq!(b.rango(), 2);
        // B·(1,2,3) = (1+3, 2+3) = (4,5).
        let q = b.aplica3([1.0, 2.0, 3.0]);
        assert!(
            (q[0] - 4.0).abs() < 1e-12 && (q[1] - 5.0).abs() < 1e-12,
            "q={q:?}"
        );
        // El núcleo contiene a (−1,−1,1): B·n = (0,0).
        let n = b.nucleo_dir();
        let img = b.aplica3(n);
        assert!(img[0].abs() < 1e-12 && img[1].abs() < 1e-12, "B·n={img:?}");
        // La dirección es ±(−1,−1,1)/√3.
        let esperado = 1.0 / 3.0f64.sqrt();
        let alin = (n[0] * -esperado + n[1] * -esperado + n[2] * esperado).abs();
        assert!((alin - 1.0).abs() < 1e-12, "n={n:?}");
        // Todo el núcleo cae en el origen: n·s también.
        let img2 = b.aplica3([n[0] * 2.5, n[1] * 2.5, n[2] * 2.5]);
        assert!(img2[0].abs() < 1e-12 && img2[1].abs() < 1e-12);
        // Giro rígido: rota_z(90°) lleva (1,0,0) a (0,1,0), z intacto.
        let r = rota_z([1.0, 0.0, 2.0], std::f64::consts::FRAC_PI_2);
        assert!(r[0].abs() < 1e-12 && (r[1] - 1.0).abs() < 1e-12 && (r[2] - 2.0).abs() < 1e-12);
        // El cubo tiene 8 esquinas y 12 aristas sanas.
        assert_eq!(ARISTAS_CUBO.len(), 12);
        for i in 0..8 {
            let e = esquina(i);
            assert!(e.iter().all(|v| v.abs() == 1.0), "esquina {i}: {e:?}");
        }
        // Ángulo: quieto al inicio, media vuelta al final.
        assert_eq!(b.angulo_en(0.0), 0.0);
        assert!((b.angulo_en(1.0) - std::f64::consts::PI).abs() < 1e-12);
        // Bordes honestos: filas dependientes, NaN e infinito no pasan.
        assert!(NoCuadradaAnim::try_new(1.0, 2.0, 3.0, 2.0, 4.0, 6.0).is_err());
        assert!(NoCuadradaAnim::try_new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_err());
        assert!(NoCuadradaAnim::try_new(f64::NAN, 0.0, 1.0, 0.0, 1.0, 1.0).is_err());
    }

    #[test]
    fn matematica_punto_exacta() {
        // Caso libre: (1,2)·(3,4) = 11, simétrico.
        assert!((punto([1.0, 2.0], [3.0, 4.0]) - 11.0).abs() < 1e-12);
        assert!((punto([1.0, 2.0], [3.0, 4.0]) - punto([3.0, 4.0], [1.0, 2.0])).abs() < 1e-12);
        // Cauchy-Schwarz en un caso no trivial.
        let a: [f64; 2] = [1.5, -2.0];
        let c: [f64; 2] = [-0.5, 3.0];
        let na = (a[0] * a[0] + a[1] * a[1]).sqrt();
        let nc = (c[0] * c[0] + c[1] * c[1]).sqrt();
        assert!(punto(a, c).abs() <= na * nc + 1e-12);
        let dual = match PuntoDualAnim::try_new(PUNTO_W[0], PUNTO_W[1]) {
            Ok(v) => v,
            Err(e) => panic!("w válido rechazado: {e}"),
        };
        // |w| = √5.
        assert!((dual.norma_w() - 5.0f64.sqrt()).abs() < 1e-12);
        // (1,0)·w = 2 (la componente x de w, como debe ser).
        assert!((dual.prod([1.0, 0.0]) - 2.0).abs() < 1e-12);
        // Pie sobre (1,0): ((1,0)·w)/(w·w)·w = (2/5)·(2,1) = (0.8, 0.4).
        let pie = dual.proyeccion([1.0, 0.0]);
        assert!(
            (pie[0] - 0.8).abs() < 1e-12 && (pie[1] - 0.4).abs() < 1e-12,
            "pie={pie:?}"
        );
        // El pie es paralelo a w y el resto perpendicular.
        let resto = [1.0 - pie[0], 0.0 - pie[1]];
        assert!(
            punto(resto, dual.w()).abs() < 1e-12,
            "resto·w={}",
            punto(resto, dual.w())
        );
        // Ortogonal da cero: (−1,2)·(2,1) = 0.
        assert!(dual.prod([-1.0, 2.0]).abs() < 1e-12);
        // v_en barre unitario (radio PUNTO_RADIO): |v| = radio siempre.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let vv = dual.v_en(t);
            let norma = (vv[0] * vv[0] + vv[1] * vv[1]).sqrt();
            assert!((norma - PUNTO_RADIO).abs() < 1e-12, "t={t}: {vv:?}");
        }
        // El máximo sobre el barrido es |w|·radio (alineado con w).
        let maximo = dual.norma_w() * PUNTO_RADIO;
        for t in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0] {
            assert!(dual.prod(dual.v_en(t)) <= maximo + 1e-9, "t={t}");
        }
        // Bordes honestos: w nulo, NaN e infinito no pasan.
        assert!(PuntoDualAnim::try_new(0.0, 0.0).is_err());
        assert!(PuntoDualAnim::try_new(f64::NAN, 1.0).is_err());
        assert!(PuntoDualAnim::try_new(2.0, f64::INFINITY).is_err());
        assert_eq!(punto([f64::NAN, 0.0], [1.0, 1.0]), 0.0);
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
        // Charset dinámico (carteles fijos + valores con decimal).
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:;=+-|()? ".chars() {
            if let Some(b) = mayus_ascii(ch) {
                assert!(glifo(b).is_some(), "sin glifo {ch:?}");
            }
        }
        for cartel in [
            "NUCLEO=0",
            "RANGO=2",
            "NUCLEO=RECTA",
            "V.W=",
            "V.W=W.V",
            "DET=",
        ] {
            for ch in cartel.chars() {
                let b = mayus_ascii(ch);
                assert!(b.is_some(), "{cartel}: char sin mapa {ch:?}");
                if let Some(byte) = b {
                    assert!(glifo(byte).is_some(), "{cartel}: sin glifo {ch:?}");
                }
            }
        }
        let mut num = [0u8; 10];
        assert_eq!(formatea_1_decimal(2.5, &mut num), 3);
        assert_eq!(&num[..3], b"2.5");
        let mut num = [0u8; 10];
        let l = formatea_1_decimal(-4.6, &mut num);
        assert_eq!(&num[..l], b"-4.6");
        let mut num = [0u8; 10];
        assert_eq!(formatea_1_decimal(f64::NAN, &mut num), 1);
    }

    #[test]
    fn defaults_48_frames_en_80x64() {
        let r = render_inversa_frames(80, 64);
        assert!(r.is_ok());
        if let Ok(set) = r {
            assert_eq!(set.len(), 48);
        }
        let r = render_no_cuadrada_frames(80, 64);
        assert!(r.is_ok());
        let r = render_punto_frames(80, 64);
        assert!(r.is_ok());
        if let Ok(set) = r {
            assert_eq!(set.len(), 48);
        }
    }

    #[test]
    fn contenido_determinista_anima_y_rotula() {
        for id in TEMPLATE_IDS {
            let a = set_ok(id, 96, 64, 6);
            let b = set_ok(id, 96, 64, 6);
            assert_eq!(a, b, "{id} no determinista");
            // Algún par consecutivo difiere (la escena anima).
            let mut cambia = false;
            for par in a.windows(2) {
                if par[0].pixels != par[1].pixels {
                    cambia = true;
                    break;
                }
            }
            assert!(cambia, "{id} no anima");
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
            let r = render_extra_frames(id, 96, 64, total);
            assert!(r.is_ok());
            if let Ok(set) = r {
                completo = set;
            }
            let r1 = render_extra_rango(id, 96, 64, total, 0, 3);
            let r2 = render_extra_rango(id, 96, 64, total, 3, 5);
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
        assert!(render_extra_frames("taylor-series", 96, 64, 4).is_err());
        assert!(render_extra_frames(TEMPLATE_IDS[0], 0, 64, 4).is_err());
        assert!(render_extra_frames(TEMPLATE_IDS[0], 96, 64, 0).is_err());
        assert!(render_extra_frames(TEMPLATE_IDS[0], 96, 64, 1501).is_err());
        assert!(render_extra_frames(TEMPLATE_IDS[0], 4096, 4096, 64).is_err());
        assert!(render_extra_rango(TEMPLATE_IDS[0], 96, 64, 8, 5, 4).is_err());
        assert!(render_extra_rango(TEMPLATE_IDS[0], 96, 64, 8, 0, 0).is_err());
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
