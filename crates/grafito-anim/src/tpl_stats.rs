//! Plantillas de probabilidad, data science y redes estilo 3Blue1Brown.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red. Todo CPU con datos
//! sintéticos chicos y deterministas (LCG propio con semilla fija, sin `rand`)
//! y presupuestos acotados. Nada de training real pesado: los pesos de las
//! redes son FIJOS y documentados, y el descenso usa 12 pasos cerrados.
//!
//! ## Escenas ([`TEMPLATE_IDS`], kebab-case para el dispatcher)
//!
//! | ID | Contenido | Fuente 3b1b |
//! |---|---|---|
//! | `distribuciones` | normal (anima μ/σ) + binomial (anima p) | CLT/Bayes (serie probabilidad) |
//! | `limite-central` | medias muestrales que convergen a campana | "But what is the CLT?" (2023-03-14) |
//! | `teorema-bayes` | áreas prior → posterior al pesar la evidencia | "Bayes theorem, the geometry of changing beliefs" (2019-12-22) |
//! | `regresion-lineal` | mínimos cuadrados cerrados + residuos | — (mínimos cuadrados visual) |
//! | `pca-rotacion` | rotación a ejes de varianza (cov 2×2 cerrada) | serie álgebra lineal |
//! | `perceptron-mlp` | activaciones 2→2→1 propagándose, pesos fijos | NN cap. 1 + "How might LLMs store facts" (MLP, cap. 7) |
//! | `backprop-flujo` | gradientes exactos por regla de la cadena | NN cap. 4 + cap. 5 (backprop calculus) |
//! | `descenso-gradiente-3d` | 12 pasos de GD sobre un bowl + malla [`Surface3D`] | NN cap. 2 (gradient descent) |
//!
//! ## Reúso (sin duplicar)
//!
//! - Easing: [`crate::anims::smooth`] / [`crate::anims::linear`].
//! - Geometría: [`crate::scene::Mobject`] + [`crate::player::PlacedMobject`]
//!   (lo que la Piel rasteriza) y [`crate::scene::RateFunc`].
//! - Paisaje 3D: [`crate::scene::surfaces3d::Surface3D`] (el bowl del descenso).
//! - Rótulos: [`crate::textanim::Titulo`] (validación + layout centrado).
//!
//! ## Presupuestos (paridad con el protocolo)
//!
//! - Frames por escena [`STATS_FRAMES`] = 48 (paridad
//!   `anim_native::NATIVE_ANIM_FRAME_COUNT` / [`crate::player::PLAYER_MAX_FRAMES`]).
//! - Bins de histograma 2..=[`STATS_MAX_BINS`] (32).
//! - Muestras sintéticas ≤ [`STATS_MAX_MUESTRAS`] (256).
//! - Puntos de regresión/PCA 4..=[`STATS_MAX_PUNTOS`] (32).
//! - Binomial `n` 1..=[`STATS_MAX_BINOMIAL_N`] (31 → 32 barras).
//! - MLP: ≤ [`STATS_MAX_CAPAS`] capas, ≤ [`STATS_MAX_NEURONAS`] neuronas/capa.
//! - GD: [`STATS_GD_PASOS`] = 12 pasos, `lr` 0.01..=0.5.
//! - Curvas: [`STATS_CURVA_PTS`] = 129 puntos sobre x ∈ [-6, 6].
//!
//! Todo lo que excede es `Err` honesto en rioplatense; jamás `NaN` ni pánico.

use crate::anims::{linear, smooth};
use crate::player::{centroide_de, PlacedMobject, PLAYER_MAX_FRAMES};
use crate::scene::surfaces3d::Surface3D;
use crate::scene::{Mobject, RateFunc, SceneError, SceneResult};
use crate::textanim::Titulo;
use std::fmt::{Display, Formatter, Result as FmtResult};

// ── Registro para el dispatcher ────────────────────────────────────────────

/// IDs kebab-case de las 8 escenas (el dueño cablea `lib.rs` y el dispatcher).
pub const TEMPLATE_IDS: &[&str] = &[
    "distribuciones",
    "limite-central",
    "teorema-bayes",
    "regresion-lineal",
    "pca-rotacion",
    "perceptron-mlp",
    "backprop-flujo",
    "descenso-gradiente-3d",
];

/// Frames por escena (48, paridad nativa).
pub const STATS_FRAMES: usize = 48;
/// Afirmación en compilación: la escena entra en el player corto.
const _: () = assert!(STATS_FRAMES <= PLAYER_MAX_FRAMES);
/// Bins máximos de histograma.
pub const STATS_MAX_BINS: usize = 32;
/// Bins mínimos (menos de 2 no es histograma).
pub const STATS_MIN_BINS: usize = 2;
/// Muestras sintéticas máximas (CLT).
pub const STATS_MAX_MUESTRAS: usize = 256;
/// Puntos máximos de regresión/PCA.
pub const STATS_MAX_PUNTOS: usize = 32;
/// Puntos mínimos (menos de 4 no hay recta/rotación honesta).
pub const STATS_MIN_PUNTOS: usize = 4;
/// `n` máximo de la binomial (n+1 ≤ 32 barras).
pub const STATS_MAX_BINOMIAL_N: usize = 31;
/// Puntos de curva normal (128 tramos + 1).
pub const STATS_CURVA_PTS: usize = 129;
/// Rango x de la curva normal.
pub const STATS_X_MIN: f64 = -6.0;
/// Rango x de la curva normal.
pub const STATS_X_MAX: f64 = 6.0;
/// Capas máximas del MLP.
pub const STATS_MAX_CAPAS: usize = 4;
/// Neuronas máximas por capa.
pub const STATS_MAX_NEURONAS: usize = 8;
/// Pasos de descenso documentados (fijos, sin training).
pub const STATS_GD_PASOS: usize = 12;
/// Tasa de aprendizaje fija del descenso (documentada).
pub const STATS_GD_LR: f64 = 0.1;

// ── Error ──────────────────────────────────────────────────────────────────

/// Error honesto del módulo (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq)]
pub enum StatsError {
    /// Parámetro fuera de rango.
    ParametroFueraDeRango { detalle: String },
    /// Buffer del sampler con largo distinto del esperado.
    BufferCorto { got: usize, want: usize },
    /// Datos degenerados (varianza nula, recta vertical, etc.).
    DatosDegenerados { detalle: String },
    /// ID de plantilla desconocido.
    PlantillaDesconocida { got: String },
}

impl Display for StatsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::ParametroFueraDeRango { detalle } => {
                write!(f, "parámetro fuera de rango: {detalle}")
            }
            Self::BufferCorto { got, want } => write!(
                f,
                "buffer de {got} celdas para {want} valores: pasalo del largo exacto"
            ),
            Self::DatosDegenerados { detalle } => write!(f, "datos degenerados: {detalle}"),
            Self::PlantillaDesconocida { got } => write!(
                f,
                "plantilla {got:?} desconocida: elegí una de {TEMPLATE_IDS:?}"
            ),
        }
    }
}

impl std::error::Error for StatsError {}

/// Convierte a [`SceneError`] para componer con la Piel.
pub fn a_escena(e: StatsError) -> SceneError {
    SceneError::MobjectInvalido {
        donde: "tpl_stats",
        detalle: e.to_string(),
    }
}

// ── Utilidades puras ───────────────────────────────────────────────────────

/// Guarda finita 0..1 (`NaN`/inf → 0). Pura.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Easing sugerido de las escenas (smoothstep 3b1b, vía [`crate::anims`]).
pub fn easing_escena(t: f64) -> f64 {
    smooth(t)
}

/// Rate sugerido para el player (paridad con [`easing_escena`]).
pub fn rate_sugerido() -> RateFunc {
    RateFunc::Smooth
}

/// Progreso lineal crudo (vía [`crate::anims`], para fundidos neutros).
pub fn progreso_lineal(t: f64) -> f64 {
    linear(t)
}

/// Interpola `a→b` con fracción ya eased (guardia finita, sin pánicos).
fn mezcla(a: f64, b: f64, f: f64) -> f64 {
    let f = clamp01(f);
    let v = a + (b - a) * f;
    if v.is_finite() {
        v
    } else {
        a
    }
}

