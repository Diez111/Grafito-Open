//! Escenas 4D y politopos animados (tesseract, 16/24/120/600-cell, simplex ND).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! (solo `std`). Matemática propia en CPU: rotaciones en los planos XW/YW/ZW
//! con matrices 4×4, proyección perspectiva 4D→3D→2D y proyección
//! estereográfica, más secciones 3D (`w = corte`) barriendo el hipercubo.
//! Cada plantilla es una escena didáctica con setup + animación + etiqueta,
//! en paleta noche-violeta (fondo casi negro, aristas cian/violeta por
//! profundidad, corte en verde).
//!
//! ## Escenas (ver [`TEMPLATE_IDS`])
//!
//! | Id | Idea |
//! |---|---|
//! | `tesseract-xw` | hipercubo rotando en el plano XW (vuelta completa) |
//! | `celda-24` | 24-cell en rotación doble XW+ZW (isoclínica) |
//! | `hipercubo-corte` | plano `w = c` barriendo el hipercubo (−1.35..1.35) |
//! | `estereografica` | 600-cell sobre la 3-esfera, proyección estereográfica |
//! | `simplex-nd` | simplex regular ND (default N=4: 5-cell) rotando en XW |
//!
//! ## Pipeline de proyección
//!
//! 1. Rota los vértices 4D con [`Mat4`] (planos XW/YW/ZW).
//! 2. 4D→3D: perspectiva `p₃ = p.xyz · d/(d−w)` ([`perspectiva_4d_a_3d`]) o
//!    estereográfica desde el polo `p₃ = p.xyz / (1−w)` sobre la 3-esfera
//!    unidad ([`estereografica_a_3d`], aristas dibujadas como cuerdas rectas
//!    de los arcos reales —ver nota abajo—).
//! 3. 3D→2D: pinhole look-at [`paso_3d_a_2d`], paridad exacta con
//!    `scene.rs::Camera::project_3d` (mismo forward/right/up y
//!    `escala = 1/(tan(fov/2)·profundidad)`); este módulo no lo importa para
//!    seguir compilando standalone con `rustc --test`.
//!
//! Nota honesta: la proyección estereográfica es conforme (preserva ángulos)
//! y manda las aristas rectas 4D a arcos de círculo 3D; acá se dibujan como
//! segmentos rectos entre vértices proyectados (cuerdas), no como arcos.
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
//! - Frames por set: 1..=64 en preview ([`D4D_MAX_FRAMES`], paridad
//!   `PREVIEW_SHORT_MAX_FRAMES`), hasta 1500 en largo
//!   ([`D4D_LONGFORM_MAX_FRAMES`], 50 s a 30 fps ≤ 60 s del timeline).
//! - Set en RAM ≤ 64 MiB ([`D4D_CHUNK_MAX_BYTES`]): si excede es `Err`
//!   ([`Error4D::SetDemasiadoGrande`]) y el frente compone por rangos
//!   con [`render_4d_rango`] drenando a disco.
//! - Lienzo 64..=4096 por lado (paridad `Resolution`).
//! - Vértices ≤ 600 y aristas ≤ 1200 por escena ([`D4D_MAX_VERTICES`] /
//!   [`D4D_MAX_ARISTAS`]): el 120-cell entra exacto (600v/1200a).
//! - Samplers sin allocs por frame: la única alloc por frame es el propio
//!   píxel-buffer de salida (inherente: cada frame es dueño). Los politopos
//!   se generan una vez en el setup (el 120-cell: 600 vértices + 179 700
//!   pares de distancias en dos pasadas, sin guardar matriz).
//!
//! Fuentes:
//! - Rotaciones 4D por planos y matrices por par de ejes (XY/XZ/XW/YZ/YW/ZW,
//!   `M[a,a]=M[b,b]=cos`, `M[a,b]=−M[b,a]=sin`): notas GI1988 de proyección
//!   4D y `baileysnyder.com/interactive-4d/rotations` (6 planos; rotar se
//!   describe por planos, no por ejes).
//! - Tesseract 16v/32e, perspectiva `escala = d/(d−w)` con `d = 2.0`,
//!   rotaciones XW/YW/ZW: ejemplo `viz_tesseract` de FURY.
//! - Estereográfica conforme (ángulos preservados, aristas curvas) y
//!   24-cell autodual de 24 vértices / 96 aristas: `4d.pardesco.com`.
//! - Proyección central del toro de Clifford 4D y giro inside-out:
//!   Banchoff, `math.brown.edu/~tbanchof/Beyond3d` cap. 6.
//! - 600-cell 120v/720a (8 + 16 + 96 de permutaciones pares de
//!   `(0, ±½, ±φ/2, ±φ⁻¹/2)`), 120-cell 600v/1200a en marco de radio √8
//!   (tipos 24 + 64·3 + 96·2 + 192): Wikipedia `120-cell` + generador
//!   `probe_120cell_generator.py` (vfd-crystallisation).
//! - Conteos pineados en el workspace: `grafito-geometry/src/polytopes.rs`
//!   (tesseract 16/32, 16-cell 8/24, 24-cell 24/96, 120-cell 600/1200,
//!   600-cell 120/720).

// ── Ids estables ──────────────────────────────────────────────────────────

/// Ids estables (kebab-case) de las 5 plantillas 4D.
/// El frente los cablea al dispatcher nativo; acá ya los atiende
/// [`render_4d_frames`].
pub const TEMPLATE_IDS: &[&str] = &[
    "tesseract-xw",
    "celda-24",
    "hipercubo-corte",
    "estereografica",
    "simplex-nd",
];

// ── Presupuestos ──────────────────────────────────────────────────────────

/// Ancho default del set (paridad con el canónico del chat 480×360).
pub const D4D_DEFAULT_W: u32 = 480;
/// Alto default del set.
pub const D4D_DEFAULT_H: u32 = 360;
/// Frames default por escena (paridad `NATIVE_ANIM_FRAME_COUNT`).
pub const D4D_DEFAULT_FRAMES: usize = 48;
/// Frames mínimos por pedido.
pub const D4D_MIN_FRAMES: usize = 1;
/// Tope de frames en preview (paridad `PREVIEW_SHORT_MAX_FRAMES`).
pub const D4D_MAX_FRAMES: usize = 64;
/// Tope de frames en largo (paridad `VIDEO_LONGFORM_MAX_FRAMES`: 50 s a 30 fps).
pub const D4D_LONGFORM_MAX_FRAMES: usize = 1500;
/// Tope de bytes RGBA del set en RAM (paridad `LONGFORM_CHUNK_MAX_BYTES`).
pub const D4D_CHUNK_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Lado mínimo del lienzo (paridad `Resolution`).
pub const D4D_CANVAS_MIN: u32 = 64;
/// Lado máximo del lienzo (paridad `Resolution`).
pub const D4D_CANVAS_MAX: u32 = 4096;
/// Bytes por píxel RGBA.
pub const D4D_BYTES_POR_PIXEL: usize = 4;
/// Vértices máximos por escena (el 120-cell entra exacto con 600).
pub const D4D_MAX_VERTICES: usize = 600;
/// Aristas máximas por escena (el 120-cell entra exacto con 1200).
pub const D4D_MAX_ARISTAS: usize = 1200;
/// Dimensión máxima del simplex ND (vértices N+1 ≤ 9, aristas ≤ 36).
pub const D4D_SIMPLEX_N_MAX: usize = 8;
/// Dimensión mínima del simplex ND (el 1-simplex es un segmento; el 2, un triángulo).
pub const D4D_SIMPLEX_N_MIN: usize = 2;
/// Dimensión default del simplex (N=4: el 5-cell).
pub const D4D_SIMPLEX_N_DEFAULT: usize = 4;

// ── Error ─────────────────────────────────────────────────────────────────

/// Error honesto de las escenas (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error4D {
    /// Lienzo fuera de 64..=4096 por lado (o cero).
    LienzoInvalido { w: u32, h: u32 },
    /// Conteo de frames fuera de 1..=1500.
    FramesFueraDeRango { got: usize },
    /// Plantilla que no es de este módulo.
    PlantillaDesconocida { got: String },
    /// El set excede 64 MiB: bajar frames/lienzo o componer por rangos.
    SetDemasiadoGrande { bytes: usize },
    /// El politopo excede vértices/aristas (defensa interna, hoy inalcanzable).
    PolitopoDemasiadoGrande { vertices: usize, aristas: usize },
    /// Dimensión del simplex fuera de 2..=8.
    SimplexInvalido { n: usize },
}

impl std::fmt::Display for Error4D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LienzoInvalido { w, h } => write!(
                f,
                "lienzo {w}x{h} inválido: usá {D4D_CANVAS_MIN}..={D4D_CANVAS_MAX} por lado"
            ),
            Self::FramesFueraDeRango { got } => write!(
                f,
                "{got} frames fuera de {D4D_MIN_FRAMES}..={D4D_LONGFORM_MAX_FRAMES}: \
                 preview hasta {D4D_MAX_FRAMES}, largo por rangos"
            ),
            Self::PlantillaDesconocida { got } => write!(
                f,
                "plantilla {got:?} desconocida: elegí una de {TEMPLATE_IDS:?}"
            ),
            Self::SetDemasiadoGrande { bytes } => write!(
                f,
                "set de {bytes} bytes excede {D4D_CHUNK_MAX_BYTES}: \
                 bajá frames/lienzo o componé por rangos con render_4d_rango"
            ),
            Self::PolitopoDemasiadoGrande { vertices, aristas } => write!(
                f,
                "politopo de {vertices}v/{aristas}a excede \
                 {D4D_MAX_VERTICES}v/{D4D_MAX_ARISTAS}a: recortá la resolución del politopo"
            ),
            Self::SimplexInvalido { n } => write!(
                f,
                "simplex N={n} fuera de {D4D_SIMPLEX_N_MIN}..={D4D_SIMPLEX_N_MAX}"
            ),
        }
    }
}