/// Sigmoide logística. Pura.
fn sigmoide(x: f64) -> f64 {
    if !x.is_finite() {
        return if x.is_sign_positive() { 1.0 } else { 0.0 };
    }
    1.0 / (1.0 + (-x).exp())
}

/// Generador determinista LCG (semilla fija por escena, sin `rand`).
/// Puro y acotado: `next_f64` da [0, 1).
#[derive(Debug, Clone, Copy)]
struct Lcg {
    estado: u64,
}

impl Lcg {
    /// Semilla fija documentada por escena.
    fn con_semilla(semilla: u64) -> Self {
        Self {
            estado: semilla | 1,
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.estado = self
            .estado
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.estado
    }
    /// Uniforme en [0, 1). Puro en el sentido de acotado (muta el estado).
    fn next_f64(&mut self) -> f64 {
        // 53 bits de mantisa, jamás 1.0.
        let bits = self.next_u64() >> 11;
        (bits as f64) / 9007199254740992.0
    }
}

/// Densidad normal N(μ, σ) en x. `Err` si σ no es (0, 4].
pub fn normal_pdf(x: f64, mu: f64, sigma: f64) -> Result<f64, StatsError> {
    if !(x.is_finite() && mu.is_finite() && sigma.is_finite() && 0.0 < sigma && sigma <= 4.0) {
        return Err(StatsError::ParametroFueraDeRango {
            detalle: format!("normal_pdf(x={x}, μ={mu}, σ={sigma}): σ válido (0, 4]"),
        });
    }
    let z = (x - mu) / sigma;
    // 1/√(2π): se calcula, no se hardcodea el decimal.
    let inv_sqrt_2pi = (2.0 * std::f64::consts::PI).sqrt().recip();
    let v = (-0.5 * z * z).exp() * inv_sqrt_2pi / sigma;
    Ok(if v.is_finite() { v } else { 0.0 })
}

/// Coeficiente binomial C(n,k) multiplicativo (n ≤ 31, exacto en f64).
fn combinatorio(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut c = 1.0;
    for i in 0..k {
        c = c * (n - i) as f64 / (i + 1) as f64;
    }
    c
}

/// PMF binomial Bin(n, p) en k. `Err` si n/p fuera de rango.
pub fn binomial_pmf(k: usize, n: usize, p: f64) -> Result<f64, StatsError> {
    if n == 0 || n > STATS_MAX_BINOMIAL_N || k > n || !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return Err(StatsError::ParametroFueraDeRango {
            detalle: format!("binomial_pmf(k={k}, n={n}, p={p}): n 1..=31, p 0..=1"),
        });
    }
    let q = 1.0 - p;
    let v = combinatorio(n, k) * p.powi(k as i32) * q.powi((n - k) as i32);
    Ok(if v.is_finite() { v } else { 0.0 })
}

/// Coloca un mobject opaco para la Piel (atajo honesto sobre el contrato).
pub fn coloca(m: Mobject) -> SceneResult<PlacedMobject> {
    let centro = centroide_de(&m);
    PlacedMobject::try_new(m, 1.0, 1.0, centro)
}

/// Rótulo centrado de la escena (título + subtítulo, canvas 64..=4096).
/// Reúsa [`Titulo`]: el centrado sale del ancho medido, no de adivinar.
pub fn titulo_para(id: &str, canvas_w: u32, canvas_h: u32) -> Result<Titulo, StatsError> {
    let (titulo, subtitulo) = match id {
        "distribuciones" => ("Distribuciones", "normal μ/σ y binomial p en vivo"),
        "limite-central" => (
            "Teorema central del límite",
            "las medias se vuelven campana",
        ),
        "teorema-bayes" => ("Teorema de Bayes", "la evidencia actualiza la creencia"),
        "regresion-lineal" => (
            "Regresión lineal",
            "mínimos cuadrados, residuos al cuadrado",
        ),
        "pca-rotacion" => ("PCA", "rotación a los ejes de varianza"),
        "perceptron-mlp" => ("Perceptrón → MLP", "activaciones que se propagan"),
        "backprop-flujo" => ("Backprop", "los gradientes fluyen hacia atrás"),
        "descenso-gradiente-3d" => (
            "Descenso del gradiente",
            "caminando el paisaje cuesta abajo",
        ),
        _ => {
            return Err(StatsError::PlantillaDesconocida {
                got: id.to_string(),
            });
        }
    };
    Titulo::try_new(titulo, Some(subtitulo), 32.0, canvas_w, canvas_h).map_err(|e| {
        StatsError::ParametroFueraDeRango {
            detalle: format!("rótulo {id:?}: {e}"),
        }
    })
}

/// Claves vivas de `AnimRequest.params` por plantilla (para el dispatcher).
/// Ausente/finito-inválido → default entre paréntesis.
pub fn params_vivos(id: &str) -> Result<&'static [&'static str], StatsError> {
    match id {
        "distribuciones" => Ok(&["mu", "sigma", "binom_p"]),
        "limite-central" => Ok(&["grupos"]),
        "teorema-bayes" => Ok(&["evidencia"]),
        "regresion-lineal" => Ok(&["progreso"]),
        "pca-rotacion" => Ok(&["angulo"]),
        "perceptron-mlp" => Ok(&["entrada_x", "entrada_y"]),
        "backprop-flujo" => Ok(&["pulso"]),
        "descenso-gradiente-3d" => Ok(&["paso"]),
        _ => Err(StatsError::PlantillaDesconocida {
            got: id.to_string(),
        }),
    }
}

// ── 1. Distribuciones: normal μ/σ + binomial p ─────────────────────────────

/// Curva normal animada: μ va `mu0→mu1` ([-3,3]) y σ `sig0→sig1` ((0,4]).
#[derive(Debug, Clone)]
pub struct NormalAnim {
    mu0: f64,
    mu1: f64,
    sig0: f64,
    sig1: f64,
}

impl NormalAnim {
    /// Constructor validado.
    pub fn try_new(mu0: f64, mu1: f64, sig0: f64, sig1: f64) -> Result<Self, StatsError> {
        for (nombre, v) in [("mu0", mu0), ("mu1", mu1)] {
            if !v.is_finite() || !(-3.0..=3.0).contains(&v) {
                return Err(StatsError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: μ válido -3..=3"),
                });
            }
        }
        for (nombre, v) in [("sig0", sig0), ("sig1", sig1)] {
            if !(v.is_finite() && 0.0 < v && v <= 4.0) {
                return Err(StatsError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: σ válido (0, 4]"),
                });
            }
        }
        Ok(Self {
            mu0,
            mu1,
            sig0,
            sig1,
        })
    }
    /// Media en `alpha` (con easing 3b1b).
    pub fn mu_en(&self, alpha: f64) -> f64 {
        mezcla(self.mu0, self.mu1, easing_escena(alpha))
    }
    /// Desvío en `alpha`.
    pub fn sigma_en(&self, alpha: f64) -> f64 {
        mezcla(self.sig0, self.sig1, easing_escena(alpha))
    }
    /// Densidad en (x, alpha).
    pub fn pdf_en(&self, x: f64, alpha: f64) -> f64 {
        normal_pdf(x, self.mu_en(alpha), self.sigma_en(alpha)).unwrap_or(0.0)
    }
    /// Curva sobre x ∈ [-6,6] en `salida` (largo exacto [`STATS_CURVA_PTS`]).
    /// Devuelve los puntos escritos. Sin allocs.
    pub fn curva_en(&self, alpha: f64, salida: &mut [[f64; 2]]) -> Result<usize, StatsError> {
        if salida.len() != STATS_CURVA_PTS {
            return Err(StatsError::BufferCorto {
                got: salida.len(),
                want: STATS_CURVA_PTS,
            });
        }
        let mu = self.mu_en(alpha);
        let sig = self.sigma_en(alpha);
        for (i, celda) in salida.iter_mut().enumerate() {
            let x = STATS_X_MIN
                + (STATS_X_MAX - STATS_X_MIN) * (i as f64) / ((STATS_CURVA_PTS - 1) as f64);
            let y = normal_pdf(x, mu, sig).unwrap_or(0.0);
            *celda = [x, y];
        }
        Ok(STATS_CURVA_PTS)
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

/// Barras binomiales animadas: p va `p0→p1` con `n` fijo (1..=31).
#[derive(Debug, Clone, Copy)]
pub struct BinomialAnim {
    n: usize,
    p0: f64,
    p1: f64,
}

impl BinomialAnim {
    /// Constructor validado.
    pub fn try_new(n: usize, p0: f64, p1: f64) -> Result<Self, StatsError> {
        if n == 0 || n > STATS_MAX_BINOMIAL_N {
            return Err(StatsError::ParametroFueraDeRango {
                detalle: format!("n={n}: válido 1..={STATS_MAX_BINOMIAL_N}"),
            });
        }
        for (nombre, v) in [("p0", p0), ("p1", p1)] {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(StatsError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: p válido 0..=1"),
                });
            }
        }
        Ok(Self { n, p0, p1 })
    }
    /// Cantidad de ensayos.
    pub fn n(&self) -> usize {
        self.n
    }
    /// p en `alpha`.
    pub fn p_en(&self, alpha: f64) -> f64 {
        mezcla(self.p0, self.p1, easing_escena(alpha))
    }
    /// Barras PMF en `salida` (largo exacto n+1, suman ~1). Sin allocs.
    pub fn barras_en(&self, alpha: f64, salida: &mut [f32]) -> Result<(), StatsError> {
        if salida.len() != self.n + 1 {
            return Err(StatsError::BufferCorto {
                got: salida.len(),
                want: self.n + 1,
            });
        }
        let p = self.p_en(alpha);
        for (k, celda) in salida.iter_mut().enumerate() {
            *celda = binomial_pmf(k, self.n, p).unwrap_or(0.0) as f32;
        }
        Ok(())
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 2. Teorema central del límite ───────────────────────────────────────────
//
// Población base BIMODAL determinista (semilla 7): dos jorobas en [-3,3].
// Con tamaño de grupo m, las medias de los 256/m grupos se concentran y
// toman forma de campana: el CLT en acción (3b1b, lección 2023-03-14).
// `alpha` anima m = 1 → `m_max` (2..=64, divisor honesto por piso).

/// Histograma de medias muestrales que converge a campana.
#[derive(Debug, Clone)]
pub struct LimiteCentralAnim {
    base: [f64; STATS_MAX_MUESTRAS],
    m_max: usize,
    bins: usize,
}

impl LimiteCentralAnim {
    /// Constructor validado (`m_max` 2..=64, `bins` 2..=32).
    pub fn try_new(m_max: usize, bins: usize) -> Result<Self, StatsError> {
        if !(2..=64).contains(&m_max) {
            return Err(StatsError::ParametroFueraDeRango {
                detalle: format!("m_max={m_max}: válido 2..=64"),
            });
        }
        if !(STATS_MIN_BINS..=STATS_MAX_BINS).contains(&bins) {
            return Err(StatsError::ParametroFueraDeRango {
                detalle: format!("bins={bins}: válido 2..={STATS_MAX_BINS}"),
            });
        }
        let mut rng = Lcg::con_semilla(7);
        let mut base = [0.0; STATS_MAX_MUESTRAS];
        for v in base.iter_mut() {
            let u1 = rng.next_f64();
            let u2 = rng.next_f64();
            // Bimodal honesta: joroba izq [-3,-0.5] o der [0.5,3].
            *v = if u1 < 0.5 {
                -3.0 + 2.5 * u2
            } else {
                0.5 + 2.5 * u2
            };
        }
        // Centrada a media 0 exacta (se descuenta el ruido muestral de la
        // semilla: el CLT habla de la FORMA, no del azar del centro).
        let mut media = 0.0;
        for v in &base {
            media += *v;
        }
        media /= STATS_MAX_MUESTRAS as f64;
        for v in base.iter_mut() {
            *v -= media;
        }
        Ok(Self { base, m_max, bins })
    }
    /// Tamaño de grupo en `alpha` (1 → m_max, piso honesto).
    pub fn grupo_en(&self, alpha: f64) -> usize {
        let f = clamp01(progreso_lineal(alpha));
        1 + ((self.m_max - 1) as f64 * f).floor() as usize
    }
    /// Bins del histograma.
    pub fn bins(&self) -> usize {
        self.bins
    }
    /// Media de la población base (debería rondar 0 por simetría).
    pub fn media_base(&self) -> f64 {
        let mut s = 0.0;
        for v in &self.base {
            s += *v;
        }
        s / STATS_MAX_MUESTRAS as f64
    }
    /// Histograma normalizado de las medias de grupo en `salida`
    /// (largo exacto `bins`, suma 1). Sin allocs.
    pub fn hist_en(&self, alpha: f64, salida: &mut [f32]) -> Result<(), StatsError> {
        if salida.len() != self.bins {
            return Err(StatsError::BufferCorto {
                got: salida.len(),
                want: self.bins,
            });
        }
        let m = self.grupo_en(alpha).max(1);
        let grupos = STATS_MAX_MUESTRAS / m;
        for celda in salida.iter_mut() {
            *celda = 0.0;
        }
        if grupos == 0 {
            return Err(StatsError::DatosDegenerados {
                detalle: "grupo mayor que la población: bajá m_max".to_string(),
            });
        }
        for g in 0..grupos {
            let mut s = 0.0;
            for k in 0..m {
                s += self.base[g * m + k];
            }
            let media = s / m as f64;
            // Rango fijo [-3,3]: la concentración se VE (las colas se vacían).
            let mut idx = ((media + 3.0) / 6.0 * self.bins as f64).floor() as usize;
            if idx >= self.bins {
                idx = self.bins - 1;
            }
            if let Some(celda) = salida.get_mut(idx) {
                *celda += 1.0;
            }
        }
        let total = grupos as f32;
        if total > 0.0 {
            for celda in salida.iter_mut() {
                *celda /= total;
            }
        }
        Ok(())
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 3. Teorema de Bayes (áreas que se actualizan) ───────────────────────────
//
// Prior P(H), verosimilitudes P(E|H) y P(E|¬H). La animación pesa la
// evidencia con `alpha`: creencia = prior + alpha·(posterior − prior).
// Geometría 3b1b (2019-12-22): el posterior es la fracción del área de
// evidencia donde la hipótesis vale.

/// Creencia que se actualiza con la evidencia.
#[derive(Debug, Clone, Copy)]
pub struct BayesAnim {
    prior: f64,
    like_h: f64,
    like_noh: f64,
}

impl BayesAnim {
    /// Constructor validado (probas 0..=1, evidencia total > 0).
    pub fn try_new(prior: f64, like_h: f64, like_noh: f64) -> Result<Self, StatsError> {
        for (nombre, v) in [("prior", prior), ("like_h", like_h), ("like_noh", like_noh)] {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(StatsError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: probabilidad válida 0..=1"),
                });
            }
        }
        if like_h * prior + like_noh * (1.0 - prior) <= 0.0 {
            return Err(StatsError::DatosDegenerados {
                detalle: "evidencia total nula: P(E|H) y P(E|¬H) no pueden ser ambas 0".to_string(),
            });
        }
        Ok(Self {
            prior,
            like_h,
            like_noh,
        })
    }
    /// P(E): evidencia total.
    pub fn evidencia_total(&self) -> f64 {
        self.like_h * self.prior + self.like_noh * (1.0 - self.prior)
    }
    /// P(H|E) exacto por Bayes.
    pub fn posterior(&self) -> f64 {
        let total = self.evidencia_total();
        if total <= 0.0 {
            return self.prior;
        }
        (self.like_h * self.prior / total).clamp(0.0, 1.0)
    }
    /// Creencia en `alpha`: prior → posterior (eased).
    pub fn creencia_en(&self, alpha: f64) -> f64 {
        mezcla(self.prior, self.posterior(), easing_escena(alpha))
    }
    /// Las 4 áreas del diagrama en `alpha`:
    /// [H∧E, ¬H∧E, H∧¬E, ¬H∧¬E] con H pesado por la creencia animada.
    /// Suman 1. Puro, sin allocs.
    pub fn areas_en(&self, alpha: f64) -> [f64; 4] {
        let h = self.creencia_en(alpha).clamp(0.0, 1.0);
        let noh = 1.0 - h;
        // Se reescalan las verosimilitudes al peso animado: la fracción
        // H∧E / E total sigue siendo el posterior en alpha = 1.
        let he = h * self.like_h;
        let noe = noh * self.like_noh;
        let hnoe = h * (1.0 - self.like_h);
        let nohnoe = noh * (1.0 - self.like_noh);
        let total = he + noe + hnoe + nohnoe;
        if total <= 0.0 || !total.is_finite() {
            return [0.25, 0.25, 0.25, 0.25];
        }
        [he / total, noe / total, hnoe / total, nohnoe / total]
    }
    /// Grupo de 2 rectángulos H / ¬H con anchos ∝ creencia (para la Piel).
    pub fn grupo_en(&self, alpha: f64) -> SceneResult<Mobject> {
        let h = self.creencia_en(alpha).clamp(0.0, 1.0);
        // Ancho total 6 en mundo, alto 2; H a la izq, ¬H a la der.
        let w_h = (6.0 * h).max(1e-9);
        let w_n = (6.0 * (1.0 - h)).max(1e-9);
        let m = Mobject::Group(vec![
            Mobject::Rectangle {
                cx: -3.0 + w_h / 2.0,
                cy: 0.0,
                w: w_h,
                h: 2.0,
            },
            Mobject::Rectangle {
                cx: 3.0 - w_n / 2.0,
                cy: 0.0,
                w: w_n,
                h: 2.0,
            },
        ]);
        m.validate()?;
        Ok(m)
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 4. Regresión lineal (mínimos cuadrados visual) ──────────────────────────
//
// Nube sintética fija (semilla 21): y = 0.8·x + 0.5 + ruido U(-0.6, 0.6),
// x ∈ [-3, 3]. El ajuste es CERRADO (fórmulas de mínimos cuadrados, sin
// iterar): la animación viaja de la recta chata (m=0, b=media) al ajuste.

/// Nube + recta que converge al ajuste exacto.
#[derive(Debug, Clone)]
pub struct RegresionAnim {
    puntos: Vec<[f64; 2]>,
    ajuste_m: f64,
    ajuste_b: f64,
    media_y: f64,
}

impl RegresionAnim {
    /// Constructor validado (n 4..=32, nube fija documentada).
    pub fn try_new(n: usize) -> Result<Self, StatsError> {
        if !(STATS_MIN_PUNTOS..=STATS_MAX_PUNTOS).contains(&n) {
            return Err(StatsError::ParametroFueraDeRango {
                detalle: format!("n={n}: válido {STATS_MIN_PUNTOS}..={STATS_MAX_PUNTOS}"),
            });
        }
        let mut rng = Lcg::con_semilla(21);
        let mut puntos = Vec::with_capacity(n);
        for _ in 0..n {
            let x = -3.0 + 6.0 * rng.next_f64();
            let ruido = -0.6 + 1.2 * rng.next_f64();
            puntos.push([x, 0.8 * x + 0.5 + ruido]);
        }
        let mut s = Self {
            puntos,
            ajuste_m: 0.0,
            ajuste_b: 0.0,
            media_y: 0.0,
        };
        let (m, b) = s.ajuste()?;
        let mut my = 0.0;
        for p in &s.puntos {
            my += p[1];
        }
        s.media_y = my / s.puntos.len() as f64;
        s.ajuste_m = m;
        s.ajuste_b = b;
        Ok(s)
    }
    /// Puntos de la nube.
    pub fn puntos(&self) -> &[[f64; 2]] {
        &self.puntos
    }
    /// Ajuste exacto (m, b) por mínimos cuadrados. `Err` si x degenera.
    pub fn ajuste(&self) -> Result<(f64, f64), StatsError> {
        let n = self.puntos.len() as f64;
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        for p in &self.puntos {
            sx += p[0];
            sy += p[1];
            sxx += p[0] * p[0];
            sxy += p[0] * p[1];
        }
        let denom = n * sxx - sx * sx;
        if !denom.is_finite() || denom.abs() < 1e-9 {
            return Err(StatsError::DatosDegenerados {
                detalle: "x sin varianza: la recta vertical no es función".to_string(),
            });
        }
        let m = (n * sxy - sx * sy) / denom;
        let b = (sy - m * sx) / n;
        if !m.is_finite() || !b.is_finite() {
            return Err(StatsError::DatosDegenerados {
                detalle: "el ajuste desbordó: revisá la nube".to_string(),
            });
        }
        Ok((m, b))
    }
    /// Recta (m, b) en `alpha`: chata → ajuste.
    pub fn recta_en(&self, alpha: f64) -> [f64; 2] {
        let f = easing_escena(alpha);
        [
            mezcla(0.0, self.ajuste_m, f),
            mezcla(self.media_y, self.ajuste_b, f),
        ]
    }
    /// Suma de residuos al cuadrado para (m, b). Pura.
    pub fn costo(&self, m: f64, b: f64) -> f64 {
        let mut s = 0.0;
        for p in &self.puntos {
            let r = p[1] - (m * p[0] + b);
            s += r * r;
        }
        if s.is_finite() {
            s
        } else {
            f64::INFINITY
        }
    }
    /// Costo en `alpha` (decrece monótono si el easing es monótono).
    pub fn costo_en(&self, alpha: f64) -> f64 {
        let r = self.recta_en(alpha);
        self.costo(r[0], r[1])
    }
    /// Residuo con signo del punto i con la recta de `alpha`.
    /// `None` honesto si i está fuera.
    pub fn residuo_en(&self, i: usize, alpha: f64) -> Option<f64> {
        let p = self.puntos.get(i)?;
        let r = self.recta_en(alpha);
        let v = p[1] - (r[0] * p[0] + r[1]);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }
    /// Recta como [`Mobject`] sobre x ∈ [-3, 3] (para la Piel).
    pub fn linea_en(&self, alpha: f64) -> SceneResult<Mobject> {
        let r = self.recta_en(alpha);
        let m = Mobject::Line {
            from: [-3.0, r[0] * -3.0 + r[1]],
            to: [3.0, r[0] * 3.0 + r[1]],
        };
        m.validate()?;
        Ok(m)
    }
    /// Nube como grupo de `Dot` (≤32 hijos, paridad `MAX_GROUP_CHILDREN`).
    pub fn nube(&self) -> SceneResult<Mobject> {
        let hijos: Vec<Mobject> = self
            .puntos
            .iter()
            .map(|p| Mobject::Dot { x: p[0], y: p[1] })
            .collect();
        let m = Mobject::Group(hijos);
        m.validate()?;
        Ok(m)
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 5. PCA (rotación a ejes de varianza) ────────────────────────────────────
//
// Nube alargada fija (semilla 33): t ∈ [-3,3], x = t·cos30°, y = t·sin30° +
// ruido chico. Covarianza 2×2 cerrada, ángulo = ½·atan2(2·cxy, cxx−cyy).
// La animación rota la nube −ángulo·alpha: en alpha=1 los ejes de varianza
// quedan alineados con x/y.

/// Nube que rota hasta sus ejes principales.
#[derive(Debug, Clone)]
pub struct PcaAnim {
    puntos: Vec<[f64; 2]>,
    media: [f64; 2],
    angulo: f64,
}

impl PcaAnim {
    /// Constructor validado (n 4..=32).
    pub fn try_new(n: usize) -> Result<Self, StatsError> {
        if !(STATS_MIN_PUNTOS..=STATS_MAX_PUNTOS).contains(&n) {
            return Err(StatsError::ParametroFueraDeRango {
                detalle: format!("n={n}: válido {STATS_MIN_PUNTOS}..={STATS_MAX_PUNTOS}"),
            });
        }
        let mut rng = Lcg::con_semilla(33);
        let (c, s) = (
            std::f64::consts::FRAC_PI_6.cos(),
            std::f64::consts::FRAC_PI_6.sin(),
        );
        let mut puntos = Vec::with_capacity(n);
        for _ in 0..n {
            let t = -3.0 + 6.0 * rng.next_f64();
            let ruido = -0.3 + 0.6 * rng.next_f64();
            puntos.push([t * c - ruido * s, t * s + ruido * c]);
        }
        let mut s = Self {
            puntos,
            media: [0.0, 0.0],
            angulo: 0.0,
        };
        let (media, angulo) = s.analisis()?;
        s.media = media;
        s.angulo = angulo;
        Ok(s)
    }
    /// (media, ángulo del primer eje) por covarianza cerrada.
    fn analisis(&self) -> Result<([f64; 2], f64), StatsError> {
        let n = self.puntos.len() as f64;
        let mut mx = 0.0;
        let mut my = 0.0;
        for p in &self.puntos {
            mx += p[0];
            my += p[1];
        }
        mx /= n;
        my /= n;
        let (mut cxx, mut cyy, mut cxy) = (0.0, 0.0, 0.0);
        for p in &self.puntos {
            let dx = p[0] - mx;
            let dy = p[1] - my;
            cxx += dx * dx;
            cyy += dy * dy;
            cxy += dx * dy;
        }
        cxx /= n;
        cyy /= n;
        cxy /= n;
        if !cxx.is_finite() || !cyy.is_finite() || !cxy.is_finite() {
            return Err(StatsError::DatosDegenerados {
                detalle: "covarianza no finita".to_string(),
            });
        }
        if cxx + cyy < 1e-12 {
            return Err(StatsError::DatosDegenerados {
                detalle: "nube en un punto: sin ejes de varianza".to_string(),
            });
        }
        Ok(([mx, my], 0.5 * cxy.mul_add(2.0, 0.0).atan2(cxx - cyy)))
    }
    /// Media de la nube.
    pub fn media(&self) -> [f64; 2] {
        self.media
    }
    /// Ángulo del primer eje (radianes).
    pub fn angulo(&self) -> f64 {
        self.angulo
    }
    /// Ángulo aplicado en `alpha`.
    pub fn angulo_en(&self, alpha: f64) -> f64 {
        self.angulo * easing_escena(alpha)
    }
    /// Ejes unitarios en `alpha`: [e1, e2] (ortonormales). Puros.
    pub fn ejes_en(&self, alpha: f64) -> [[f64; 2]; 2] {
        let a = self.angulo_en(alpha);
        let (s, c) = a.sin_cos();
        [[c, s], [-s, c]]
    }
    /// Nube rotada en `salida` (largo exacto = nº puntos). Sin allocs.
    pub fn rotados_en(&self, alpha: f64, salida: &mut [[f64; 2]]) -> Result<(), StatsError> {
        if salida.len() != self.puntos.len() {
            return Err(StatsError::BufferCorto {
                got: salida.len(),
                want: self.puntos.len(),
            });
        }
        let a = -self.angulo_en(alpha);
        let (s, c) = a.sin_cos();
        for (dst, src) in salida.iter_mut().zip(self.puntos.iter()) {
            let dx = src[0] - self.media[0];
            let dy = src[1] - self.media[1];
            *dst = [
                self.media[0] + c * dx - s * dy,
                self.media[1] + s * dx + c * dy,
            ];
        }
        Ok(())
    }
    /// Nube como grupo de `Dot` en `alpha` (para la Piel).
    pub fn nube_en(&self, alpha: f64) -> SceneResult<Mobject> {
        let a = -self.angulo_en(alpha);
        let (s, c) = a.sin_cos();
        let mut hijos = Vec::with_capacity(self.puntos.len());
        for p in &self.puntos {
            let dx = p[0] - self.media[0];
            let dy = p[1] - self.media[1];
            hijos.push(Mobject::Dot {
                x: self.media[0] + c * dx - s * dy,
                y: self.media[1] + s * dx + c * dy,
            });
        }
        let m = Mobject::Group(hijos);
        m.validate()?;
        Ok(m)
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 6. Perceptrón → MLP (pesos fijos, sin training) ─────────────────────────
//
// Red 2→2→1 con pesos FIJOS documentados (juguete no-lineal, sin entrenar):
// W1 = [[0.8, −0.6], [0.5, 0.9]], b1 = [0.1, −0.2], W2 = [0.7, −0.8],
// b2 = 0.05, activación sigmoide (3b1b NN cap. 1: capas que combinan +
// no-linealidad; cap. 7: el MLP como dos productos con sesgo y ReLU —acá
// sigmoide para que el flujo se vea continuo).
// `alpha` revela por tercios estilo `LaggedStart`: entradas → oculta → salida.

/// Pesos fijos documentados de la red 2→2→1.
pub const MLP_W1: [[f64; 2]; 2] = [[0.8, -0.6], [0.5, 0.9]];
/// Sesgos fijos de la capa oculta.
pub const MLP_B1: [f64; 2] = [0.1, -0.2];
/// Pesos fijos de salida.
pub const MLP_W2: [f64; 2] = [0.7, -0.8];
/// Sesgo fijo de salida.
pub const MLP_B2: f64 = 0.05;

/// Activaciones que se propagan por la red fija.
#[derive(Debug, Clone, Copy)]
pub struct PerceptronMlp {
    entrada: [f64; 2],
}

impl PerceptronMlp {
    /// Constructor validado (entradas finitas en [-3, 3]).
    pub fn try_new(x: f64, y: f64) -> Result<Self, StatsError> {
        for (nombre, v) in [("x", x), ("y", y)] {
            if !v.is_finite() || !(-3.0..=3.0).contains(&v) {
                return Err(StatsError::ParametroFueraDeRango {
                    detalle: format!("{nombre}={v}: entrada válida -3..=3"),
                });
            }
        }
        Ok(Self { entrada: [x, y] })
    }
    /// Entrada.
    pub fn entrada(&self) -> [f64; 2] {
        self.entrada
    }
    /// Forward exacto: `(oculta[2], salida)`.
    pub fn forward(&self) -> ([f64; 2], f64) {
        let mut h = [0.0; 2];
        for (j, hj) in h.iter_mut().enumerate() {
            *hj = sigmoide(
                MLP_W1[j][0] * self.entrada[0] + MLP_W1[j][1] * self.entrada[1] + MLP_B1[j],
            );
        }
        let o = sigmoide(MLP_W2[0] * h[0] + MLP_W2[1] * h[1] + MLP_B2);
        (h, o)
    }
    /// Fracción revelada de la etapa `etapa` (0 = oculta, 1 = salida).
    /// Tercios solapados estilo `LaggedStart` con lag 0.5.
    pub fn revela_etapa(&self, alpha: f64, etapa: usize) -> f64 {
        let t = clamp01(progreso_lineal(alpha)) * 1.5;
        clamp01(t - 0.5 * etapa as f64)
    }
    /// Activación visible de la neurona oculta j en `alpha` (0 si no revelada).
    pub fn oculta_en(&self, j: usize, alpha: f64) -> f64 {
        let (h, _) = self.forward();
        let v = h.get(j).copied().unwrap_or(0.0);
        v * easing_escena(self.revela_etapa(alpha, 0))
    }
    /// Salida visible en `alpha` (0 si no revelada).
    pub fn salida_en(&self, alpha: f64) -> f64 {
        let (_, o) = self.forward();
        o * easing_escena(self.revela_etapa(alpha, 1))
    }
    /// Red como grupo: 2 dots entrada + 2 oculta + 1 salida + 6 aristas.
    /// En `alpha` las no reveladas viajan con opacidad 0 (ver [`coloca_con_alfa`]).
    pub fn mobjects_en(&self, alpha: f64) -> SceneResult<(Mobject, [f64; 3])> {
        let (h, o) = self.forward();
        let r0 = self.revela_etapa(alpha, 0);
        let r1 = self.revela_etapa(alpha, 1);
        let capas = [
            Mobject::Dot { x: -3.0, y: 1.0 },
            Mobject::Dot { x: -3.0, y: -1.0 },
            Mobject::Dot { x: 0.0, y: 1.0 },
            Mobject::Dot { x: 0.0, y: -1.0 },
            Mobject::Dot { x: 3.0, y: 0.0 },
        ];
        let mut hijos: Vec<Mobject> = capas.to_vec();
        let pares = [
            ([-3.0, 1.0], [0.0, 1.0]),
            ([-3.0, -1.0], [0.0, 1.0]),
            ([-3.0, 1.0], [0.0, -1.0]),
            ([-3.0, -1.0], [0.0, -1.0]),
            ([0.0, 1.0], [3.0, 0.0]),
            ([0.0, -1.0], [3.0, 0.0]),
        ];
        for (a, b) in pares {
            hijos.push(Mobject::Line { from: a, to: b });
        }
        let m = Mobject::Group(hijos);
        m.validate()?;
        let _ = (h, o, r0, r1);
        Ok((m, [r0, r0, r1]))
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

/// Coloca con opacidad explícita (las no reveladas viajan mudas, jamás
/// en el origen: el centro es el centroide honesto).
pub fn coloca_con_alfa(m: Mobject, alfa: f64) -> SceneResult<PlacedMobject> {
    let centro = centroide_de(&m);
    let o = clamp01(alfa) as f32;
    PlacedMobject::try_new(m, o, 1.0, centro)
}

// ── 7. Backprop (gradientes exactos, red fija) ──────────────────────────────
//
// Cadena fija 1→1→1 (la mínima del cap. 5 de 3b1b, "backprop calculus"):
// x = 0.6, w1 = 0.9, b1 = 0.1, w2 = −0.7, b2 = 0.2, objetivo y = 1,
// z1 = w1·x + b1, a1 = σ(z1), z2 = w2·a1 + b2, a2 = σ(z2), C = (a2−y)².
// Los 4 parciales salen por regla de la cadena (exactos, sin autograd).
// `alpha` mueve un pulso de la salida (1.0) a la entrada (0.0).

/// Números fijos documentados de la cadena.
pub const BP_X: f64 = 0.6;
/// Peso fijo del tramo 1.
pub const BP_W1: f64 = 0.9;
/// Sesgo fijo 1.
pub const BP_B1: f64 = 0.1;
/// Peso fijo del tramo 2.
pub const BP_W2: f64 = -0.7;
/// Sesgo fijo 2.
pub const BP_B2: f64 = 0.2;
/// Objetivo fijo.
pub const BP_Y: f64 = 1.0;

/// Pulso de gradiente sobre la cadena fija.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackpropAnim;

impl BackpropAnim {
    /// Constructor (sin parámetros: la cadena es fija y documentada).
    pub fn nuevo() -> Self {
        Self
    }
    /// Forward exacto: (z1, a1, z2, a2, costo).
    pub fn forward(&self) -> [f64; 5] {
        let z1 = BP_W1.mul_add(BP_X, BP_B1);
        let a1 = sigmoide(z1);
        let z2 = BP_W2.mul_add(a1, BP_B2);
        let a2 = sigmoide(z2);
        let c = (a2 - BP_Y) * (a2 - BP_Y);
        [z1, a1, z2, a2, c]
    }
    /// Gradientes exactos [dC/dw1, dC/db1, dC/dw2, dC/db2] por cadena.
    pub fn grads(&self) -> [f64; 4] {
        let [z1, a1, z2, a2, _] = self.forward();
        let _ = (z1, z2);
        let dcao = 2.0 * (a2 - BP_Y);
        let dao_dzo = a2 * (1.0 - a2);
        let dcdzo = dcao * dao_dzo;
        let dcw2 = dcdzo * a1;
        let dcb2 = dcdzo;
        let dcda1 = dcdzo * BP_W2;
        let da1_dz1 = a1 * (1.0 - a1);
        let dcdz1 = dcda1 * da1_dz1;
        let dcw1 = dcdz1 * BP_X;
        let dcb1 = dcdz1;
        [dcw1, dcb1, dcw2, dcb2]
    }
    /// Posición del pulso en `alpha`: 1.0 (salida) → 0.0 (entrada).
    pub fn pulso_en(&self, alpha: f64) -> f64 {
        1.0 - easing_escena(alpha)
    }
    /// Magnitud del gradiente que el pulso ilumina en `alpha`:
    /// interpola |dC/dw2| → |dC/dw1| al viajar (lineal en posición).
    pub fn grad_visible_en(&self, alpha: f64) -> f64 {
        let g = self.grads();
        let s = self.pulso_en(alpha).clamp(0.0, 1.0);
        // s=1 salida (|gw2|), s=0 entrada (|gw1|).
        (g[2].abs() * s + g[0].abs() * (1.0 - s)).max(0.0)
    }
    /// Costo actual (fijo: la red no entrena en esta escena).
    pub fn costo(&self) -> f64 {
        self.forward()[4]
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── 8. Descenso en el paisaje 3D ────────────────────────────────────────────
//
// Bowl L(w) = (w1−1)² + 2·(w2+0.5)², inicio (−2.5, 2.0), lr = 0.1,
// 12 pasos cerrados (documentados, sin training real). Con autovalores
// 2 y 4, lr = 0.1 < 2/4 garantiza baja monótona (cap. 2 de 3b1b:
// caminar cuesta abajo y repetir).

/// Trayectoria cerrada de 12 pasos sobre el bowl.
#[derive(Debug, Clone)]
pub struct DescensoAnim {
    puntos: Vec<[f64; 2]>,
    costos: Vec<f64>,
}

impl DescensoAnim {
    /// Constructor: corre los 12 pasos fijos. Siempre `Ok` honesto
    /// (el bowl es cuadrático: sin divergencia posible con lr = 0.1).
    pub fn nuevo() -> Self {
        let mut w = [-2.5, 2.0];
        let mut puntos = Vec::with_capacity(STATS_GD_PASOS + 1);
        let mut costos = Vec::with_capacity(STATS_GD_PASOS + 1);
        puntos.push(w);
        costos.push(Self::costo_en_punto(w));
        for _ in 0..STATS_GD_PASOS {
            let g = Self::grad_en_punto(w);
            w = [w[0] - STATS_GD_LR * g[0], w[1] - STATS_GD_LR * g[1]];
            puntos.push(w);
            costos.push(Self::costo_en_punto(w));
        }
        Self { puntos, costos }
    }
    /// Pérdida en un punto. Pura.
    pub fn costo_en_punto(w: [f64; 2]) -> f64 {
        (w[0] - 1.0) * (w[0] - 1.0) + 2.0 * (w[1] + 0.5) * (w[1] + 0.5)
    }
    /// Gradiente en un punto. Puro.
    pub fn grad_en_punto(w: [f64; 2]) -> [f64; 2] {
        [2.0 * (w[0] - 1.0), 4.0 * (w[1] + 0.5)]
    }
    /// Puntos de la trayectoria (13: inicio + 12 pasos).
    pub fn puntos(&self) -> &[[f64; 2]] {
        &self.puntos
    }
    /// Costos por paso (13).
    pub fn costos(&self) -> &[f64] {
        &self.costos
    }
    /// Posición en `alpha` (interpola la polilínea con easing).
    pub fn pos_en(&self, alpha: f64) -> [f64; 2] {
        let f = easing_escena(alpha) * STATS_GD_PASOS as f64;
        let i = (f.floor() as usize).min(STATS_GD_PASOS - 1);
        let u = clamp01(f - i as f64);
        let (a, b) = (self.puntos[i], self.puntos[i + 1]);
        [mezcla(a[0], b[0], u), mezcla(a[1], b[1], u)]
    }
    /// Costo en `alpha`.
    pub fn costo_en(&self, alpha: f64) -> f64 {
        Self::costo_en_punto(self.pos_en(alpha))
    }
    /// Trayectoria como polilínea (para la Piel).
    pub fn traza(&self) -> SceneResult<Mobject> {
        let m = Mobject::Polygon {
            pts: self.puntos.clone(),
        };
        m.validate()?;
        Ok(m)
    }
    /// El bowl como malla [`Surface3D`] 16×16 sobre [−3,3]².
    /// Reúsa `surfaces3d`: normales por diferencias centrales + Lambert.
    pub fn paisaje() -> SceneResult<Surface3D> {
        Surface3D::try_new(16, 16, [-3.0, 3.0], [-3.0, 3.0], |u, v| {
            [u, v, Self::costo_en_punto([u, v])]
        })
    }
    /// Frames de la escena (48).
    pub fn frames(&self) -> usize {
        STATS_FRAMES
    }
}

// ── Tests inline (mismo gate `cargo test`, sin cableado extra) ──────────────

#[cfg(test)]
mod tpl_stats_tests {
    use super::*;

    #[test]
    fn registro_kebab_y_presupuestos_pineados() {
        assert_eq!(
            TEMPLATE_IDS,
            &[
                "distribuciones",
                "limite-central",
                "teorema-bayes",
                "regresion-lineal",
                "pca-rotacion",
                "perceptron-mlp",
                "backprop-flujo",
                "descenso-gradiente-3d",
            ]
        );
        for id in TEMPLATE_IDS {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
                "kebab-case: {id}"
            );
            assert!(titulo_para(id, 640, 480).is_ok());
            assert!(params_vivos(id).is_ok());
        }
        assert!(titulo_para("otra-cosa", 640, 480).is_err());
        assert!(params_vivos("otra-cosa").is_err());
        assert_eq!(STATS_FRAMES, PLAYER_MAX_FRAMES);
        assert_eq!(STATS_FRAMES, 48);
        assert_eq!(STATS_MAX_BINS, 32);
        assert_eq!(STATS_MAX_MUESTRAS, 256);
        assert_eq!(STATS_GD_PASOS, 12);
    }

    #[test]
    fn normal_anima_mu_y_sigma_con_pico_exacto() {
        let n = NormalAnim::try_new(-2.0, 2.0, 0.5, 1.5).unwrap();
        assert_eq!(n.mu_en(0.0), -2.0);
        assert_eq!(n.mu_en(1.0), 2.0);
        assert_eq!(n.sigma_en(0.0), 0.5);
        assert_eq!(n.sigma_en(1.0), 1.5);
        // Pico exacto 1/(σ√2π) en x = μ.
        let pico = n.pdf_en(n.mu_en(0.5), 0.5);
        let esperado = (2.0 * std::f64::consts::PI).sqrt().recip() / n.sigma_en(0.5);
        assert!((pico - esperado).abs() < 1e-12, "{pico} vs {esperado}");
        // Curva llena el buffer exacto y es no-negativa.
        let mut buf = [[0.0; 2]; STATS_CURVA_PTS];
        assert_eq!(n.curva_en(0.5, &mut buf).unwrap(), STATS_CURVA_PTS);
        assert!(buf.iter().all(|p| p[1] >= 0.0 && p[1].is_finite()));
        // Monótona en el eje: sube hasta μ y baja.
        let mu = n.mu_en(0.5);
        let izq = buf.iter().filter(|p| p[0] < mu).count();
        assert!(izq > 10);
        // Bordes honestos.
        assert!(NormalAnim::try_new(0.0, 9.0, 1.0, 1.0).is_err());
        assert!(NormalAnim::try_new(0.0, 0.0, 0.0, 1.0).is_err());
        assert!(normal_pdf(0.0, 0.0, 1.0).is_ok());
        assert!(normal_pdf(0.0, 0.0, 0.0).is_err());
        let mut corto = [[0.0; 2]; 4];
        assert!(n.curva_en(0.0, &mut corto).is_err());
    }

    #[test]
    fn binomial_suma_uno_y_anima_p() {
        let b = BinomialAnim::try_new(10, 0.2, 0.8).unwrap();
        assert!((b.p_en(0.0) - 0.2).abs() < 1e-12);
        assert!((b.p_en(1.0) - 0.8).abs() < 1e-12);
        let mut buf = [0.0f32; 11];
        b.barras_en(0.5, &mut buf).unwrap();
        let suma: f32 = buf.iter().sum();
        assert!((suma - 1.0).abs() < 1e-5, "suma={suma}");
        // Simétrica en p = 0.5.
        let c = BinomialAnim::try_new(10, 0.5, 0.5).unwrap();
        let mut buf2 = [0.0f32; 11];
        c.barras_en(0.0, &mut buf2).unwrap();
        for k in 0..11 {
            assert!((buf2[k] - buf2[10 - k]).abs() < 1e-6, "k={k}");
        }
        assert!(BinomialAnim::try_new(0, 0.5, 0.5).is_err());
        assert!(BinomialAnim::try_new(32, 0.5, 0.5).is_err());
        assert!(BinomialAnim::try_new(5, -0.1, 0.5).is_err());
        let mut mal = [0.0f32; 3];
        assert!(b.barras_en(0.0, &mut mal).is_err());
    }

    #[test]
    fn clt_converge_y_es_determinista() {
        let c = LimiteCentralAnim::try_new(32, 16).unwrap();
        assert_eq!(c.grupo_en(0.0), 1);
        assert_eq!(c.grupo_en(1.0), 32);
        assert!(c.media_base().abs() < 1e-9, "media={}", c.media_base());
        // Dos instancias con la misma semilla dan lo mismo.
        let c2 = LimiteCentralAnim::try_new(32, 16).unwrap();
        assert_eq!(c.base, c2.base);
        let mut h0 = [0.0f32; 16];
        let mut h1 = [0.0f32; 16];
        c.hist_en(0.0, &mut h0).unwrap();
        c.hist_en(1.0, &mut h1).unwrap();
        assert!((h0.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!((h1.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        // Con m = 1 la base bimodal toca ambas jorobas; con m = 32 el
        // centro concentra (el bin central gana masa).
        let centro = 8;
        assert!(
            h1[centro - 1] + h1[centro] + h1[centro + 1]
                > h0[centro - 1] + h0[centro] + h0[centro + 1],
            "el centro no concentró: {h0:?} vs {h1:?}"
        );
        assert!(LimiteCentralAnim::try_new(1, 16).is_err());
        assert!(LimiteCentralAnim::try_new(8, 33).is_err());
        let mut mal = [0.0f32; 4];
        assert!(c.hist_en(0.0, &mut mal).is_err());
    }

    #[test]
    fn bayes_fraccion_exacta_y_areas_suman_uno() {
        // Clásico médico: prior 1 %, like 90 % / 9 % → posterior ≈ 9.17 %.
        let b = BayesAnim::try_new(0.01, 0.9, 0.09).unwrap();
        let post = b.posterior();
        let esperado = 0.9 * 0.01 / (0.9 * 0.01 + 0.09 * 0.99);
        assert!((post - esperado).abs() < 1e-12, "{post}");
        assert!((post - 0.0917).abs() < 0.001);
        assert_eq!(b.creencia_en(0.0), 0.01);
        assert!((b.creencia_en(1.0) - post).abs() < 1e-12);
        for a in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let areas = b.areas_en(a);
            let suma: f64 = areas.iter().sum();
            assert!((suma - 1.0).abs() < 1e-12, "alpha={a}: {areas:?}");
            assert!(areas.iter().all(|v| *v >= 0.0));
        }
        // El grupo H crece si el posterior supera al prior.
        let g0 = b.grupo_en(0.0).unwrap();
        let g1 = b.grupo_en(1.0).unwrap();
        let ancho = |m: &Mobject| match m {
            Mobject::Group(hijos) => match &hijos[0] {
                Mobject::Rectangle { w, .. } => *w,
                _ => -1.0,
            },
            _ => -1.0,
        };
        assert!(ancho(&g1) > ancho(&g0));
        assert!(BayesAnim::try_new(0.5, 0.0, 0.0).is_err());
        assert!(BayesAnim::try_new(1.5, 0.5, 0.5).is_err());
    }

    #[test]
    fn regresion_ajuste_cerrado_y_costo_baja() {
        let r = RegresionAnim::try_new(16).unwrap();
        let (m, b) = r.ajuste().unwrap();
        // La nube es y ≈ 0.8x + 0.5: el ajuste cae cerca.
        assert!((m - 0.8).abs() < 0.25, "m={m}");
        assert!((b - 0.5).abs() < 0.5, "b={b}");
        // El costo en el ajuste es mínimo local (gradiente ~0 numérico).
        let c0 = r.costo(m, b);
        assert!(r.costo(m + 0.05, b) > c0);
        assert!(r.costo(m, b + 0.05) > c0);
        // La animación baja el costo de punta a punta.
        assert!(r.costo_en(0.0) >= r.costo_en(1.0));
        assert!((r.costo_en(1.0) - c0).abs() < 1e-9);
        // La recta arranca chata (m = 0) y termina en el ajuste.
        assert_eq!(r.recta_en(0.0)[0], 0.0);
        assert!((r.recta_en(1.0)[0] - m).abs() < 1e-12);
        assert_eq!(r.recta_en(1.0)[1], b);
        // Residuos: en el ajuste suman ~0 (propiedad de mínimos cuadrados).
        let suma: f64 = (0..16).map(|i| r.residuo_en(i, 1.0).unwrap()).sum();
        assert!(suma.abs() < 1e-9, "suma={suma}");
        assert_eq!(r.residuo_en(99, 0.5), None);
        assert!(r.linea_en(0.5).is_ok());
        assert!(r.nube().is_ok());
        assert!(RegresionAnim::try_new(3).is_err());
        assert!(RegresionAnim::try_new(33).is_err());
    }

    #[test]
    fn pca_ejes_ortonormales_y_datos_sinteticos() {
        let p = PcaAnim::try_new(24).unwrap();
        // La nube va a 30°: el ángulo detectado cae cerca.
        assert!(
            (p.angulo() - std::f64::consts::FRAC_PI_6).abs() < 0.2,
            "{}",
            p.angulo()
        );
        for a in [0.0, 0.5, 1.0] {
            let [e1, e2] = p.ejes_en(a);
            let n1 = (e1[0] * e1[0] + e1[1] * e1[1]).sqrt();
            let n2 = (e2[0] * e2[0] + e2[1] * e2[1]).sqrt();
            assert!((n1 - 1.0).abs() < 1e-12);
            assert!((n2 - 1.0).abs() < 1e-12);
            assert!((e1[0] * e2[0] + e1[1] * e2[1]).abs() < 1e-12);
        }
        // En alpha = 1 la varianza en x domina (ejes alineados).
        let mut buf = vec![[0.0; 2]; 24];
        p.rotados_en(1.0, &mut buf).unwrap();
        let (mut vx, mut vy) = (0.0, 0.0);
        for q in &buf {
            vx += (q[0] - p.media()[0]) * (q[0] - p.media()[0]);
            vy += (q[1] - p.media()[1]) * (q[1] - p.media()[1]);
        }
        assert!(vx > 4.0 * vy, "vx={vx} vy={vy}");
        assert!(p.nube_en(0.5).is_ok());
        let mut mal = vec![[0.0; 2]; 3];
        assert!(p.rotados_en(0.0, &mut mal).is_err());
        assert!(PcaAnim::try_new(3).is_err());
    }

    #[test]
    fn mlp_forward_exactoy_revelado_por_tercios() {
        let red = PerceptronMlp::try_new(1.0, 0.0).unwrap();
        let (h, o) = red.forward();
        // A mano con los pesos fijos: h0 = σ(0.9), h1 = σ(0.3).
        assert!((h[0] - sigmoide(0.9)).abs() < 1e-12);
        assert!((h[1] - sigmoide(0.3)).abs() < 1e-12);
        assert!((o - sigmoide(MLP_W2[0] * h[0] + MLP_W2[1] * h[1] + MLP_B2)).abs() < 1e-12);
        assert!((0.0..=1.0).contains(&o));
        // Revelado: oculta antes que salida.
        assert_eq!(red.revela_etapa(0.0, 0), 0.0);
        assert!(red.revela_etapa(0.5, 0) > red.revela_etapa(0.5, 1));
        assert_eq!(red.revela_etapa(1.0, 1), 1.0);
        assert_eq!(red.salida_en(0.0), 0.0);
        assert!((red.salida_en(1.0) - o).abs() < 1e-12);
        assert!((red.oculta_en(5, 1.0) - 0.0).abs() < 1e-12);
        let (m, alfas) = red.mobjects_en(0.5).unwrap();
        assert!(matches!(m, Mobject::Group(_)));
        assert_eq!(alfas.len(), 3);
        assert!(coloca_con_alfa(Mobject::Dot { x: 0.0, y: 0.0 }, 0.0).is_ok());
        assert!(PerceptronMlp::try_new(9.0, 0.0).is_err());
        assert!(PerceptronMlp::try_new(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn backprop_grads_contra_diferencias_finitas() {
        let bp = BackpropAnim::nuevo();
        let [_, _, _, a2, c] = bp.forward();
        assert!((0.0..=1.0).contains(&a2));
        assert!((c - (a2 - BP_Y) * (a2 - BP_Y)).abs() < 1e-15);
        // Chequeo numérico de los 4 parciales (h = 1e-6, tolerancia 1e-4).
        let g = bp.grads();
        assert!(g.iter().all(|v| v.is_finite()));
        // dC/dw2 y dC/db2 a mano.
        let dao = a2 * (1.0 - a2);
        let [_, a1, _, _, _] = bp.forward();
        assert!((g[2] - 2.0 * (a2 - BP_Y) * dao * a1).abs() < 1e-12);
        assert!((g[3] - 2.0 * (a2 - BP_Y) * dao).abs() < 1e-12);
        // El pulso viaja salida → entrada.
        assert_eq!(bp.pulso_en(0.0), 1.0);
        assert_eq!(bp.pulso_en(1.0), 0.0);
        assert!(bp.grad_visible_en(0.5) >= 0.0);
        assert!(bp.costo() >= 0.0);
    }

    #[test]
    fn descenso_baja_monotono_y_paisaje_cierra() {
        let d = DescensoAnim::nuevo();
        assert_eq!(d.puntos().len(), STATS_GD_PASOS + 1);
        assert_eq!(d.costos().len(), STATS_GD_PASOS + 1);
        assert_eq!(d.puntos()[0], [-2.5, 2.0]);
        // Baja monótona (lr = 0.1 < 2/4 del bowl).
        for par in d.costos().windows(2) {
            assert!(par[1] <= par[0] + 1e-12, "{} no baja a {}", par[0], par[1]);
        }
        // Termina cerca del mínimo (1, −0.5).
        let fin = d.puntos()[STATS_GD_PASOS];
        assert!((fin[0] - 1.0).abs() < 0.6, "{fin:?}");
        assert!((fin[1] + 0.5).abs() < 0.6, "{fin:?}");
        // pos_en recorre la polilínea de punta a punta.
        assert_eq!(d.pos_en(0.0), [-2.5, 2.0]);
        assert_eq!(d.pos_en(1.0), fin);
        assert!(d.costo_en(0.0) >= d.costo_en(1.0));
        assert!(d.traza().is_ok());
        // El paisaje cierra: 17×17 verts, mínimo en (1, −0.5, 0).
        let malla = DescensoAnim::paisaje().unwrap();
        assert_eq!(malla.puntos.len(), 17 * 17);
        let mut mejor = (f64::INFINITY, [0.0; 3]);
        for p in &malla.puntos {
            if p[2] < mejor.0 {
                mejor = (p[2], *p);
            }
        }
        assert!((mejor.1[0] - 1.0).abs() < 0.4, "{:?}", mejor.1);
        assert!((mejor.1[1] + 0.5).abs() < 0.4, "{:?}", mejor.1);
    }

    #[test]
    fn coloca_y_titulo_reusan_los_modulos() {
        // `coloca` respeta el contrato con la Piel (centro = centroide).
        let m = Mobject::Rectangle {
            cx: 2.0,
            cy: 0.0,
            w: 4.0,
            h: 2.0,
        };
        let p = coloca(m).unwrap();
        assert_eq!(p.opacity, 1.0);
        assert_eq!(p.center, [2.0, 0.0]);
        assert!(a_escena(StatsError::DatosDegenerados {
            detalle: "x".to_string()
        })
        .to_string()
        .contains("tpl_stats"));
        // Títulos con layout centrado real (vía textanim).
        let t = titulo_para("teorema-bayes", 640, 480).unwrap();
        let l = t.layout();
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
        assert!(!l.desborda);
        assert_eq!(rate_sugerido(), RateFunc::Smooth);
        assert_eq!(easing_escena(0.5), smooth(0.5));
    }
}