impl std::error::Error for Error4D {}

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
    pub fn nuevo(width: u32, height: u32) -> Result<Self, Error4D> {
        valida_lienzo(width, height)?;
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(D4D_BYTES_POR_PIXEL));
        match len {
            Some(n) => Ok(Self {
                width,
                height,
                pixels: vec![0u8; n],
            }),
            None => Err(Error4D::SetDemasiadoGrande { bytes: usize::MAX }),
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
            .checked_mul(D4D_BYTES_POR_PIXEL)?;
        let p = self.pixels.get(i..i + D4D_BYTES_POR_PIXEL)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// Lienzo válido 64..=4096 por lado. Puro.
fn valida_lienzo(w: u32, h: u32) -> Result<(), Error4D> {
    if !(D4D_CANVAS_MIN..=D4D_CANVAS_MAX).contains(&w)
        || !(D4D_CANVAS_MIN..=D4D_CANVAS_MAX).contains(&h)
    {
        return Err(Error4D::LienzoInvalido { w, h });
    }
    Ok(())
}

// ── Matemática pura (sin allocs, con guardias finitas) ────────────────────

/// Finito o cero (guarda de los samplers). Puro.
fn finito_o_cero(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Smoothstep `3t²−2t³` (paridad `RateFunc::Smooth`). Clamp 0..1. Puro.
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

/// Punto 4D (`Copy`, sin allocs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4 {
    /// Coordenadas `[x, y, z, w]`.
    pub c: [f64; 4],
}

impl Vec4 {
    /// Constructor (guarda finita: `NaN`/inf → 0 honesto, sin pánico).
    pub fn nuevo(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self {
            c: [
                finito_o_cero(x),
                finito_o_cero(y),
                finito_o_cero(z),
                finito_o_cero(w),
            ],
        }
    }

    /// Norma al cuadrado. No finita → 0 honesto. Pura.
    pub fn norma2(self) -> f64 {
        let v = self.c[0] * self.c[0]
            + self.c[1] * self.c[1]
            + self.c[2] * self.c[2]
            + self.c[3] * self.c[3];
        if v.is_finite() {
            v
        } else {
            0.0
        }
    }

    /// Norma euclídea. Pura.
    pub fn norma(self) -> f64 {
        self.norma2().sqrt()
    }

    /// Distancia al cuadrado a otro punto. Pura.
    pub fn dist2(self, otro: Self) -> f64 {
        let mut v = 0.0;
        for k in 0..4 {
            let d = self.c[k] - otro.c[k];
            v += d * d;
        }
        if v.is_finite() {
            v
        } else {
            f64::INFINITY
        }
    }

    /// Escala uniforme (no finita → identidad honesta). Pura.
    pub fn escala(self, s: f64) -> Self {
        if !s.is_finite() {
            return self;
        }
        Self::nuevo(self.c[0] * s, self.c[1] * s, self.c[2] * s, self.c[3] * s)
    }
}

/// Matriz 4×4 por filas (`Copy`, sin allocs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Filas `m[fila][col]`.
    pub m: [[f64; 4]; 4],
}

impl Mat4 {
    /// Identidad (sin rotación).
    pub const IDENTIDAD: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Rotación en el plano de los ejes `a`/`b` (`0=x, 1=y, 2=z, 3=w`):
    /// `M[a,a]=M[b,b]=cos`, `M[a,b]=−sin`, `M[b,a]=+sin` (convención
    /// matemática estándar, como el ejemplo `R_xy`/`R_zw` de FURY).
    /// Ejes iguales o fuera de rango → identidad honesta. Pura.
    pub fn rot_plano(a: usize, b: usize, ang: f64) -> Self {
        if a == b || a > 3 || b > 3 || !ang.is_finite() {
            return Self::IDENTIDAD;
        }
        let (s, c) = ang.sin_cos();
        let (s, c) = (finito_o_cero(s), finito_o_cero(c));
        let mut m = Self::IDENTIDAD;
        m.m[a][a] = c;
        m.m[b][b] = c;
        m.m[a][b] = -s;
        m.m[b][a] = s;
        m
    }

    /// Rotación en el plano XW (la 4D por excelencia: mezcla `x` con `w`).
    pub fn rot_xw(ang: f64) -> Self {
        Self::rot_plano(0, 3, ang)
    }

    /// Rotación en el plano YW.
    pub fn rot_yw(ang: f64) -> Self {
        Self::rot_plano(1, 3, ang)
    }

    /// Rotación en el plano ZW.
    pub fn rot_zw(ang: f64) -> Self {
        Self::rot_plano(2, 3, ang)
    }

    /// Composición `self ∘ otra` (aplica `otra` primero). Pura.
    pub fn compone(self, otra: Self) -> Self {
        let mut m = [[0.0; 4]; 4];
        for (i, fila) in m.iter_mut().enumerate() {
            for (j, celda) in fila.iter_mut().enumerate() {
                let mut v = 0.0;
                for k in 0..4 {
                    v += self.m[i][k] * otra.m[k][j];
                }
                *celda = finito_o_cero(v);
            }
        }
        Self { m }
    }

    /// Aplica la matriz a un punto (columna). Pura.
    pub fn aplica(self, p: Vec4) -> Vec4 {
        let mut c = [0.0; 4];
        for (i, celda) in c.iter_mut().enumerate() {
            let mut v = 0.0;
            for k in 0..4 {
                v += self.m[i][k] * p.c[k];
            }
            *celda = finito_o_cero(v);
        }
        Vec4 { c }
    }
}

/// Proyección perspectiva 4D→3D: `p₃ = p.xyz · d/(d−w)` (como el ejemplo
/// `viz_tesseract` de FURY con `d = 2.0`).
/// `None` honesto si el punto cae en el plano de la cámara (`d−w ≤ d·0.15`,
/// evita el blow-up) o algo no es finito. Pura.
pub fn perspectiva_4d_a_3d(p: Vec4, d: f64) -> Option<[f64; 3]> {
    if !d.is_finite() || d <= 0.0 {
        return None;
    }
    let denom = d - p.c[3];
    if !denom.is_finite() || denom <= d * 0.15 {
        return None;
    }
    let s = d / denom;
    if !s.is_finite() {
        return None;
    }
    let q = [p.c[0] * s, p.c[1] * s, p.c[2] * s];
    if q.iter().all(|v| v.is_finite()) {
        Some(q)
    } else {
        None
    }
}

/// Distancia 4D default de la cámara (`d = 2.6`: magnificación contenida
/// 0.64..2.0 para vértices de radio ≤ 1.5).
pub const D4D_DISTANCIA_4D: f64 = 2.6;

/// Proyección estereográfica 3D desde el polo norte de la 3-esfera unidad:
/// `p₃ = p.xyz / (1−w)` (conforme: preserva ángulos; las aristas rectas 4D
/// se vuelven arcos —acá se dibujan como cuerdas—).
/// Requiere la 3-esfera unidad (los generadores ya normalizan cuando hace
/// falta); `None` honesto cerca del polo (`1−w < 1/3`, proyección al
/// infinito) o si algo no es finito. Pura.
pub fn estereografica_a_3d(p: Vec4) -> Option<[f64; 3]> {
    let denom = 1.0 - p.c[3];
    if !denom.is_finite() || denom < 1.0 / 3.0 {
        return None;
    }
    let q = [p.c[0] / denom, p.c[1] / denom, p.c[2] / denom];
    if q.iter().all(|v| v.is_finite()) {
        Some(q)
    } else {
        None
    }
}

/// Proyección pinhole 3D→2D look-at, paridad exacta con
/// `scene.rs::Camera::project_3d`: `forward = center−eye` normalizado,
/// `right = forward × up` (`up = [0,1,0]`, o `[1,0,0]` si degenera),
/// `up2 = right × forward`, `x/y = dot · 1/(tan(fov/2)·profundidad)`.
/// `None` honesto si el punto cae detrás (`profundidad ≤ 0`), la base
/// degenera o algo no es finito. `fov_rad` en radianes (1°..179°). Pura.
pub fn paso_3d_a_2d(
    p: [f64; 3],
    eye: [f64; 3],
    center: [f64; 3],
    fov_rad: f64,
) -> Option<[f64; 2]> {
    if !p.iter().all(|v| v.is_finite())
        || !eye.iter().all(|v| v.is_finite())
        || !center.iter().all(|v| v.is_finite())
        || !fov_rad.is_finite()
    {
        return None;
    }
    let fov_deg = fov_rad * 180.0 / std::f64::consts::PI;
    if !(1.0..=179.0).contains(&fov_deg) {
        return None;
    }
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
    // right = forward × [0,1,0] = (−fz, 0, fx).
    let mut rx = -fz;
    let mut ry = 0.0;
    let mut rz = fx;
    let mut rlen = (rx * rx + ry * ry + rz * rz).sqrt();
    if !rlen.is_finite() || rlen < 1e-9 {
        // forward ∥ up: up = [1,0,0] → right = (0, fz, −fy).
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
    let mitad = (fov_rad / 2.0).tan();
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

/// Ojo 3D fijo de las escenas (mira al origen desde `+z`).
pub const D4D_EYE: [f64; 3] = [0.0, 0.0, 5.0];
/// Objetivo 3D fijo (origen).
pub const D4D_CENTER: [f64; 3] = [0.0, 0.0, 0.0];
/// FOV vertical fijo de las escenas (42°).
pub const D4D_FOV_RAD: f64 = 42.0 * std::f64::consts::PI / 180.0;

// ── Politopos (generación procedural con cotas) ───────────────────────────

/// Politopo 4D generable (más el simplex ND con `n` validado 2..=8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Politopo {
    /// Hipercubo: 16v/32e (`±1` por coordenada).
    Tesseract,
    /// 16-cell: 8v/24e (permutaciones de `(±1,0,0,0)`).
    Cell16,
    /// 24-cell: 24v/96e (8 + 16 de `(±½,±½,±½,±½)`).
    Cell24,
    /// 120-cell: 600v/1200a (marco de radio √8, 7 tipos por permutaciones).
    Cell120,
    /// 600-cell: 120v/720a (8 + 16 + 96 de permutaciones pares).
    Cell600,
    /// Simplex regular de dimensión `n` (N+1 vértices, grafo completo).
    Simplex(usize),
}

impl Politopo {
    /// Vértices esperados (pineado con `polytopes.rs`; el simplex: N+1).
    /// `None` honesto si el simplex trae `n` fuera de rango.
    pub fn vertices_esperados(self) -> Option<usize> {
        match self {
            Self::Tesseract => Some(16),
            Self::Cell16 => Some(8),
            Self::Cell24 => Some(24),
            Self::Cell120 => Some(600),
            Self::Cell600 => Some(120),
            Self::Simplex(n) => {
                if (D4D_SIMPLEX_N_MIN..=D4D_SIMPLEX_N_MAX).contains(&n) {
                    Some(n + 1)
                } else {
                    None
                }
            }
        }
    }

    /// Aristas esperadas (pineado; el simplex: `(N+1)·N/2`).
    /// `None` honesto si el simplex trae `n` fuera de rango.
    pub fn aristas_esperadas(self) -> Option<usize> {
        match self {
            Self::Tesseract => Some(32),
            Self::Cell16 => Some(24),
            Self::Cell24 => Some(96),
            Self::Cell120 => Some(1200),
            Self::Cell600 => Some(720),
            Self::Simplex(n) => {
                if (D4D_SIMPLEX_N_MIN..=D4D_SIMPLEX_N_MAX).contains(&n) {
                    Some((n + 1) * n / 2)
                } else {
                    None
                }
            }
        }
    }
}

/// Genera vértices y aristas del politopo (índices `i < j` ordenados).
/// Normaliza el radio máximo a 1.5 (escala uniforme: preserva regularidad)
/// salvo el simplex, que ya sale de arista ~1 con radio < 1.
/// `Err` honesto si excede [`D4D_MAX_VERTICES`]/[`D4D_MAX_ARISTAS`] o el
/// simplex trae `n` inválido. La única alloc vive acá (setup, no por frame).
pub fn genera_politopo(p: Politopo) -> Result<(Vec<Vec4>, Vec<[usize; 2]>), Error4D> {
    let (mut verts, aristas) = match p {
        Politopo::Tesseract => genera_tesseract(),
        Politopo::Cell16 => genera_16cell(),
        Politopo::Cell24 => genera_24cell(),
        Politopo::Cell120 => genera_120cell(),
        Politopo::Cell600 => genera_600cell(),
        Politopo::Simplex(n) => genera_simplex(n)?,
    };
    if verts.len() > D4D_MAX_VERTICES || aristas.len() > D4D_MAX_ARISTAS {
        return Err(Error4D::PolitopoDemasiadoGrande {
            vertices: verts.len(),
            aristas: aristas.len(),
        });
    }
    if !matches!(p, Politopo::Simplex(_)) {
        normaliza_radio(&mut verts, 1.5);
    }
    Ok((verts, aristas))
}

/// Escala uniforme para que el radio máximo sea `radio` (regularidad
/// intacta; radio no finito o ya ≤ radio → no toca nada). Puro sobre el buffer.
fn normaliza_radio(verts: &mut [Vec4], radio: f64) {
    if !radio.is_finite() || radio <= 0.0 {
        return;
    }
    let mut max2 = 0.0;
    for v in verts.iter() {
        let n2 = v.norma2();
        if n2 > max2 {
            max2 = n2;
        }
    }
    if !max2.is_finite() || max2 <= 0.0 {
        return;
    }
    let s = radio / max2.sqrt();
    if s.is_finite() && s > 0.0 && (s - 1.0).abs() > 1e-12 {
        for v in verts.iter_mut() {
            *v = v.escala(s);
        }
    }
}

/// Hipercubo: las 16 combinaciones de `±1`; arista = Hamming 1 (32, exacto).
fn genera_tesseract() -> (Vec<Vec4>, Vec<[usize; 2]>) {
    let mut verts = Vec::with_capacity(16);
    for k in 0..16 {
        verts.push(Vec4::nuevo(
            if k & 1 == 0 { -1.0 } else { 1.0 },
            if k & 2 == 0 { -1.0 } else { 1.0 },
            if k & 4 == 0 { -1.0 } else { 1.0 },
            if k & 8 == 0 { -1.0 } else { 1.0 },
        ));
    }
    let mut aristas = Vec::with_capacity(32);
    for i in 0..16 {
        for b in 0..4 {
            let j = i ^ (1 << b);
            if i < j {
                aristas.push([i, j]);
            }
        }
    }
    (verts, aristas)
}

/// 16-cell: 8 permutaciones de `(±1,0,0,0)`; aristas por distancia mínima.
fn genera_16cell() -> (Vec<Vec4>, Vec<[usize; 2]>) {
    let mut verts = Vec::with_capacity(8);
    for eje in 0..4 {
        for s in [-1.0, 1.0] {
            let mut c = [0.0; 4];
            c[eje] = s;
            verts.push(Vec4 { c });
        }
    }
    let aristas = aristas_por_distancia_minima(&verts, 24);
    (verts, aristas)
}

/// 24-cell: 8 de `(±1,0,0,0)` + 16 de `(±½,±½,±½,±½)`; aristas por mínima.
fn genera_24cell() -> (Vec<Vec4>, Vec<[usize; 2]>) {
    let (mut verts, _) = genera_16cell();
    verts.reserve(16);
    for k in 0..16 {
        verts.push(Vec4::nuevo(
            if k & 1 == 0 { -0.5 } else { 0.5 },
            if k & 2 == 0 { -0.5 } else { 0.5 },
            if k & 4 == 0 { -0.5 } else { 0.5 },
            if k & 8 == 0 { -0.5 } else { 0.5 },
        ));
    }
    let aristas = aristas_por_distancia_minima(&verts, 96);
    (verts, aristas)
}

/// 600-cell de radio unidad: 8 + 16 + 96 (permutaciones pares de
/// `(0, ±½, ±φ/2, ±φ⁻¹/2)`); aristas por distancia mínima (720).
fn genera_600cell() -> (Vec<Vec4>, Vec<[usize; 2]>) {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let inv_phi = 1.0 / phi;
    let mut verts = Vec::with_capacity(120);
    for eje in 0..4 {
        for s in [-1.0, 1.0] {
            let mut c = [0.0; 4];
            c[eje] = s;
            verts.push(Vec4 { c });
        }
    }
    for k in 0..16 {
        verts.push(Vec4::nuevo(
            if k & 1 == 0 { -0.5 } else { 0.5 },
            if k & 2 == 0 { -0.5 } else { 0.5 },
            if k & 4 == 0 { -0.5 } else { 0.5 },
            if k & 8 == 0 { -0.5 } else { 0.5 },
        ));
    }
    let base = [0.0, 0.5, phi / 2.0, inv_phi / 2.0];
    for perm in permutaciones_pares() {
        for mask in 0..8 {
            let mut c = [0.0; 4];
            let mut bit = 0;
            for (pos, eje) in perm.iter().enumerate() {
                let v = base[pos];
                if v == 0.0 {
                    c[*eje] = 0.0;
                } else {
                    c[*eje] = if (mask >> bit) & 1 == 0 { -v } else { v };
                    bit += 1;
                }
            }
            verts.push(Vec4 { c });
        }
    }
    let aristas = aristas_por_distancia_minima(&verts, 720);
    (verts, aristas)
}

/// 120-cell en marco de radio √8 (arista `3−√5 ≈ 0.764): 24 + 64·3 + 96·2
/// + 192 = 600 vértices; aristas por distancia mínima (1200).
fn genera_120cell() -> (Vec<Vec4>, Vec<[usize; 2]>) {
    let sqrt5 = 5.0_f64.sqrt();
    let phi = (1.0 + sqrt5) / 2.0;
    let inv_phi = 1.0 / phi; // φ−1 ≈ 0.618
    let phi2 = phi * phi; // φ² ≈ 2.618
    let inv_phi2 = 1.0 / phi2; // φ⁻² ≈ 0.382
    let mut verts = Vec::with_capacity(600);
    // Tipo 1: 24 de (0, 0, ±2, ±2): 6 pares de posiciones × 4 signos.
    for a in 0..4 {
        for b in (a + 1)..4 {
            for sa in [-2.0, 2.0] {
                for sb in [-2.0, 2.0] {
                    let mut c = [0.0; 4];
                    c[a] = sa;
                    c[b] = sb;
                    verts.push(Vec4 { c });
                }
            }
        }
    }
    // Tipos 2–4: 64 cada uno (4 posiciones × 16 signos).
    for pos in 0..4 {
        for mask in 0..16 {
            let s = |k: usize| {
                if (mask >> k) & 1 == 0 {
                    -1.0
                } else {
                    1.0
                }
            };
            // Tipo 2: (±1, ±1, ±1, ±√5).
            let mut c2 = [s(0), s(1), s(2), s(3)];
            c2[pos] = s(pos) * sqrt5;
            verts.push(Vec4 { c: c2 });
            // Tipo 3: (±φ², ±φ⁻¹, ±φ⁻¹, ±φ⁻¹) con φ² en `pos`.
            let mut c3 = [
                s(0) * inv_phi,
                s(1) * inv_phi,
                s(2) * inv_phi,
                s(3) * inv_phi,
            ];
            c3[pos] = s(pos) * phi2;
            verts.push(Vec4 { c: c3 });
            // Tipo 4: (±φ, ±φ, ±φ, ±φ⁻²) con φ⁻² en `pos`.
            let mut c4 = [s(0) * phi, s(1) * phi, s(2) * phi, s(3) * phi];
            c4[pos] = s(pos) * inv_phi2;
            verts.push(Vec4 { c: c4 });
        }
    }
    // Tipos 5–6: 96 cada uno (12 permutaciones pares × 8 signos).
    for perm in permutaciones_pares() {
        for mask in 0..8 {
            let s = |k: usize| {
                if (mask >> k) & 1 == 0 {
                    -1.0
                } else {
                    1.0
                }
            };
            // Tipo 5: (0, ±φ⁻², ±1, ±φ²).
            let mut c5 = [0.0; 4];
            c5[perm[1]] = s(0) * inv_phi2;
            c5[perm[2]] = s(1);
            c5[perm[3]] = s(2) * phi2;
            verts.push(Vec4 { c: c5 });
            // Tipo 6: (0, ±φ⁻¹, ±φ, ±√5).
            let mut c6 = [0.0; 4];
            c6[perm[1]] = s(0) * inv_phi;
            c6[perm[2]] = s(1) * phi;
            c6[perm[3]] = s(2) * sqrt5;
            verts.push(Vec4 { c: c6 });
        }
    }
    // Tipo 7: 192 (12 permutaciones pares × 16 signos de (±φ⁻¹, ±1, ±φ, ±2)).
    for perm in permutaciones_pares() {
        for mask in 0..16 {
            let s = |k: usize| {
                if (mask >> k) & 1 == 0 {
                    -1.0
                } else {
                    1.0
                }
            };
            let mut c = [0.0; 4];
            c[perm[0]] = s(0) * inv_phi;
            c[perm[1]] = s(1);
            c[perm[2]] = s(2) * phi;
            c[perm[3]] = s(3) * 2.0;
            verts.push(Vec4 { c });
        }
    }
    let aristas = aristas_por_distancia_minima(&verts, 1200);
    (verts, aristas)
}

/// Las 12 permutaciones pares de `[0,1,2,3]` (paridad por inversiones).
fn permutaciones_pares() -> Vec<[usize; 4]> {
    let mut salida = Vec::with_capacity(12);
    for a in 0..4 {
        for b in 0..4 {
            if b == a {
                continue;
            }
            for c in 0..4 {
                if c == a || c == b {
                    continue;
                }
                for d in 0..4 {
                    if d == a || d == b || d == c {
                        continue;
                    }
                    let p = [a, b, c, d];
                    let mut inv = 0;
                    for i in 0..4 {
                        for j in (i + 1)..4 {
                            if p[i] > p[j] {
                                inv += 1;
                            }
                        }
                    }
                    if inv % 2 == 0 {
                        salida.push(p);
                    }
                }
            }
        }
    }
    salida
}

/// Aristas por distancia mínima entre pares (dos pasadas, sin guardar
/// matriz: 1ª el mínimo, 2ª las que caen dentro de `1e-6` relativo).
/// Corta en `esperadas` (los politopos regulares cortan exacto; si el mínimo
/// no es finito devuelve vacío honesto). Puro salvo la alloc de salida.
fn aristas_por_distancia_minima(verts: &[Vec4], esperadas: usize) -> Vec<[usize; 2]> {
    let n = verts.len();
    if n < 2 {
        return Vec::new();
    }
    let mut min = f64::INFINITY;
    for i in 0..n {
        for j in (i + 1)..n {
            let d = verts[i].dist2(verts[j]);
            if d > 1e-12 && d < min {
                min = d;
            }
        }
    }
    if !min.is_finite() {
        return Vec::new();
    }
    let tol = min.max(1.0) * 1e-6;
    let mut aristas = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = verts[i].dist2(verts[j]);
            if (d - min).abs() <= tol {
                if aristas.len() >= esperadas {
                    break;
                }
                aristas.push([i, j]);
            }
        }
        if aristas.len() >= esperadas {
            break;
        }
    }
    aristas
}

/// Simplex regular de dimensión `n` (arista 1, centroide en el origen):
/// el 1-simplex es `[∓½]`; cada nivel incrusta el anterior en `z = −a` y suma
/// el ápice en `z = +n·a` con `a = 1/√(2n(n+1))` (arista ápice–base = 1
/// exacto por construcción). Aristas = grafo completo (todas equidistantes
/// en R^N).
/// Dimensiones > 4 se guardan descartando coordenadas extra: la sombra 4D es
/// ortográfica honesta pero YA no es regular (distancias desiguales) ni
/// centrada; solo N ≤ 4 conserva regularidad exacta en lo guardado.
fn genera_simplex(n: usize) -> Result<(Vec<Vec4>, Vec<[usize; 2]>), Error4D> {
    if !(D4D_SIMPLEX_N_MIN..=D4D_SIMPLEX_N_MAX).contains(&n) {
        return Err(Error4D::SimplexInvalido { n });
    }
    // Puntos en R^n como Vec<f64> (setup; n ≤ 8).
    let mut pts: Vec<Vec<f64>> = vec![vec![-0.5], vec![0.5]];
    for dim in 2..=n {
        let d = dim as f64;
        let a = 1.0 / (2.0 * d * (d + 1.0)).sqrt();
        let mut next = Vec::with_capacity(pts.len() + 1);
        for p in pts.iter() {
            let mut q = Vec::with_capacity(dim);
            for v in p.iter() {
                q.push(*v);
            }
            q.push(-a);
            next.push(q);
        }
        let mut apice = vec![0.0; dim - 1];
        apice.push(d * a);
        next.push(apice);
        pts = next;
    }
    let mut verts = Vec::with_capacity(n + 1);
    for p in pts.iter() {
        let mut c = [0.0; 4];
        for (k, v) in p.iter().enumerate().take(4) {
            c[k] = *v;
        }
        verts.push(Vec4::nuevo(c[0], c[1], c[2], c[3]));
    }
    let mut aristas = Vec::with_capacity((n + 1) * n / 2);
    for i in 0..verts.len() {
        for j in (i + 1)..verts.len() {
            aristas.push([i, j]);
        }
    }
    Ok((verts, aristas))
}

/// Caras cuadradas del tesseract para la escena de corte: por cada vértice y
/// cada par de ejes, el quad `{v, v+e_a, v+e_a+e_b, v+e_b}` (ciclo ordenado),
/// deduplicado por clave ordenada (24 caras exactas).
pub fn caras_hipercubo() -> Vec<[usize; 4]> {
    let mut caras = Vec::with_capacity(24);
    for v in 0..16 {
        for a in 0..4 {
            for b in (a + 1)..4 {
                let j = v ^ (1 << a);
                let l = v ^ (1 << a) ^ (1 << b);
                let k = v ^ (1 << b);
                let mut clave = [v, j, l, k];
                clave.sort_unstable();
                let mut vista = false;
                for c in caras.iter() {
                    if *c == clave {
                        vista = true;
                        break;
                    }
                }
                if !vista {
                    caras.push(clave);
                }
            }
        }
    }
    caras
}

/// Intersección del politopo con el hiperplano `w = corte`: por cada cara
/// quad corta sus 4 aristas (cruce estricto con `t = sa/(sa−sb)`); 2 puntos
/// → un segmento 3D. Vértice exacto sobre el plano = medida cero durante el
/// barrido (se salta honesto, sin segmento degenerado).
/// Devuelve los segmentos 3D (vacíos si el plano no toca nada). Pura salvo alloc.
pub fn seccion_w(verts: &[Vec4], caras: &[[usize; 4]], corte: f64) -> Vec<([f64; 3], [f64; 3])> {
    let mut segs = Vec::new();
    if !corte.is_finite() || verts.is_empty() {
        return segs;
    }
    for cara in caras.iter() {
        let quad = [cara[0], cara[1], cara[2], cara[3]];
        let mut pts: [[f64; 3]; 4] = [[0.0; 3]; 4];
        let mut npts = 0;
        for e in 0..4 {
            let ia = quad[e];
            let ib = quad[(e + 1) % 4];
            let (Some(a), Some(b)) = (verts.get(ia), verts.get(ib)) else {
                continue;
            };
            let sa = a.c[3] - corte;
            let sb = b.c[3] - corte;
            if sa * sb < 0.0 {
                let t = sa / (sa - sb);
                if t.is_finite() && npts < 4 {
                    pts[npts] = [
                        a.c[0] + (b.c[0] - a.c[0]) * t,
                        a.c[1] + (b.c[1] - a.c[1]) * t,
                        a.c[2] + (b.c[2] - a.c[2]) * t,
                    ];
                    npts += 1;
                }
            }
        }
        if npts == 2 {
            segs.push((pts[0], pts[1]));
        }
    }
    segs
}

// ── Presupuestos puros ────────────────────────────────────────────────────

/// Bytes RGBA del set (`w·h·4·frames`); `None` si desborda. Puro, sin allocs.
pub fn estima_bytes(w: u32, h: u32, frames: usize) -> Option<usize> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(D4D_BYTES_POR_PIXEL))
        .and_then(|v| v.checked_mul(frames))
}

/// ¿Cuántos frames de `w`×`h` entran en 64 MiB? Lados 0 o desborde → 0
/// honesto. Puro. (Pineado: 1280×720→18, 640×480→54, como en el protocolo.)
pub fn frames_por_chunk(w: u32, h: u32) -> usize {
    let por_frame = (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(D4D_BYTES_POR_PIXEL))
        .unwrap_or(0);
    if por_frame == 0 {
        return 0;
    }
    D4D_CHUNK_MAX_BYTES / por_frame
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
pub fn valida_pedido(w: u32, h: u32, frames: usize) -> Result<(), Error4D> {
    valida_lienzo(w, h)?;
    if !(D4D_MIN_FRAMES..=D4D_LONGFORM_MAX_FRAMES).contains(&frames) {
        return Err(Error4D::FramesFueraDeRango { got: frames });
    }
    match estima_bytes(w, h, frames) {
        Some(n) if n <= D4D_CHUNK_MAX_BYTES => Ok(()),
        Some(n) => Err(Error4D::SetDemasiadoGrande { bytes: n }),
        None => Err(Error4D::SetDemasiadoGrande { bytes: usize::MAX }),
    }
}

// ── Paleta noche-violeta ──────────────────────────────────────────────────

const FONDO: [u8; 4] = [8, 10, 20, 255];
const BARRA: [u8; 4] = [5, 7, 15, 255];
const REJILLA: [u8; 4] = [30, 34, 58, 255];
const BLANCO: [u8; 4] = [232, 236, 248, 255];
const CIAN: [u8; 4] = [90, 220, 255, 255];
const VIOLETA: [u8; 4] = [168, 140, 255, 255];
const MAGENTA: [u8; 4] = [255, 120, 200, 255];
const VERDE: [u8; 4] = [110, 230, 150, 255];
const AMBAR: [u8; 4] = [255, 200, 90, 255];

// ── Raster CPU (puro, con chequeo de bordes, sin pánicos) ─────────────────

/// Vista mundo→píxel (`y` hacia arriba en mundo, abajo en píxel).
struct Vista {
    w: usize,
    h: usize,
    escala: f64,
}

/// Vista con `±D4D_MEDIO_MUNDO` unidades en la dimensión menor. Pura.
const D4D_MEDIO_MUNDO: f64 = 2.4;

/// Vista del raster. Pura.
fn vista_de(w: u32, h: u32) -> Vista {
    let menor = (w.min(h) as f64).max(1.0);
    Vista {
        w: w as usize,
        h: h as usize,
        escala: menor / (2.0 * D4D_MEDIO_MUNDO),
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
    let i = i.saturating_mul(D4D_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + D4D_BYTES_POR_PIXEL) {
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
    let i = ((y as usize) * w + (x as usize)).saturating_mul(D4D_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + D4D_BYTES_POR_PIXEL) {
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
    let r = r.min(48);
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                pinta(buf, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Segmento en píxeles con grosor y alfa (DDA determinista). Puro.
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
    alfa: f32,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let pasos = dx.abs().max(dy.abs()).max(0);
    if pasos == 0 {
        disco(buf, w, h, x0, y0, grosor / 2, c);
        return;
    }
    let radio = (grosor / 2).clamp(0, 48);
    for s in 0..=pasos {
        let x = x0 + dx.saturating_mul(s) / pasos;
        let y = y0 + dy.saturating_mul(s) / pasos;
        if radio == 0 {
            mezcla(buf, w, h, x, y, c, alfa);
        } else {
            // Disco con alfa: mezcla por píxel del disco.
            for dy2 in -radio..=radio {
                for dx2 in -radio..=radio {
                    if dx2 * dx2 + dy2 * dy2 <= radio * radio {
                        mezcla(buf, w, h, x + dx2, y + dy2, c, alfa);
                    }
                }
            }
        }
    }
}

/// Segmento mundo→píxel (silencioso si un extremo no proyecta). Puro.
#[allow(clippy::too_many_arguments)]
fn segmento_mundo(
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
    alfa: f32,
) {
    if let (Some((ax, ay)), Some((bx, by))) = (mundo_a_px(v, x0, y0), mundo_a_px(v, x1, y1)) {
        segmento_px(buf, w, h, ax, ay, bx, by, grosor, c, alfa);
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

/// Fondo + cruz de ejes tenue. El setup de todas las escenas.
fn fondo(buf: &mut [u8], w: usize, h: usize, v: &Vista) {
    rect_lleno(buf, w, h, 0, 0, w as i32, h as i32, FONDO);
    segmento_mundo(buf, w, h, v, -2.2, 0.0, 2.2, 0.0, 0, REJILLA, 1.0);
    segmento_mundo(buf, w, h, v, 0.0, -2.2, 0.0, 2.2, 0, REJILLA, 1.0);
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
        b'/' => Some([0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10]),
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
        ' ' | '.' | ',' | ':' | '=' | '+' | '-' | '/' | '(' | ')' | '?' => Some(ch as u8),
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
        pinta(buf, w, h, x, alto, REJILLA);
    }
    alto + 1
}

// ── Registro público ──────────────────────────────────────────────────────

/// ¿Atiende este módulo la plantilla (case-insensitive, con trim)?
pub fn es_plantilla_4d(template: &str) -> bool {
    etiqueta_de(template).is_some()
}

/// Etiqueta en español de la escena (`None` si no es de este módulo).
pub fn etiqueta_de(template: &str) -> Option<&'static str> {
    match template.trim().to_lowercase().as_str() {
        "tesseract-xw" => Some("TESERACTO: GIRO EN XW"),
        "celda-24" => Some("24-CELL: ROTACION DOBLE"),
        "hipercubo-corte" => Some("CORTE W: BARRIENDO EL HIPERCUBO"),
        "estereografica" => Some("ESTEREOGRAFICA: 600-CELL"),
        "simplex-nd" => Some("SIMPLEX ND EN 4D"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escena {
    TesseractXw,
    Celda24,
    Corte,
    Estereografica,
    Simplex,
}

/// Resuelve el id a escena (`Err` honesto si no es de este módulo).
fn escena_de(template: &str) -> Result<Escena, Error4D> {
    match template.trim().to_lowercase().as_str() {
        "tesseract-xw" => Ok(Escena::TesseractXw),
        "celda-24" => Ok(Escena::Celda24),
        "hipercubo-corte" => Ok(Escena::Corte),
        "estereografica" => Ok(Escena::Estereografica),
        "simplex-nd" => Ok(Escena::Simplex),
        _ => Err(Error4D::PlantillaDesconocida {
            got: template.trim().to_string(),
        }),
    }
}

// ── Dibujo de wireframes 4D ───────────────────────────────────────────────

/// Proyecta y dibuja un wireframe 4D ya rotado.
///
/// `proy` mapea cada vértice rotado a 3D (`None` = vértice clipped: sus
/// aristas se saltan). El color mezcla cian→violeta por `w` (cerca→lejos en
/// la cámara 4D) y el alfa atenúa con la profundidad 3D. Sin orden painter
/// (cero allocs por frame); sin pánicos.
#[allow(clippy::too_many_arguments)]
fn dibuja_wireframe<F>(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    rotados: &[Vec4],
    aristas: &[[usize; 2]],
    proy: F,
    grosor: i32,
) where
    F: Fn(Vec4) -> Option<[f64; 3]>,
{
    // Proyecta a 2D sobre un buffer de stack fijo (máx. 600 vértices).
    // Más vértices que el tope se recortan honesto (el setup ya acota).
    const MAXV: usize = D4D_MAX_VERTICES;
    let mut pts2: [[f64; 2]; MAXV] = [[0.0; 2]; MAXV];
    let mut ok = [false; MAXV];
    let mut prof: [f64; MAXV] = [0.0; MAXV];
    let n = rotados.len().min(MAXV);
    for (i, rv) in rotados.iter().enumerate().take(n) {
        if let Some(p3) = proy(*rv) {
            if let Some(p2) = paso_3d_a_2d(p3, D4D_EYE, D4D_CENTER, D4D_FOV_RAD) {
                pts2[i] = p2;
                ok[i] = true;
                prof[i] = p3[2];
            }
        }
    }
    // Rango de profundidad para el alfa (plano → alfa 1 parejo).
    let mut pmin = f64::INFINITY;
    let mut pmax = f64::NEG_INFINITY;
    for i in 0..n {
        if ok[i] {
            if prof[i] < pmin {
                pmin = prof[i];
            }
            if prof[i] > pmax {
                pmax = prof[i];
            }
        }
    }
    if !pmin.is_finite() || !pmax.is_finite() || (pmax - pmin) < 1e-9 {
        pmin = 0.0;
        pmax = 1.0;
    }
    for arista in aristas.iter() {
        let (a, b) = (arista[0], arista[1]);
        if a >= n || b >= n || !ok[a] || !ok[b] {
            continue;
        }
        // Color por `w` medio: cerca (w+) cian, lejos violeta.
        let wm = (rotados[a].c[3] + rotados[b].c[3]) / 2.0;
        let u = ((wm / 1.5 + 1.0) / 2.0).clamp(0.0, 1.0);
        let c = [
            (VIOLETA[0] as f64 + (CIAN[0] as f64 - VIOLETA[0] as f64) * u) as u8,
            (VIOLETA[1] as f64 + (CIAN[1] as f64 - VIOLETA[1] as f64) * u) as u8,
            (VIOLETA[2] as f64 + (CIAN[2] as f64 - VIOLETA[2] as f64) * u) as u8,
            255,
        ];
        let pm = (prof[a] + prof[b]) / 2.0;
        let alfa = (0.35 + 0.65 * (1.0 - ((pm - pmin) / (pmax - pmin)).clamp(0.0, 1.0))) as f32;
        segmento_mundo(
            buf, w, h, v, pts2[a][0], pts2[a][1], pts2[b][0], pts2[b][1], grosor, c, alfa,
        );
    }
    // Vértices como puntos (solo si son pocos: evita tapar el 600-cell).
    if n <= 32 {
        for i in 0..n {
            if ok[i] {
                if let Some((x, y)) = mundo_a_px(v, pts2[i][0], pts2[i][1]) {
                    disco(buf, w, h, x, y, 2, BLANCO);
                }
            }
        }
    }
}

// ── Escenas (setup + animación + etiqueta) ────────────────────────────────

/// Tesseract rotando en XW (vuelta completa `0..2π`): el cubo interior pasa
/// a través del exterior —en 4D nunca se tocan—.
fn dibuja_tesseract(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
) {
    // 1. Setup: fondo + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: giro XW puro (aparece con rampa para no saltar).
    let ang = t * std::f64::consts::TAU;
    let m = Mat4::rot_xw(ang);
    let mut rotados: [Vec4; 16] = [Vec4::nuevo(0.0, 0.0, 0.0, 0.0); 16];
    for (i, vv) in verts.iter().enumerate().take(16) {
        rotados[i] = m.aplica(*vv);
    }
    let rampa = tramo(t, 0.0, 0.08);
    if rampa <= 0.0 {
        return;
    }
    dibuja_wireframe(
        buf,
        w,
        h,
        v,
        &rotados,
        aristas,
        |p| perspectiva_4d_a_3d(p, D4D_DISTANCIA_4D),
        2,
    );
}

/// 24-cell en rotación doble XW+ZW isoclínica (mismo ángulo en ambos
/// planos: cada vértice recorre un círculo de Clifford).
fn dibuja_celda24(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
) {
    // 1. Setup: fondo + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: XW y ZW al mismo ritmo (isoclínica).
    let ang = t * std::f64::consts::TAU;
    let m = Mat4::rot_xw(ang).compone(Mat4::rot_zw(ang));
    let mut rotados: [Vec4; 24] = [Vec4::nuevo(0.0, 0.0, 0.0, 0.0); 24];
    for (i, vv) in verts.iter().enumerate().take(24) {
        rotados[i] = m.aplica(*vv);
    }
    dibuja_wireframe(
        buf,
        w,
        h,
        v,
        &rotados,
        aristas,
        |p| perspectiva_4d_a_3d(p, D4D_DISTANCIA_4D),
        1,
    );
}

/// Corte `w = c` barriendo el hipercubo (`c: −1.35 → +1.35 → −1.35`, ida y
/// vuelta para loopear): el poliedro sección en verde sobre el fantasma del
/// tesseract completo. Los extremos (`|c| > 1`) muestran el vacío honesto.
#[allow(clippy::too_many_arguments)]
fn dibuja_corte(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
    caras: &[[usize; 4]],
    esc_texto: usize,
    alto_barra: i32,
) {
    // 1. Setup: fondo + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: barrido ida y vuelta.
    let corte = -1.35 * (t * std::f64::consts::TAU).cos();
    // Fantasma del tesseract completo (tenue, sin rotar: el corte es el actor).
    dibuja_wireframe_fantasma(buf, w, h, v, verts, aristas);
    // Sección exacta por caras.
    let segs = seccion_w(verts, caras, corte);
    for (a, b) in segs.iter() {
        let (Some(pa), Some(pb)) = (
            paso_3d_a_2d(*a, D4D_EYE, D4D_CENTER, D4D_FOV_RAD),
            paso_3d_a_2d(*b, D4D_EYE, D4D_CENTER, D4D_FOV_RAD),
        ) else {
            continue;
        };
        segmento_mundo(buf, w, h, v, pa[0], pa[1], pb[0], pb[1], 3, VERDE, 1.0);
    }
    // Puntos de la sección (nodos del poliedro).
    for (a, b) in segs.iter() {
        for p in [a, b] {
            if let Some(q) = paso_3d_a_2d(*p, D4D_EYE, D4D_CENTER, D4D_FOV_RAD) {
                if let Some((x, y)) = mundo_a_px(v, q[0], q[1]) {
                    disco(buf, w, h, x, y, 3, BLANCO);
                }
            }
        }
    }
    // 3. Lectura viva del corte.
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(corte, &mut num);
    let y = alto_barra + 6;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "W=");
    texto_bytes(buf, w, h, x, y, esc_texto, VERDE, &num[..ln]);
}

/// Fantasma tenue del wireframe (contexto del corte): alfa bajo, sin puntos.
fn dibuja_wireframe_fantasma(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
) {
    dibuja_wireframe(
        buf,
        w,
        h,
        v,
        verts,
        aristas,
        |p| perspectiva_4d_a_3d(p, D4D_DISTANCIA_4D),
        0,
    );
    // Atenúa: el `dibuja_wireframe` ya pinta tenue con grosor 0; el gris
    // fantasma sale del alfa por profundidad (documentado, sin segundo pase).
}

/// 600-cell sobre la 3-esfera unidad con proyección estereográfica desde el
/// polo: rotación XW (vuelta) + YW (media). Aristas = cuerdas rectas de los
/// arcos reales (ver nota del módulo). Vértices cerca del polo se clippean
/// honesto (sus aristas se saltan ese frame).
fn dibuja_estereografica(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
) {
    // 1. Setup: fondo + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: XW completa + YW media (el polo barre la figura).
    let ang = t * std::f64::consts::TAU;
    let m = Mat4::rot_xw(ang).compone(Mat4::rot_yw(ang / 2.0));
    // A la esfera unidad (la estereográfica la exige).
    let mut rotados: [Vec4; 120] = [Vec4::nuevo(0.0, 0.0, 0.0, 0.0); 120];
    for (i, vv) in verts.iter().enumerate().take(120) {
        let r = m.aplica(*vv);
        let n = r.norma();
        rotados[i] = if n > 1e-9 { r.escala(1.0 / n) } else { r };
    }
    dibuja_wireframe(buf, w, h, v, &rotados, aristas, estereografica_a_3d, 1);
}

/// Simplex ND rotando en XW (+ ZW media): vértices siempre visibles como
/// puntos ámbar (son ≤ 9) y aristas magenta. Dimensiones > 4 descartan
/// coordenadas extra (ortográfico honesto).
#[allow(clippy::too_many_arguments)]
fn dibuja_simplex(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    n: usize,
    verts: &[Vec4],
    aristas: &[[usize; 2]],
    esc_texto: usize,
    alto_barra: i32,
) {
    // 1. Setup: fondo + ejes.
    fondo(buf, w, h, v);
    // 2. Animación: XW completa + ZW media.
    let ang = t * std::f64::consts::TAU;
    let m = Mat4::rot_xw(ang).compone(Mat4::rot_zw(ang / 2.0));
    const MAXS: usize = D4D_SIMPLEX_N_MAX + 1;
    let mut rotados: [Vec4; MAXS] = [Vec4::nuevo(0.0, 0.0, 0.0, 0.0); MAXS];
    for (i, vv) in verts.iter().enumerate().take(MAXS) {
        rotados[i] = m.aplica(*vv).escala(2.2);
    }
    let nverts = verts.len().min(MAXS);
    dibuja_wireframe_magenta(buf, w, h, v, &rotados[..nverts], aristas);
    // 3. Rótulo vivo N=.
    let y = alto_barra + 6;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "N=");
    let dig = [b'0' + (n.min(9) as u8)];
    texto_bytes(buf, w, h, x, y, esc_texto, AMBAR, &dig);
}

/// Wireframe magenta con nodos ámbar (simplex: pocos vértices, se ven todos).
fn dibuja_wireframe_magenta(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    rotados: &[Vec4],
    aristas: &[[usize; 2]],
) {
    const MAXS: usize = D4D_SIMPLEX_N_MAX + 1;
    let mut pts2: [[f64; 2]; MAXS] = [[0.0; 2]; MAXS];
    let mut ok = [false; MAXS];
    let n = rotados.len().min(MAXS);
    for (i, rv) in rotados.iter().enumerate().take(n) {
        if let Some(p3) = perspectiva_4d_a_3d(*rv, D4D_DISTANCIA_4D) {
            if let Some(p2) = paso_3d_a_2d(p3, D4D_EYE, D4D_CENTER, D4D_FOV_RAD) {
                pts2[i] = p2;
                ok[i] = true;
            }
        }
    }
    for arista in aristas.iter() {
        let (a, b) = (arista[0], arista[1]);
        if a >= n || b >= n || !ok[a] || !ok[b] {
            continue;
        }
        segmento_mundo(
            buf, w, h, v, pts2[a][0], pts2[a][1], pts2[b][0], pts2[b][1], 2, MAGENTA, 0.95,
        );
    }
    for i in 0..n {
        if ok[i] {
            if let Some((x, y)) = mundo_a_px(v, pts2[i][0], pts2[i][1]) {
                disco(buf, w, h, x, y, 3, AMBAR);
            }
        }
    }
}

/// Un frame de la escena en `t` global (politopos ya generados en el setup).
#[allow(clippy::too_many_arguments)]
fn dibuja_frame(
    esc: Escena,
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    tel: &[Vec4],
    ari: &[[usize; 2]],
    caras: &[[usize; 4]],
    simplex_n: usize,
    sv: &[Vec4],
    sa: &[[usize; 2]],
    etiqueta: &str,
    esc_texto: usize,
) {
    // 3. Etiqueta (siempre, arriba): se pinta última para no taparla.
    // El alto se necesita antes en `Corte`/`Simplex`: se calcula igual que
    // en `rotulo` (7·esc+8+1) sin pintar dos veces.
    let alto_barra = 7 * esc_texto.clamp(1, 4) as i32 + 8 + 1;
    match esc {
        Escena::TesseractXw => dibuja_tesseract(buf, w, h, v, t, tel, ari),
        Escena::Celda24 => dibuja_celda24(buf, w, h, v, t, tel, ari),
        Escena::Corte => dibuja_corte(buf, w, h, v, t, tel, ari, caras, esc_texto, alto_barra),
        Escena::Estereografica => dibuja_estereografica(buf, w, h, v, t, tel, ari),
        Escena::Simplex => {
            dibuja_simplex(buf, w, h, v, t, simplex_n, sv, sa, esc_texto, alto_barra)
        }
    }
    rotulo(buf, w, h, esc_texto, etiqueta);
}

// ── Setup compartido ──────────────────────────────────────────────────────

/// Politopos del setup según la escena (se generan una vez, no por frame).
struct Setup {
    tel: Vec<Vec4>,
    ari: Vec<[usize; 2]>,
    caras: Vec<[usize; 4]>,
    simplex_n: usize,
    sv: Vec<Vec4>,
    sa: Vec<[usize; 2]>,
}

/// Genera lo que cada escena necesita (`Err` honesto si algo excede cotas).
fn setup_de(esc: Escena, simplex_n: usize) -> Result<Setup, Error4D> {
    let n = simplex_n.clamp(D4D_SIMPLEX_N_MIN, D4D_SIMPLEX_N_MAX);
    let (tel, ari) = match esc {
        Escena::TesseractXw | Escena::Corte => genera_politopo(Politopo::Tesseract)?,
        Escena::Celda24 => genera_politopo(Politopo::Cell24)?,
        Escena::Estereografica => genera_politopo(Politopo::Cell600)?,
        Escena::Simplex => genera_politopo(Politopo::Simplex(n))?,
    };
    let caras = if matches!(esc, Escena::Corte) {
        caras_hipercubo()
    } else {
        Vec::new()
    };
    let (sv, sa) = if matches!(esc, Escena::Simplex) {
        (tel.clone(), ari.clone())
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(Setup {
        tel,
        ari,
        caras,
        simplex_n: n,
        sv,
        sa,
    })
}

// ── Renderers públicos ────────────────────────────────────────────────────

/// Núcleo: rango `[desde, desde+cantidad)` de un set de `total` frames
/// (el `t` global sale del índice global: el corte es empalme exacto).
/// Valida lienzo, rango y que el chunk entre en 64 MiB.
pub fn render_4d_rango(
    template: &str,
    width: u32,
    height: u32,
    total: usize,
    desde: usize,
    cantidad: usize,
) -> Result<Vec<RgbaFrame>, Error4D> {
    render_4d_rango_simplex(
        template,
        width,
        height,
        total,
        desde,
        cantidad,
        D4D_SIMPLEX_N_DEFAULT,
    )
}

/// Idem [`render_4d_rango`] con dimensión del simplex explícita (solo la
/// usa `simplex-nd`; el resto la ignora. Fuera de rango → clamp honesto).
pub fn render_4d_rango_simplex(
    template: &str,
    width: u32,
    height: u32,
    total: usize,
    desde: usize,
    cantidad: usize,
    simplex_n: usize,
) -> Result<Vec<RgbaFrame>, Error4D> {
    let esc = escena_de(template)?;
    let etiqueta = etiqueta_de(template).unwrap_or("4D");
    valida_lienzo(width, height)?;
    if !(D4D_MIN_FRAMES..=D4D_LONGFORM_MAX_FRAMES).contains(&total) {
        return Err(Error4D::FramesFueraDeRango { got: total });
    }
    if cantidad == 0 || desde.checked_add(cantidad).is_none_or(|fin| fin > total) {
        return Err(Error4D::FramesFueraDeRango { got: cantidad });
    }
    match estima_bytes(width, height, cantidad) {
        Some(n) if n <= D4D_CHUNK_MAX_BYTES => {}
        Some(n) => return Err(Error4D::SetDemasiadoGrande { bytes: n }),
        None => return Err(Error4D::SetDemasiadoGrande { bytes: usize::MAX }),
    }
    let vista = vista_de(width, height);
    let esc_texto = escala_texto(width, height);
    // Setup una vez (politopos + caras), no por frame.
    let setup = setup_de(esc, simplex_n)?;
    let mut frames = Vec::new();
    if frames.try_reserve_exact(cantidad).is_err() {
        return Err(Error4D::SetDemasiadoGrande { bytes: usize::MAX });
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
            &setup.tel,
            &setup.ari,
            &setup.caras,
            setup.simplex_n,
            &setup.sv,
            &setup.sa,
            etiqueta,
            esc_texto,
        );
        frames.push(frame);
    }
    Ok(frames)
}

/// Set completo de `frames` para la plantilla (valida el presupuesto de
/// 64 MiB: lo que no entra se pide por rangos con [`render_4d_rango`]).
pub fn render_4d_frames(
    template: &str,
    width: u32,
    height: u32,
    frames: usize,
) -> Result<Vec<RgbaFrame>, Error4D> {
    valida_pedido(width, height, frames)?;
    render_4d_rango(template, width, height, frames, 0, frames)
}

/// Tesseract rotando en XW (48 frames default).
pub fn render_tesseract_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, Error4D> {
    render_4d_frames("tesseract-xw", width, height, D4D_DEFAULT_FRAMES)
}

/// 24-cell en rotación doble (48 frames default).
pub fn render_celda24_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, Error4D> {
    render_4d_frames("celda-24", width, height, D4D_DEFAULT_FRAMES)
}

/// Corte barriendo el hipercubo (48 frames default).
pub fn render_corte_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, Error4D> {
    render_4d_frames("hipercubo-corte", width, height, D4D_DEFAULT_FRAMES)
}

/// Estereográfica del 600-cell (48 frames default).
pub fn render_estereografica_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, Error4D> {
    render_4d_frames("estereografica", width, height, D4D_DEFAULT_FRAMES)
}

/// Simplex ND con `n` explícita (48 frames default; `n` fuera de 2..=8 → `Err`).
pub fn render_simplex_n_frames(
    width: u32,
    height: u32,
    n: usize,
) -> Result<Vec<RgbaFrame>, Error4D> {
    if !(D4D_SIMPLEX_N_MIN..=D4D_SIMPLEX_N_MAX).contains(&n) {
        return Err(Error4D::SimplexInvalido { n });
    }
    valida_pedido(width, height, D4D_DEFAULT_FRAMES)?;
    render_4d_rango_simplex(
        "simplex-nd",
        width,
        height,
        D4D_DEFAULT_FRAMES,
        0,
        D4D_DEFAULT_FRAMES,
        n,
    )
}

/// Simplex ND default (N=4: el 5-cell. 48 frames default).
pub fn render_simplex_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, Error4D> {
    render_simplex_n_frames(width, height, D4D_SIMPLEX_N_DEFAULT)
}

// ── Tests inline ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn set_ok(id: &str, w: u32, h: u32, n: usize) -> Vec<RgbaFrame> {
        let r = render_4d_frames(id, w, h, n);
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

    fn cuenta_no_fondo(set: &[RgbaFrame]) -> usize {
        let mut n = 0;
        for f in set {
            let mut tinta = false;
            let mut i = 0;
            while i + 4 <= f.pixels.len() {
                if f.pixels[i] != FONDO[0]
                    || f.pixels[i + 1] != FONDO[1]
                    || f.pixels[i + 2] != FONDO[2]
                {
                    tinta = true;
                    break;
                }
                i += 4;
            }
            if tinta {
                n += 1;
            }
        }
        n
    }

    #[test]
    fn registro_cinco_ids_kebab_unicos() {
        assert_eq!(TEMPLATE_IDS.len(), 5);
        let mut vistos = Vec::new();
        for id in TEMPLATE_IDS {
            assert!(!id.is_empty(), "id vacío");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{id} no es kebab-case"
            );
            assert!(
                !id.starts_with('-') && !id.ends_with('-'),
                "{id}: guiones borde"
            );
            assert!(!vistos.contains(id), "{id} duplicado");
            assert!(es_plantilla_4d(id), "{id} sin escena");
            assert!(etiqueta_de(id).is_some(), "{id} sin etiqueta");
            vistos.push(*id);
        }
        assert!(escena_de("  TESSERACT-XW ").is_ok(), "case+trim");
        assert!(!es_plantilla_4d("no-existe"), "desconocida atiende");
        assert!(etiqueta_de("no-existe").is_none(), "desconocida etiqueta");
    }

    #[test]
    fn tesseract_16v_32a() {
        let (v, a) = genera_politopo(Politopo::Tesseract).expect("tesseract");
        assert_eq!(v.len(), 16);
        assert_eq!(a.len(), 32);
        assert_eq!(Politopo::Tesseract.vertices_esperados(), Some(16));
        assert_eq!(Politopo::Tesseract.aristas_esperadas(), Some(32));
    }

    #[test]
    fn cell16_8v_24a() {
        let (v, a) = genera_politopo(Politopo::Cell16).expect("16-cell");
        assert_eq!(v.len(), 8);
        assert_eq!(a.len(), 24);
        assert_eq!(Politopo::Cell16.vertices_esperados(), Some(8));
        assert_eq!(Politopo::Cell16.aristas_esperadas(), Some(24));
    }

    #[test]
    fn cell24_24v_96a() {
        let (v, a) = genera_politopo(Politopo::Cell24).expect("24-cell");
        assert_eq!(v.len(), 24);
        assert_eq!(a.len(), 96);
        assert_eq!(Politopo::Cell24.vertices_esperados(), Some(24));
        assert_eq!(Politopo::Cell24.aristas_esperadas(), Some(96));
    }

    #[test]
    fn cell600_120v_720a() {
        let (v, a) = genera_politopo(Politopo::Cell600).expect("600-cell");
        assert_eq!(v.len(), 120);
        assert_eq!(a.len(), 720);
        assert_eq!(Politopo::Cell600.vertices_esperados(), Some(120));
        assert_eq!(Politopo::Cell600.aristas_esperadas(), Some(720));
    }

    #[test]
    fn cell120_600v_1200a() {
        let (v, a) = genera_politopo(Politopo::Cell120).expect("120-cell");
        assert_eq!(v.len(), 600);
        assert_eq!(a.len(), 1200);
        assert_eq!(Politopo::Cell120.vertices_esperados(), Some(600));
        assert_eq!(Politopo::Cell120.aristas_esperadas(), Some(1200));
        assert!(v.len() <= D4D_MAX_VERTICES, "cota vértices");
        assert!(a.len() <= D4D_MAX_ARISTAS, "cota aristas");
    }

    #[test]
    fn simplex_regular_y_completo() {
        for n in D4D_SIMPLEX_N_MIN..=D4D_SIMPLEX_N_MAX {
            let (v, a) = genera_politopo(Politopo::Simplex(n)).expect("simplex");
            assert_eq!(v.len(), n + 1, "n={n} verts");
            assert_eq!(a.len(), (n + 1) * n / 2, "n={n} aristas");
            // Regularidad exacta solo N ≤ 4 (lo guardado es el simplex tal
            // cual); N > 4 es sombra ortográfica: conteo sí, métrica no.
            if n <= 4 {
                // Regular: todas las aristas miden lo mismo (1.0 exacto por
                // construcción, tolerancia 1e-9).
                let d0 = v[0].dist2(v[1]);
                assert!((d0 - 1.0).abs() < 1e-9, "n={n} arista unidad");
                for i in 0..v.len() {
                    for j in (i + 1)..v.len() {
                        assert!((v[i].dist2(v[j]) - d0).abs() < 1e-9, "n={n} regular");
                    }
                }
                // Centroide en el origen.
                let mut c = [0.0; 4];
                for vv in v.iter() {
                    for (ck, vk) in c.iter_mut().zip(vv.c.iter()) {
                        *ck += *vk;
                    }
                }
                for ck in c.iter() {
                    assert!((ck / v.len() as f64).abs() < 1e-9, "n={n} centroide");
                }
            }
        }
        // 5-cell pineado: 5v/10a.
        assert_eq!(Politopo::Simplex(4).vertices_esperados(), Some(5));
        assert_eq!(Politopo::Simplex(4).aristas_esperadas(), Some(10));
        assert!(genera_politopo(Politopo::Simplex(1)).is_err(), "n=1 err");
        assert!(genera_politopo(Politopo::Simplex(9)).is_err(), "n=9 err");
        assert_eq!(Politopo::Simplex(9).vertices_esperados(), None, "n=9 none");
    }

    #[test]
    fn rot_xw_es_rotacion() {
        // 90° en XW: (1,0,0,0) → (0,0,0,1).
        let r = Mat4::rot_xw(std::f64::consts::FRAC_PI_2);
        let p = r.aplica(Vec4::nuevo(1.0, 0.0, 0.0, 0.0));
        assert!((p.c[0]).abs() < 1e-12, "x={}", p.c[0]);
        assert!((p.c[3] - 1.0).abs() < 1e-12, "w={}", p.c[3]);
        // Preserva la norma (ortogonal) para varios ángulos y planos.
        let v = Vec4::nuevo(0.3, -0.7, 1.1, 0.9);
        let n0 = v.norma();
        for ang in [0.0, 0.5, 1.0, 2.0, 3.0, -1.2] {
            for m in [Mat4::rot_xw(ang), Mat4::rot_yw(ang), Mat4::rot_zw(ang)] {
                assert!((m.aplica(v).norma() - n0).abs() < 1e-12, "norma ang={ang}");
            }
        }
        // Composición con identidad = identidad.
        let m = Mat4::rot_xw(0.7).compone(Mat4::IDENTIDAD);
        assert!((m.aplica(v).c[0] - Mat4::rot_xw(0.7).aplica(v).c[0]).abs() < 1e-12);
        // Ejes degenerados → identidad honesta.
        assert_eq!(Mat4::rot_plano(2, 2, 1.0), Mat4::IDENTIDAD);
        assert_eq!(Mat4::rot_plano(0, 9, 1.0), Mat4::IDENTIDAD);
        // YW mueve y↔w, ZW mueve z↔w, X queda quieta en ambas.
        let q = Mat4::rot_yw(std::f64::consts::FRAC_PI_2).aplica(Vec4::nuevo(0.0, 1.0, 0.0, 0.0));
        assert!((q.c[3] - 1.0).abs() < 1e-12, "yw");
        let q = Mat4::rot_zw(std::f64::consts::FRAC_PI_2).aplica(Vec4::nuevo(0.0, 0.0, 1.0, 0.0));
        assert!((q.c[3] - 1.0).abs() < 1e-12, "zw");
    }

    #[test]
    fn perspectiva_4d_a_3d_formula_y_guardias() {
        // w=0 → identidad en xyz.
        let p = perspectiva_4d_a_3d(Vec4::nuevo(1.0, 2.0, 3.0, 0.0), 2.6).expect("w=0");
        assert_eq!(p, [1.0, 2.0, 3.0]);
        // Escala d/(d−w): w=1.3, d=2.6 → ×2.
        let p = perspectiva_4d_a_3d(Vec4::nuevo(1.0, 0.0, 0.0, 1.3), 2.6).expect("×2");
        assert!((p[0] - 2.0).abs() < 1e-12, "x={}", p[0]);
        // En el plano de la cámara → None honesto.
        assert!(perspectiva_4d_a_3d(Vec4::nuevo(0.0, 0.0, 0.0, 2.6), 2.6).is_none());
        assert!(perspectiva_4d_a_3d(Vec4::nuevo(0.0, 0.0, 0.0, 9.0), 2.6).is_none());
        assert!(perspectiva_4d_a_3d(Vec4::nuevo(1.0, 0.0, 0.0, 0.0), -1.0).is_none());
        assert!(perspectiva_4d_a_3d(Vec4::nuevo(1.0, 0.0, 0.0, 0.0), 0.0).is_none());
    }

    #[test]
    fn estereografica_origen_y_polo() {
        let p = estereografica_a_3d(Vec4::nuevo(0.0, 0.0, 0.0, 0.0)).expect("origen");
        assert_eq!(p, [0.0, 0.0, 0.0]);
        // Punto ecuatorial (w=0): pasa igual.
        let p = estereografica_a_3d(Vec4::nuevo(0.5, 0.0, 0.0, 0.0)).expect("ecuador");
        assert!((p[0] - 0.5).abs() < 1e-12);
        // Cerca del polo (1−w < 1/3) → None honesto.
        assert!(estereografica_a_3d(Vec4::nuevo(0.0, 0.0, 0.0, 1.0)).is_none());
        assert!(estereografica_a_3d(Vec4::nuevo(0.0, 0.0, 0.0, 0.8)).is_none());
    }

    #[test]
    fn pinhole_centro_y_detras() {
        // El objetivo proyecta al centro 2D.
        let p = paso_3d_a_2d(D4D_CENTER, D4D_EYE, D4D_CENTER, D4D_FOV_RAD).expect("centro");
        assert!(p[0].abs() < 1e-12 && p[1].abs() < 1e-12, "{p:?}");
        // Detrás de la cámara → None honesto.
        assert!(paso_3d_a_2d([0.0, 0.0, 9.0], D4D_EYE, D4D_CENTER, D4D_FOV_RAD).is_none());
        assert!(paso_3d_a_2d([0.0, 0.0, 0.0], D4D_EYE, D4D_EYE, D4D_FOV_RAD).is_none());
        assert!(paso_3d_a_2d([0.0, 0.0, 0.0], D4D_EYE, D4D_CENTER, 0.0).is_none());
    }

    #[test]
    fn caras_24_y_seccion_media_no_vacia() {
        let caras = caras_hipercubo();
        assert_eq!(caras.len(), 24, "caras del tesseract");
        let (verts, _) = genera_politopo(Politopo::Tesseract).expect("tesseract");
        // Sin normalizar el corte va sobre ±1: reusar vértices crudos.
        let (crudos, _) = genera_tesseract();
        let media = seccion_w(&crudos, &caras, 0.0);
        assert!(!media.is_empty(), "corte medio vacío");
        assert!(media.len() <= 24, "segmentos {}", media.len());
        // Fuera del politopo → vacío honesto.
        assert!(seccion_w(&crudos, &caras, 5.0).is_empty(), "c=5 no vacío");
        assert!(seccion_w(&crudos, &caras, -5.0).is_empty(), "c=-5 no vacío");
        assert!(
            seccion_w(&crudos, &caras, f64::NAN).is_empty(),
            "nan no vacío"
        );
        let _ = verts;
    }

    #[test]
    fn presupuestos_paridad_protocolo() {
        assert_eq!(D4D_MAX_FRAMES, 64, "preview");
        assert_eq!(D4D_LONGFORM_MAX_FRAMES, 1500, "largo");
        assert_eq!(D4D_CHUNK_MAX_BYTES, 64 * 1024 * 1024, "chunk");
        assert_eq!(frames_para_duracion(50_000, 30), 1500, "50s@30fps");
        assert_eq!(frames_para_duracion(2000, 0), 0, "fps 0");
        assert_eq!(frames_por_chunk(1280, 720), 18, "chunk 720p");
        assert_eq!(frames_por_chunk(640, 480), 54, "chunk 480p");
        assert!(valida_pedido(480, 360, 48).is_ok(), "pedido ok");
        assert!(valida_pedido(32, 360, 48).is_err(), "lienzo chico");
        assert!(valida_pedido(480, 360, 0).is_err(), "0 frames");
        assert!(valida_pedido(480, 360, 1501).is_err(), "1501 frames");
        assert!(valida_pedido(4096, 4096, 1500).is_err(), "set gigante");
        assert!(
            estima_bytes(u32::MAX, u32::MAX, usize::MAX).is_none(),
            "desborde none"
        );
    }

    #[test]
    fn smoke_todas_las_escenas_pintan() {
        for id in TEMPLATE_IDS {
            let set = set_ok(id, 64, 64, 3);
            assert_eq!(cuenta_no_fondo(&set), 3, "{id} sin tinta");
        }
        // Simplex con n explícita también pinta.
        let r = render_simplex_n_frames(64, 64, 6);
        assert!(r.is_ok(), "simplex n=6: {:?}", r.err());
        assert!(render_simplex_n_frames(64, 64, 1).is_err(), "n=1 ok");
        assert!(render_simplex_n_frames(64, 64, 9).is_err(), "n=9 ok");
    }

    #[test]
    fn errores_honestos() {
        assert!(matches!(
            render_4d_frames("no-existe", 64, 64, 4),
            Err(Error4D::PlantillaDesconocida { .. })
        ));
        assert!(matches!(
            render_4d_frames("tesseract-xw", 32, 32, 4),
            Err(Error4D::LienzoInvalido { .. })
        ));
        assert!(matches!(
            render_4d_frames("tesseract-xw", 64, 64, 0),
            Err(Error4D::FramesFueraDeRango { .. })
        ));
        assert!(matches!(
            render_4d_rango("tesseract-xw", 64, 64, 8, 6, 4),
            Err(Error4D::FramesFueraDeRango { .. })
        ));
        assert!(matches!(
            render_4d_rango("tesseract-xw", 64, 64, 8, 0, 0),
            Err(Error4D::FramesFueraDeRango { .. })
        ));
    }

    #[test]
    fn rango_empalma_exacto_y_determinista() {
        let total = 6;
        let entero = set_ok("celda-24", 64, 64, total);
        let a = render_4d_rango("celda-24", 64, 64, total, 0, 2).expect("rango a");
        let b = render_4d_rango("celda-24", 64, 64, total, 2, 4).expect("rango b");
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 4);
        for (i, f) in a.iter().chain(b.iter()).enumerate() {
            assert_eq!(f.pixels, entero[i].pixels, "frame {i}");
        }
        // Determinista: dos renders iguales byte a byte.
        let otro = set_ok("tesseract-xw", 64, 64, 3);
        let rep = render_4d_frames("tesseract-xw", 64, 64, 3).expect("rep");
        for (i, f) in otro.iter().enumerate() {
            assert_eq!(f.pixels, rep[i].pixels, "det {i}");
        }
    }

    #[test]
    fn helpers_puros_y_frame() {
        assert_eq!(suave(0.0), 0.0);
        assert_eq!(suave(1.0), 1.0);
        assert!((suave(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(tramo(0.5, 1.0, 1.0), 0.0, "escalón antes");
        assert_eq!(tramo(1.0, 1.0, 1.0), 1.0, "escalón en");
        assert_eq!(progreso_en(0, 1), 0.0);
        assert_eq!(progreso_en(9, 4), 1.0, "clamp");
        let f = RgbaFrame::nuevo(64, 64).expect("frame");
        assert_eq!(f.pixels.len(), 64 * 64 * 4);
        assert!(f.pixel_en(64, 0).is_none(), "fuera");
        assert!(f.pixel_en(0, 0).is_some(), "dentro");
        assert!(RgbaFrame::nuevo(0, 64).is_err(), "w=0");
        let mut n = [0u8; 10];
        assert_eq!(formatea_1_decimal(f64::NAN, &mut n), 1);
        assert!(escala_texto(480, 360) == 2 && escala_texto(64, 64) == 1);
    }
}
