//! Plantillas AM1 — Essence of Calculus / Análisis Matemático 1.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, solo `std` (compila
//! standalone con `rustc --test`). Modela la MATEMÁTICA de 6 animaciones
//! estilo 3Blue1Brown; el dibujado RGBA vive en
//! `grafito-app/src/anim_native.rs` (el lead la cablea: `mod tpl_am1` en
//! `lib.rs` + brazos `render_*_frames` de 48 frames).
//!
//! No duplica las 3 ya existentes (`derivative-slope`, `integral-area`,
//! `taylor-series`): cubre lo que falta del curso AM1.
//!
//! | # | `TEMPLATE_IDS` | Qué anima | Fuente principal |
//! |---|----------------|-----------|--------------------|
//! | 1 | `riemann-sums` | rectángulos izq/med/der/trapecio con n→∞ geométrico | EoC cap. 8 (integración y TFC) |
//! | 2 | `epsilon-delta` | banda ε que achica + ventana δ = ε/\|m\| | EoC cap. 7 (límites, ε-δ) |
//! | 3 | `chain-rule` | u = g(x), y = f(u), dy/dx = dy/du·du/dx con residuo | EoC cap. 4 (regla de la cadena) |
//! | 4 | `taylor-remainder` | órdenes 1..=7 de sin(x) + resto/cota de Lagrange | EoC cap. 11 (Taylor) |
//! | 5 | `improper-integral` | ∫₁ᵇ dx/xᵖ con b→∞, test-p, converge/diverge | AM1 (criterio-p, OpenStax 3.7) |
//! | 6 | `ode-slope-field` | campo de pendientes + Euler introductorio | AM1 EDO / serie EDO 3b1b |
//!
//! ## Paridad con el resto del crate (reuso sin acoplar)
//!
//! El archivo es autocontenido a propósito (gate standalone `rustc --test`):
//! replica por paridad —no por importación— lo siguiente:
//! - `suave()` = smoothstep `3t²−2t³` de `anims.rs::smooth` y
//!   `textanim.rs::TextEasing::Smooth`.
//! - Fases 20 % setup / 60 % construcción / 20 % hold de
//!   `anim_native.rs::fase_para_frame` (`FASE_SETUP_HASTA`, 20..80 %).
//! - Presupuestos de `protocol.rs`: canvas 64..=4096 (`Resolution::try_new`),
//!   frames 1..=48 (`PARAMETRIC_MAX_FRAMES`, `NATIVE_ANIM_FRAME_COUNT` en
//!   `anim_native.rs`), set RGBA ≤ 64 MiB (`PARAMETRIC_MAX_BYTES`,
//!   `NATIVE_MAX_SET_BYTES`, `LONGFORM_CHUNK_MAX_BYTES`), 4 B/px.
//! - Órdenes Taylor 1..=7 (`TAYLOR_ANIM_ORDER_MIN/MAX/DEFAULT`).
//! - Claves vivas `a`/`b` (`SCENE_PARAM_A/B`), `x0`/`span`
//!   (`SCENE_PARAM_X0/SPAN`), `terms` (`SCENE_PARAM_TERMS`).
//!
//! ## Contrato para el renderer futuro (`anim_native.rs`, lo cablea el lead)
//!
//! - Cada spec expone `frames()` y muestreadores puros por frame
//!   (`n_en`, `epsilon_en`, `sonda_en`, `orden_en`, `b_en`, `punto_en`).
//! - `params_clave(id)` declara las claves vivas que leerá el futuro
//!   `render_*_frames_with_params` (ausente → default del spec).
//! - Paleta, ejes [-3,3]², ticks y fondo: únicos en `anim_native.rs`
//!   (este módulo no emite un solo píxel).
//!
//! ## Fuentes
//!
//! - 3Blue1Brown, Essence of calculus: cap. 4 (cadena), cap. 7 (límites ε-δ),
//!   cap. 8 (integración y TFC), cap. 11 (Taylor).
//!   <https://www.3blue1brown.com/lessons/essence-of-calculus/>
//! - 3Blue1Brown, "Visualizing the chain rule and product rule" (cap. 4):
//!   dy/dx = dy/du·du/dx como cancelación de los `dh` intermedios.
//!   <https://www.3blue1brown.com/lessons/chain-rule-and-product-rule>
//! - Riemann sums: `A = lim(n→∞) Σ f(xᵢ*)Δx` (partición regular).
//! - ε-δ: `∀ε>0 ∃δ>0: 0<|x−c|<δ ⟹ |f(x)−L|<ε` (MathWorld, OpenStax 2.5).
//! - Resto de Lagrange: `Rₙ = f⁽ⁿ⁺¹⁾(ξ)(x−a)ⁿ⁺¹/(n+1)!`; desigualdad de
//!   Taylor `|Rₙ| ≤ M|x−a|ⁿ⁺¹/(n+1)!` (con `M = 1` para sin/cos).
//! - Impropias: `∫₁^∞ dx/xᵖ` converge ⟺ `p > 1` (criterio-p, OpenStax 3.7).
//! - EDO intro: campo de pendientes + Euler explícito `y += h·f(x,y)`.
//!
//! Todo lo que excede presupuestos es `Err` honesto en rioplatense; jamás
//! nada parcial en silencio. Sin `unwrap`/`expect` en producción.

// ── Registro ─────────────────────────────────────────────────────────────

/// Las 6 plantillas AM1 nuevas (kebab-case, sin colisión con las 13
/// canónicas de `protocol.rs::CANONICAL_TEMPLATES` ni con las 3 ya
/// existentes `derivative-slope` / `integral-area` / `taylor-series`).
pub const TEMPLATE_IDS: &[&str] = &[
    "riemann-sums",
    "epsilon-delta",
    "chain-rule",
    "taylor-remainder",
    "improper-integral",
    "ode-slope-field",
];

/// ¿El id es una plantilla AM1 de este módulo? (trim + minúsculas).
pub fn is_am1_template(id: &str) -> bool {
    let t = id.trim().to_lowercase();
    TEMPLATE_IDS.contains(&t.as_str())
}

/// Ficha de una plantilla para la UI / el dispatcher futuro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateMeta {
    /// Id kebab-case (uno de [`TEMPLATE_IDS`]).
    pub id: &'static str,
    /// Título en español para rótulos.
    pub titulo: &'static str,
    /// Qué muestra, en una línea.
    pub usa: &'static str,
    /// Capítulo de referencia (EoC = Essence of calculus).
    pub capitulo: &'static str,
}

/// Ficha por id (`None` honesto si no es AM1).
pub fn describe(id: &str) -> Option<TemplateMeta> {
    match id.trim().to_lowercase().as_str() {
        "riemann-sums" => Some(TemplateMeta {
            id: "riemann-sums",
            titulo: "Sumas de Riemann",
            usa: "rectángulos que se achican con n→∞ hacia el área exacta",
            capitulo: "EoC cap. 8 (integración y teorema fundamental)",
        }),
        "epsilon-delta" => Some(TemplateMeta {
            id: "epsilon-delta",
            titulo: "Épsilon-delta",
            usa: "la banda ε se achica y la ventana δ la sigue: δ = ε/|m|",
            capitulo: "EoC cap. 7 (límites y definición ε-δ)",
        }),
        "chain-rule" => Some(TemplateMeta {
            id: "chain-rule",
            titulo: "Regla de la cadena",
            usa: "u = x² por dentro, sin(u) por fuera: el producto da sin(x²)′",
            capitulo: "EoC cap. 4 (regla de la cadena y del producto)",
        }),
        "taylor-remainder" => Some(TemplateMeta {
            id: "taylor-remainder",
            titulo: "Taylor con resto",
            usa: "órdenes 1..=7 de sin(x) con resto y cota de Lagrange",
            capitulo: "EoC cap. 11 (series de Taylor)",
        }),
        "improper-integral" => Some(TemplateMeta {
            id: "improper-integral",
            titulo: "Integral impropia",
            usa: "∫₁ᵇ dx/xᵖ con b→∞: converge si p > 1, diverge si no",
            capitulo: "AM1 (criterio-p de impropias)",
        }),
        "ode-slope-field" => Some(TemplateMeta {
            id: "ode-slope-field",
            titulo: "Campo de pendientes",
            usa: "tracitos del campo dy/dx = f(x,y) + un paso de Euler",
            capitulo: "AM1 EDO intro / serie EDO 3b1b",
        }),
        _ => None,
    }
}

/// Claves vivas que leerá el futuro `render_*_frames_with_params`
/// (paridad `protocol.rs::SCENE_PARAM_*`; ausente → default del spec).
pub fn params_clave(id: &str) -> &'static [&'static str] {
    match id.trim().to_lowercase().as_str() {
        "riemann-sums" => &["a", "b", "n"],
        "epsilon-delta" => &["a", "limite", "pendiente", "epsilon"],
        "chain-rule" => &["x0", "span"],
        "taylor-remainder" => &["terms", "x"],
        "improper-integral" => &["p", "b"],
        "ode-slope-field" => &["h", "x0", "y0"],
        _ => &[],
    }
}

// ── Presupuestos (paridad protocolo § docs superiores) ───────────────────

/// Frames máximos por animación (paridad `NATIVE_ANIM_FRAME_COUNT` = 48).
pub const AM1_MAX_FRAMES: usize = 48;
/// Lado mínimo del canvas (paridad `Resolution::try_new`).
pub const AM1_MIN_DIM: u32 = 64;
/// Lado máximo del canvas (paridad `Resolution::try_new`).
pub const AM1_MAX_DIM: u32 = 4096;
/// Tope del set RGBA en RAM (paridad `NATIVE_MAX_SET_BYTES` = 64 MiB).
pub const AM1_MAX_SET_BYTES: usize = 64 * 1024 * 1024;
/// Bytes por píxel RGBA (espejo `egui::ColorImage` / `NATIVE_BYTES_PER_PIXEL`).
pub const AM1_BYTES_PER_PIXEL: usize = 4;
/// Mundo de referencia de los ejes nativos ([-3,3]² en `anim_native.rs`).
pub const AM1_MUNDO_MIN: f64 = -3.0;
/// Mundo de referencia de los ejes nativos ([-3,3]² en `anim_native.rs`).
pub const AM1_MUNDO_MAX: f64 = 3.0;

// ── Error ────────────────────────────────────────────────────────────────

/// Error honesto AM1 (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq)]
pub enum Am1Error {
    /// Parámetro fuera de rango o no finito (lleva el detalle).
    ParametroInvalido { detalle: String },
    /// Frames fuera de 1..=48.
    FramesFueraDeRango { got: usize, max: usize },
    /// Vista fuera de 64..=4096 por lado.
    VistaFueraDeRango { detalle: String },
    /// Set estimado sobre 64 MiB.
    ExcedeMemoria { got: usize, max: usize },
}

impl std::fmt::Display for Am1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParametroInvalido { detalle } => {
                write!(f, "parámetro inválido: {detalle}")
            }
            Self::FramesFueraDeRango { got, max } => {
                write!(f, "frames fuera de rango: pediste {got}, válido 1..={max}")
            }
            Self::VistaFueraDeRango { detalle } => {
                write!(f, "vista inválida ({detalle}): usá lados 64..=4096")
            }
            Self::ExcedeMemoria { got, max } => {
                write!(
                    f,
                    "el set estimado ({got} bytes) excede el tope de {max} bytes: \
                     bajá la resolución o los fotogramas"
                )
            }
        }
    }
}

impl std::error::Error for Am1Error {}

// ── Utilidades puras compartidas ─────────────────────────────────────────

/// Guarda finita 0..1 (`NaN`/inf → 0). Pura.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Smoothstep `3t²−2t³` (paridad `anims.rs::smooth` / `TextEasing::Smooth`).
fn suave(t: f64) -> f64 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

/// Interpolación lineal con guarda finita (no finito → `a`). Pura.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let v = a + (b - a) * clamp01(t);
    if v.is_finite() {
        v
    } else {
        a
    }
}

/// Alpha de construcción por frame (paridad `fase_para_frame`: setup 20 %,
/// construcción 60 % con easing, hold 20 %). Total: frame ≥ frames → 1.
pub fn alpha_fase(frame: usize, frames: usize) -> f64 {
    if frames <= 1 {
        return 1.0;
    }
    let last = frames - 1;
    let t = (frame.min(last) as f64) / (last as f64);
    if t < 0.2 {
        0.0
    } else if t < 0.8 {
        suave((t - 0.2) / 0.6)
    } else {
        1.0
    }
}

/// Bytes RGBA estimados del set (`w*h*4*frames`). `None` si desborda.
pub fn estimate_set_bytes(w: u32, h: u32, frames: usize) -> Option<usize> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(AM1_BYTES_PER_PIXEL))
        .and_then(|v| v.checked_mul(frames))
}

/// Valida vista + frames + presupuesto del set (lo que el dispatcher chequea
/// antes de renderizar). Pura, sin pánicos.
pub fn vista_valida(w: u32, h: u32, frames: usize) -> Result<(), Am1Error> {
    if w < AM1_MIN_DIM || h < AM1_MIN_DIM || w > AM1_MAX_DIM || h > AM1_MAX_DIM {
        return Err(Am1Error::VistaFueraDeRango {
            detalle: format!("{w}x{h}"),
        });
    }
    if frames == 0 || frames > AM1_MAX_FRAMES {
        return Err(Am1Error::FramesFueraDeRango {
            got: frames,
            max: AM1_MAX_FRAMES,
        });
    }
    match estimate_set_bytes(w, h, frames) {
        Some(got) if got <= AM1_MAX_SET_BYTES => Ok(()),
        Some(got) => Err(Am1Error::ExcedeMemoria {
            got,
            max: AM1_MAX_SET_BYTES,
        }),
        None => Err(Am1Error::ExcedeMemoria {
            got: usize::MAX,
            max: AM1_MAX_SET_BYTES,
        }),
    }
}

fn valida_frames(frames: usize) -> Result<(), Am1Error> {
    if frames == 0 || frames > AM1_MAX_FRAMES {
        return Err(Am1Error::FramesFueraDeRango {
            got: frames,
            max: AM1_MAX_FRAMES,
        });
    }
    Ok(())
}

fn exige_finito_en_rango(v: f64, min: f64, max: f64, nombre: &str) -> Result<f64, Am1Error> {
    if !v.is_finite() || v < min || v > max {
        return Err(Am1Error::ParametroInvalido {
            detalle: format!("{nombre} = {v}: usá el rango {min}..={max}"),
        });
    }
    Ok(v)
}

// ── 1. Sumas de Riemann (n→∞) ────────────────────────────────────────────

/// Rectángulos máximos por frame (tope de dibujado honesto del renderer).
pub const RIEMANN_MAX_N: usize = 256;

/// Dónde se evalúa la altura de cada rectángulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiemannRegla {
    /// Extremo izquierdo.
    Izquierda,
    /// Punto medio (default: la que mejor converge en clase).
    #[default]
    PuntoMedio,
    /// Extremo derecho.
    Derecha,
    /// Trapecio (promedio de bordes).
    Trapecio,
}

impl RiemannRegla {
    /// Nombre kebab para params/rótulos.
    pub const fn nombre(self) -> &'static str {
        match self {
            Self::Izquierda => "izquierda",
            Self::PuntoMedio => "medio",
            Self::Derecha => "derecha",
            Self::Trapecio => "trapecio",
        }
    }

    /// Parsea el nombre (`None` honesto si no matchea).
    pub fn desde_nombre(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "izquierda" | "left" => Some(Self::Izquierda),
            "medio" | "midpoint" | "middle" => Some(Self::PuntoMedio),
            "derecha" | "right" => Some(Self::Derecha),
            "trapecio" | "trapezoid" => Some(Self::Trapecio),
            _ => None,
        }
    }
}

/// Función canónica no negativa (área honesta, con integral exacta).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiemannFuncion {
    /// `x²` (default: la parábola del curso).
    #[default]
    Cuadratica,
    /// `sin(x) + 1.5` (siempre ≥ 0.5, muestra una no-polinómica).
    SenoDesplazado,
}

impl RiemannFuncion {
    /// Evalúa (siempre finita en el dominio validado). Pura.
    pub fn eval(self, x: f64) -> f64 {
        match self {
            Self::Cuadratica => x * x,
            Self::SenoDesplazado => x.sin() + 1.5,
        }
    }

    /// Integral exacta en `[a, b]` (para el error honesto). Pura.
    pub fn integral_exacta(self, a: f64, b: f64) -> f64 {
        match self {
            Self::Cuadratica => (b * b * b - a * a * a) / 3.0,
            Self::SenoDesplazado => (a.cos() - b.cos()) + 1.5 * (b - a),
        }
    }

    /// Nombre kebab para params/rótulos.
    pub const fn nombre(self) -> &'static str {
        match self {
            Self::Cuadratica => "cuadratica",
            Self::SenoDesplazado => "seno",
        }
    }
}

/// Suma de Riemann animada: `n` crece geométrico `n_min → n_max` con el
/// alpha de fase (sensación `n→∞`), la suma tiende a la integral exacta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiemannSums {
    a: f64,
    b: f64,
    n_min: usize,
    n_max: usize,
    frames: usize,
    regla: RiemannRegla,
    funcion: RiemannFuncion,
}

impl RiemannSums {
    /// Constructor validado: `[a, b]` finito con `a < b` dentro de ±10,
    /// `1 <= n_min <= n_max <= 256`, frames 1..=48.
    pub fn try_new(
        a: f64,
        b: f64,
        n_min: usize,
        n_max: usize,
        frames: usize,
        regla: RiemannRegla,
        funcion: RiemannFuncion,
    ) -> Result<Self, Am1Error> {
        exige_finito_en_rango(a, -10.0, 10.0, "a")?;
        exige_finito_en_rango(b, -10.0, 10.0, "b")?;
        if a >= b {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("a = {a}, b = {b}: necesito a < b"),
            });
        }
        if n_min == 0 || n_min > n_max || n_max > RIEMANN_MAX_N {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!(
                    "n = {n_min}..={n_max}: usá 1 <= n_min <= n_max <= {RIEMANN_MAX_N}"
                ),
            });
        }
        valida_frames(frames)?;
        Ok(Self {
            a,
            b,
            n_min,
            n_max,
            frames,
            regla,
            funcion,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Subdivisiones en el frame (crecimiento geométrico: `n→∞` visual).
    pub fn n_en(&self, frame: usize) -> usize {
        let alpha = alpha_fase(frame, self.frames);
        if self.n_max <= self.n_min {
            return self.n_min;
        }
        let ratio = self.n_max as f64 / self.n_min as f64;
        let n = (self.n_min as f64 * ratio.powf(alpha)).round() as usize;
        n.clamp(self.n_min, self.n_max)
    }

    /// Ancho de cada rectángulo en el frame. Puro.
    pub fn ancho_en(&self, frame: usize) -> f64 {
        (self.b - self.a) / self.n_en(frame).max(1) as f64
    }

    /// Rectángulo `i` del frame: `(x0, x1, altura)`. `None` si `i >= n`.
    /// Pura, sin pánicos.
    pub fn rect_en(&self, frame: usize, i: usize) -> Option<(f64, f64, f64)> {
        let n = self.n_en(frame);
        if i >= n {
            return None;
        }
        let dx = (self.b - self.a) / n as f64;
        let x0 = self.a + i as f64 * dx;
        let x1 = x0 + dx;
        let h = match self.regla {
            RiemannRegla::Izquierda => self.funcion.eval(x0),
            RiemannRegla::PuntoMedio => self.funcion.eval((x0 + x1) / 2.0),
            RiemannRegla::Derecha => self.funcion.eval(x1),
            RiemannRegla::Trapecio => (self.funcion.eval(x0) + self.funcion.eval(x1)) / 2.0,
        };
        if h.is_finite() {
            Some((x0, x1, h))
        } else {
            None
        }
    }

    /// Suma del frame (`NaN` honesto si un parcial no es finito). Pura.
    pub fn suma_en(&self, frame: usize) -> f64 {
        let n = self.n_en(frame);
        let dx = (self.b - self.a) / n.max(1) as f64;
        let mut suma = 0.0;
        for i in 0..n {
            let (_, _, h) = match self.rect_en(frame, i) {
                Some(r) => r,
                None => return f64::NAN,
            };
            suma += h * dx;
            if !suma.is_finite() {
                return f64::NAN;
            }
        }
        suma
    }

    /// Área exacta bajo la curva en `[a, b]`. Pura.
    pub fn exacta(&self) -> f64 {
        self.funcion.integral_exacta(self.a, self.b)
    }

    /// Error absoluto del frame vs. la exacta (`NaN` honesto si no hay suma).
    pub fn error_en(&self, frame: usize) -> f64 {
        let s = self.suma_en(frame);
        if s.is_finite() {
            (s - self.exacta()).abs()
        } else {
            f64::NAN
        }
    }
}

// ── 2. Épsilon-delta ─────────────────────────────────────────────────────

/// Definición animada: ε se achica geométrico y δ la sigue con
/// `δ = ε/|m|` (exacto para la recta canónica: Lipschitz con igualdad).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EpsilonDelta {
    punto: f64,
    limite: f64,
    pendiente: f64,
    eps_max: f64,
    eps_min: f64,
    frames: usize,
}

impl EpsilonDelta {
    /// Constructor validado: punto/límite finitos en ±10, pendiente finita
    /// no nula en ±20 en valor absoluto, `0 < eps_min < eps_max <= 10`.
    pub fn try_new(
        punto: f64,
        limite: f64,
        pendiente: f64,
        eps_max: f64,
        eps_min: f64,
        frames: usize,
    ) -> Result<Self, Am1Error> {
        exige_finito_en_rango(punto, -10.0, 10.0, "punto")?;
        exige_finito_en_rango(limite, -10.0, 10.0, "limite")?;
        if !pendiente.is_finite() || pendiente == 0.0 || pendiente.abs() > 20.0 {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("pendiente = {pendiente}: usá un valor finito no nulo hasta 20"),
            });
        }
        if !eps_max.is_finite() || !eps_min.is_finite() || eps_min <= 0.0 || eps_min >= eps_max {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("epsilon = {eps_min}..={eps_max}: necesito 0 < min < max"),
            });
        }
        if eps_max > 10.0 {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("epsilon máximo = {eps_max}: usá hasta 10"),
            });
        }
        valida_frames(frames)?;
        Ok(Self {
            punto,
            limite,
            pendiente,
            eps_max,
            eps_min,
            frames,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Recta canónica `f(x) = m(x−a) + L`. Pura.
    pub fn eval(&self, x: f64) -> f64 {
        self.pendiente * (x - self.punto) + self.limite
    }

    /// Semialtura ε del frame (achique geométrico). Pura.
    pub fn epsilon_en(&self, frame: usize) -> f64 {
        let alpha = alpha_fase(frame, self.frames);
        let ratio = self.eps_min / self.eps_max;
        self.eps_max * ratio.powf(alpha)
    }

    /// Semiancho δ del frame (`ε/|m|`, exacto para la recta). Pura.
    pub fn delta_en(&self, frame: usize) -> f64 {
        self.epsilon_en(frame) / self.pendiente.abs()
    }

    /// δ para un ε dado (misma regla, para el scrub vivo). Pura.
    pub fn delta_para(&self, epsilon: f64) -> f64 {
        if epsilon.is_finite() && epsilon > 0.0 {
            epsilon / self.pendiente.abs()
        } else {
            f64::NAN
        }
    }

    /// ¿`x` cumple la definición en el frame?
    /// (`|x−a| < δ` implica `|f(x)−L| < ε`; chequea ambas). Pura.
    pub fn cumple_en(&self, x: f64, frame: usize) -> bool {
        if !x.is_finite() {
            return false;
        }
        let (eps, delta) = (self.epsilon_en(frame), self.delta_en(frame));
        (x - self.punto).abs() < delta && (self.eval(x) - self.limite).abs() < eps
    }

    /// Holgura `ε − |f(x)−L|` (≥ 0 = adentro de la banda). Pura.
    pub fn holgura_en(&self, x: f64, frame: usize) -> f64 {
        self.epsilon_en(frame) - (self.eval(x) - self.limite).abs()
    }
}

// ── 3. Regla de la cadena (visual) ───────────────────────────────────────

/// Composición canónica del EoC cap. 4: `g(x) = x²` por dentro,
/// `f(u) = sin(u)` por fuera, `h(x) = sin(x²)` total.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainRule {
    centro: f64,
    span: f64,
    frames: usize,
}

impl ChainRule {
    /// Constructor validado: centro en ±3, `span` en 0.25..=3.0
    /// (paridad `derivative-slope`), frames 1..=48.
    pub fn try_new(centro: f64, span: f64, frames: usize) -> Result<Self, Am1Error> {
        exige_finito_en_rango(centro, AM1_MUNDO_MIN, AM1_MUNDO_MAX, "x0")?;
        exige_finito_en_rango(span, 0.25, 3.0, "span")?;
        valida_frames(frames)?;
        Ok(Self {
            centro,
            span,
            frames,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Punto sonda del frame (barrido `centro−span → centro+span`). Pura.
    pub fn sonda_en(&self, frame: usize) -> f64 {
        lerp(
            self.centro - self.span,
            self.centro + self.span,
            alpha_fase(frame, self.frames),
        )
        .clamp(self.centro - self.span, self.centro + self.span)
    }

    /// Interior `u = g(x) = x²`. Pura.
    pub fn interior_en(x: f64) -> f64 {
        x * x
    }

    /// Total `h(x) = sin(x²)`. Pura.
    pub fn total_en(x: f64) -> f64 {
        (x * x).sin()
    }

    /// Derivada exterior `f′(u) = cos(u)`. Pura.
    pub fn derivada_exterior_en(u: f64) -> f64 {
        u.cos()
    }

    /// Derivada interior `g′(x) = 2x`. Pura.
    pub fn derivada_interior_en(x: f64) -> f64 {
        2.0 * x
    }

    /// Derivada total directa `h′(x) = 2x·cos(x²)`. Pura.
    pub fn derivada_total_en(x: f64) -> f64 {
        2.0 * x * (x * x).cos()
    }

    /// Los 4 números del frame: `(u, dy/du, du/dx, producto)`. Puros.
    pub fn factores_en(&self, frame: usize) -> (f64, f64, f64, f64) {
        let x = self.sonda_en(frame);
        let u = Self::interior_en(x);
        let dy_du = Self::derivada_exterior_en(u);
        let du_dx = Self::derivada_interior_en(x);
        (u, dy_du, du_dx, dy_du * du_dx)
    }

    /// Residuo `|directa − producto|` en el frame (cero exacto salvo
    /// redondeo: la gracia pedagógica es que coincide). Puro.
    pub fn residuo_en(&self, frame: usize) -> f64 {
        let x = self.sonda_en(frame);
        let (_, _, _, producto) = self.factores_en(frame);
        (Self::derivada_total_en(x) - producto).abs()
    }
}

// ── 4. Taylor con resto / órdenes ────────────────────────────────────────

/// Orden mínimo animado (paridad `TAYLOR_ANIM_ORDER_MIN`).
pub const TAYLOR_ORDEN_MIN: usize = 1;
/// Orden máximo animado (paridad `TAYLOR_ANIM_ORDER_MAX` = 7).
pub const TAYLOR_ORDEN_MAX: usize = 7;

/// Maclaurin de `sin(x)` por órdenes con resto y cota de Lagrange
/// (`|Rₙ| ≤ |x|ⁿ⁺¹/(n+1)!`, porque `|f⁽ⁿ⁺¹⁾| ≤ 1`).
/// Complementa `taylor-series` (que dibuja `Pₙ`): acá el foco es el ERROR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaylorRemainder {
    x_eval: f64,
    orden_min: usize,
    orden_max: usize,
    frames: usize,
}

fn factorial(n: usize) -> f64 {
    let mut acc = 1.0;
    for k in 2..=n {
        acc *= k as f64;
    }
    acc
}

/// Polinomio de Maclaurin de `sin` hasta grado `n` (solo impares aportan).
fn maclaurin_seno(x: f64, grado: usize) -> f64 {
    let mut suma = 0.0;
    let mut k = 0;
    loop {
        let g = 2 * k + 1;
        if g > grado {
            break;
        }
        let signo = if k % 2 == 0 { 1.0 } else { -1.0 };
        suma += signo * x.powi(g as i32) / factorial(g);
        k += 1;
    }
    suma
}

impl TaylorRemainder {
    /// Constructor validado: `x` finito en ±3.5,
    /// `1 <= orden_min <= orden_max <= 7`, frames 1..=48.
    pub fn try_new(
        x_eval: f64,
        orden_min: usize,
        orden_max: usize,
        frames: usize,
    ) -> Result<Self, Am1Error> {
        exige_finito_en_rango(x_eval, -3.5, 3.5, "x")?;
        if orden_min < TAYLOR_ORDEN_MIN || orden_min > orden_max || orden_max > TAYLOR_ORDEN_MAX {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!(
                    "órdenes = {orden_min}..={orden_max}: usá 1 <= min <= max <= {TAYLOR_ORDEN_MAX}"
                ),
            });
        }
        valida_frames(frames)?;
        Ok(Self {
            x_eval,
            orden_min,
            orden_max,
            frames,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Orden del frame (escalones enteros `min → max`). Puro.
    pub fn orden_en(&self, frame: usize) -> usize {
        let alpha = alpha_fase(frame, self.frames);
        let span = self.orden_max - self.orden_min;
        let paso = ((alpha * (span as f64 + 1.0)).floor() as usize).min(span);
        self.orden_min + paso
    }

    /// Aproximación `Pₙ(x)` del frame. Pura.
    pub fn aprox_en(&self, frame: usize) -> f64 {
        maclaurin_seno(self.x_eval, self.orden_en(frame))
    }

    /// Resto real `sin(x) − Pₙ(x)` del frame (con signo). Puro.
    pub fn resto_en(&self, frame: usize) -> f64 {
        self.x_eval.sin() - self.aprox_en(frame)
    }

    /// Cota de Lagrange `|x|ⁿ⁺¹/(n+1)!` del frame. Pura.
    pub fn cota_en(&self, frame: usize) -> f64 {
        let n = self.orden_en(frame);
        self.x_eval.abs().powi((n + 1) as i32) / factorial(n + 1)
    }

    /// Menor orden en `[min, max]` cuya cota baja de `tol`
    /// (`None` honesto si ninguno alcanza). Puro.
    pub fn orden_suficiente(&self, tol: f64) -> Option<usize> {
        if !tol.is_finite() || tol <= 0.0 {
            return None;
        }
        for n in self.orden_min..=self.orden_max {
            let cota = self.x_eval.abs().powi((n + 1) as i32) / factorial(n + 1);
            if cota < tol {
                return Some(n);
            }
        }
        None
    }
}

// ── 5. Integrales impropias ──────────────────────────────────────────────

/// Convergencia del test-p.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convergencia {
    /// `p > 1`: la cola tiende al valor finito.
    Converge,
    /// `p <= 1`: la cola no tiene límite finito.
    Diverge,
}

/// `∫₁ᵇ dx/xᵖ` con `b → ∞` (barrido geométrico): el caso que decide el
/// criterio-p en clase. `p = 1` da `ln b`, el resto sale cerrado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImproperIntegral {
    p: f64,
    b_min: f64,
    b_max: f64,
    frames: usize,
}

impl ImproperIntegral {
    /// Constructor validado: `p` finito en 0.25..=4.0,
    /// `1 <= b_min < b_max <= 10000`, frames 1..=48.
    pub fn try_new(p: f64, b_min: f64, b_max: f64, frames: usize) -> Result<Self, Am1Error> {
        exige_finito_en_rango(p, 0.25, 4.0, "p")?;
        if !b_min.is_finite() || !b_max.is_finite() || b_min < 1.0 || b_min >= b_max {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("b = {b_min}..={b_max}: necesito 1 <= min < max"),
            });
        }
        if b_max > 10_000.0 {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("b máximo = {b_max}: usá hasta 10000"),
            });
        }
        valida_frames(frames)?;
        Ok(Self {
            p,
            b_min,
            b_max,
            frames,
        })
    }

    /// Fotogramas.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Cota superior del frame (`b → ∞` geométrico). Pura.
    pub fn b_en(&self, frame: usize) -> f64 {
        let alpha = alpha_fase(frame, self.frames);
        self.b_min * (self.b_max / self.b_min).powf(alpha)
    }

    /// Valor truncado `∫₁ᵇ dx/xᵖ` del frame. Puro.
    pub fn valor_en(&self, frame: usize) -> f64 {
        let b = self.b_en(frame);
        if (self.p - 1.0).abs() < 1e-12 {
            b.ln()
        } else {
            (1.0 - b.powf(1.0 - self.p)) / (self.p - 1.0)
        }
    }

    /// Estado según el criterio-p. Puro.
    pub fn estado(&self) -> Convergencia {
        if self.p > 1.0 {
            Convergencia::Converge
        } else {
            Convergencia::Diverge
        }
    }

    /// Límite cuando `b → ∞` (`Some(1/(p−1))` si converge, `None` si no).
    /// Puro.
    pub fn limite(&self) -> Option<f64> {
        if self.p > 1.0 {
            Some(1.0 / (self.p - 1.0))
        } else {
            None
        }
    }

    /// Distancia del frame al límite (`None` honesto si diverge). Pura.
    pub fn distancia_en(&self, frame: usize) -> Option<f64> {
        self.limite().map(|l| (self.valor_en(frame) - l).abs())
    }
}

// ── 6. EDO: campo de pendientes introductorio ────────────────────────────

/// Lado máximo de la grilla del campo (32×32 = 1024 tracitos, tope honesto).
pub const SLOPE_MAX_N: usize = 32;
/// Pasos máximos del Euler introductorio.
pub const EULER_MAX_PASOS: usize = 256;

/// Campo canónico `dy/dx = f(x, y)` (autónomos y suaves: Euler estable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Campo {
    /// `dy/dx = −y/2` (default: decaimiento, solución `e^(−x/2)` exacta).
    #[default]
    Decaimiento,
    /// `dy/dx = x − y` (no autónomo suave para comparar).
    RectaMenosY,
}

impl Campo {
    /// Pendiente en `(x, y)` (`None` honesto si no es finita). Pura.
    pub fn pendiente(self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let m = match self {
            Self::Decaimiento => -y / 2.0,
            Self::RectaMenosY => x - y,
        };
        if m.is_finite() {
            Some(m)
        } else {
            None
        }
    }

    /// Nombre kebab para params/rótulos.
    pub const fn nombre(self) -> &'static str {
        match self {
            Self::Decaimiento => "decaimiento",
            Self::RectaMenosY => "recta-menos-y",
        }
    }

    /// Solución exacta que pasa por `(x0, y0)` (solo `Decaimiento` la tiene
    /// cerrada; el resto es `None` honesto). Pura.
    pub fn exacta(self, x0: f64, y0: f64, x: f64) -> Option<f64> {
        match self {
            Self::Decaimiento => {
                let y = y0 * (-(x - x0) / 2.0).exp();
                if y.is_finite() {
                    Some(y)
                } else {
                    None
                }
            }
            Self::RectaMenosY => None,
        }
    }
}

/// Grilla del campo de pendientes (centros de celda en el rectángulo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopeFieldSpec {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    nx: usize,
    ny: usize,
}

impl SlopeFieldSpec {
    /// Constructor validado: rectángulo finito con `x0 < x1`, `y0 < y1`
    /// dentro de ±10, grilla 2..=32 por lado.
    pub fn try_new(
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        nx: usize,
        ny: usize,
    ) -> Result<Self, Am1Error> {
        for (v, nombre) in [(x0, "x0"), (x1, "x1"), (y0, "y0"), (y1, "y1")] {
            exige_finito_en_rango(v, -10.0, 10.0, nombre)?;
        }
        if x0 >= x1 || y0 >= y1 {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("rectángulo [{x0},{x1}]×[{y0},{y1}]: necesito x0 < x1 e y0 < y1"),
            });
        }
        if !(2..=SLOPE_MAX_N).contains(&nx) || !(2..=SLOPE_MAX_N).contains(&ny) {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("grilla {nx}×{ny}: usá 2..={SLOPE_MAX_N} por lado"),
            });
        }
        Ok(Self {
            x0,
            x1,
            y0,
            y1,
            nx,
            ny,
        })
    }

    /// Cantidad de tracitos (`nx*ny`, ≤ 1024 por construcción).
    pub fn cantidad(&self) -> usize {
        self.nx * self.ny
    }

    /// Tracito `(i, j)`: `(x, y, ux, uy)` unitario en la dirección `(1, m)`.
    /// `None` honesto si el índice o la pendiente no son finitos. Pura.
    pub fn segmento_en(&self, campo: Campo, i: usize, j: usize) -> Option<(f64, f64, f64, f64)> {
        if i >= self.nx || j >= self.ny {
            return None;
        }
        let x = self.x0 + (i as f64 + 0.5) * (self.x1 - self.x0) / self.nx as f64;
        let y = self.y0 + (j as f64 + 0.5) * (self.y1 - self.y0) / self.ny as f64;
        let m = campo.pendiente(x, y)?;
        let norma = (1.0 + m * m).sqrt();
        if !norma.is_finite() || norma <= 0.0 {
            return None;
        }
        Some((x, y, 1.0 / norma, m / norma))
    }
}

/// Un Euler explícito introductorio: `y += h·f(x, y)` desde `(x_ini, y_ini)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EulerIntro {
    campo: Campo,
    x_ini: f64,
    y_ini: f64,
    h: f64,
    pasos: usize,
}

impl EulerIntro {
    /// Constructor validado: inicio finito en ±10, `h` en 0.001..=0.5,
    /// pasos 1..=256.
    pub fn try_new(
        campo: Campo,
        x_ini: f64,
        y_ini: f64,
        h: f64,
        pasos: usize,
    ) -> Result<Self, Am1Error> {
        exige_finito_en_rango(x_ini, -10.0, 10.0, "x0")?;
        exige_finito_en_rango(y_ini, -10.0, 10.0, "y0")?;
        exige_finito_en_rango(h, 0.001, 0.5, "h")?;
        if !(1..=EULER_MAX_PASOS).contains(&pasos) {
            return Err(Am1Error::ParametroInvalido {
                detalle: format!("pasos = {pasos}: usá 1..={EULER_MAX_PASOS}"),
            });
        }
        Ok(Self {
            campo,
            x_ini,
            y_ini,
            h,
            pasos,
        })
    }

    /// Cantidad de pasos.
    pub fn pasos(&self) -> usize {
        self.pasos
    }

    /// Punto tras `k` pasos (`k = 0` = inicio; `None` si `k > pasos` o el
    /// esquema se va a no-finito). Puro y determinista.
    pub fn punto_en(&self, k: usize) -> Option<(f64, f64)> {
        if k > self.pasos {
            return None;
        }
        let (mut x, mut y) = (self.x_ini, self.y_ini);
        for _ in 0..k {
            let m = self.campo.pendiente(x, y)?;
            x += self.h;
            y += self.h * m;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
        }
        Some((x, y))
    }

    /// Error vs. la exacta en el paso `k` (`None` honesto sin exacta o sin
    /// punto). Puro.
    pub fn error_en(&self, k: usize) -> Option<f64> {
        let (x, y) = self.punto_en(k)?;
        self.campo
            .exacta(self.x_ini, self.y_ini, x)
            .map(|e| (y - e).abs())
    }
}

// ── Tests inline (mismo gate `cargo test` una vez cableado) ──────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registro_kebab_unico_sin_colision() {
        assert_eq!(TEMPLATE_IDS.len(), 6);
        for id in TEMPLATE_IDS {
            assert_eq!(*id, id.trim().to_lowercase(), "kebab-case: {id}");
            assert!(id.contains('-'), "kebab-case con guion: {id}");
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "solo minúsculas y guiones: {id}"
            );
            assert!(is_am1_template(id), "{id} se reconoce");
            assert!(describe(id).is_some(), "{id} tiene ficha");
            assert!(!params_clave(id).is_empty(), "{id} declara params");
        }
        let mut v = TEMPLATE_IDS.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 6, "sin duplicados");
        // No pisa las 3 ya existentes (ni las otras 10 canónicas).
        for vieja in [
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
            assert!(!is_am1_template(vieja), "{vieja} no es AM1");
        }
        assert!(!is_am1_template("no-existe"));
        assert!(describe("no-existe").is_none());
        assert!(params_clave("no-existe").is_empty());
        assert!(is_am1_template("  RIEMANN-SUMS  "));
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(AM1_MAX_FRAMES, 48);
        assert_eq!((AM1_MIN_DIM, AM1_MAX_DIM), (64, 4096));
        assert_eq!(AM1_MAX_SET_BYTES, 64 * 1024 * 1024);
        assert_eq!(AM1_BYTES_PER_PIXEL, 4);
        // Set canónico 640×480×48 = 58_982_400 B: entra holgado en 64 MiB.
        assert_eq!(estimate_set_bytes(640, 480, 48), Some(58_982_400));
        assert!(vista_valida(640, 480, 48).is_ok());
        assert!(vista_valida(64, 64, 1).is_ok());
        assert!(vista_valida(4096, 4096, 48).is_err());
        assert!(vista_valida(63, 480, 48).is_err());
        assert!(vista_valida(640, 480, 0).is_err());
        assert!(vista_valida(640, 480, 49).is_err());
        assert!(estimate_set_bytes(0, 0, 0).is_some());
    }

    #[test]
    fn fase_setup_construye_hold() {
        assert_eq!(alpha_fase(0, 48), 0.0);
        assert_eq!(alpha_fase(47, 48), 1.0);
        assert_eq!(alpha_fase(0, 1), 1.0);
        let mut anterior = 0.0;
        for k in 0..48 {
            let a = alpha_fase(k, 48);
            assert!(a + 1e-12 >= anterior, "monótona en k={k}");
            anterior = a;
        }
        // Setup quieto (20 %) y hold final (último 20 %).
        assert_eq!(alpha_fase(4, 48), 0.0);
        let medio = alpha_fase(24, 48);
        assert!(medio > 0.0 && medio < 1.0, "construye en el medio: {medio}");
    }

    #[test]
    fn riemann_n_crece_y_converge() {
        let r = RiemannSums::try_new(
            0.0,
            2.0,
            2,
            128,
            48,
            RiemannRegla::PuntoMedio,
            RiemannFuncion::Cuadratica,
        )
        .unwrap();
        assert_eq!(r.frames(), 48);
        assert_eq!(r.n_en(0), 2);
        assert_eq!(r.n_en(47), 128);
        let mut anterior = 0;
        for k in 0..48 {
            let n = r.n_en(k);
            assert!(n >= anterior, "n no decrece en k={k}");
            anterior = n;
        }
        // Punto medio en x² [0,2]: el error final es mucho menor que el inicial.
        let e0 = r.error_en(0);
        let e1 = r.error_en(47);
        assert!(e0.is_finite() && e1.is_finite());
        assert!(e1 < e0 / 10.0, "e0={e0} e1={e1}");
        assert!((r.exacta() - 8.0 / 3.0).abs() < 1e-12);
        assert_eq!(r.rect_en(47, 128), None);
        let (x0, x1, h) = r.rect_en(0, 0).unwrap();
        assert!(x0 < x1 && h >= 0.0);
        // Reglas por nombre + bordes.
        assert_eq!(
            RiemannRegla::desde_nombre("medio"),
            Some(RiemannRegla::PuntoMedio)
        );
        assert_eq!(
            RiemannRegla::desde_nombre(" TRAPECIO "),
            Some(RiemannRegla::Trapecio)
        );
        assert_eq!(RiemannRegla::desde_nombre("cosa"), None);
        assert!(RiemannSums::try_new(
            2.0,
            0.0,
            1,
            4,
            8,
            RiemannRegla::Izquierda,
            RiemannFuncion::Cuadratica
        )
        .is_err());
        assert!(RiemannSums::try_new(
            0.0,
            2.0,
            0,
            4,
            8,
            RiemannRegla::Izquierda,
            RiemannFuncion::Cuadratica
        )
        .is_err());
        assert!(RiemannSums::try_new(
            0.0,
            2.0,
            1,
            257,
            8,
            RiemannRegla::Izquierda,
            RiemannFuncion::Cuadratica
        )
        .is_err());
        assert!(RiemannSums::try_new(
            0.0,
            2.0,
            1,
            4,
            0,
            RiemannRegla::Izquierda,
            RiemannFuncion::Cuadratica
        )
        .is_err());
        // Seno desplazado: exacta cerrada y suma finita.
        let s = RiemannSums::try_new(
            0.0,
            std::f64::consts::PI,
            4,
            64,
            24,
            RiemannRegla::Trapecio,
            RiemannFuncion::SenoDesplazado,
        )
        .unwrap();
        assert!((s.exacta() - (2.0 + 1.5 * std::f64::consts::PI)).abs() < 1e-9);
        assert!(s.suma_en(23).is_finite());
    }

    #[test]
    fn epsilon_delta_achica_y_cumple() {
        let e = EpsilonDelta::try_new(1.0, 2.0, 2.0, 1.0, 0.01, 48).unwrap();
        assert_eq!(e.frames(), 48);
        let eps0 = e.epsilon_en(0);
        let eps1 = e.epsilon_en(47);
        assert!((eps0 - 1.0).abs() < 1e-12);
        assert!((eps1 - 0.01).abs() < 1e-9, "eps1={eps1}");
        assert!((e.delta_en(10) - e.epsilon_en(10) / 2.0).abs() < 1e-12);
        assert!((e.delta_para(0.5) - 0.25).abs() < 1e-12);
        assert!(e.delta_para(-1.0).is_nan());
        // En el punto siempre cumple; lejos del punto, no.
        assert!(e.cumple_en(1.0, 20));
        assert!(!e.cumple_en(9.0, 47));
        assert!(!e.cumple_en(f64::NAN, 10));
        assert!(e.holgura_en(1.0, 10) > 0.0);
        assert!(EpsilonDelta::try_new(1.0, 2.0, 0.0, 1.0, 0.01, 8).is_err());
        assert!(EpsilonDelta::try_new(1.0, 2.0, 2.0, 0.01, 1.0, 8).is_err());
        assert!(EpsilonDelta::try_new(1.0, 2.0, f64::INFINITY, 1.0, 0.01, 8).is_err());
    }

    #[test]
    fn cadena_producto_igual_directa() {
        let c = ChainRule::try_new(0.0, 1.5, 48).unwrap();
        assert_eq!(c.frames(), 48);
        assert!((c.sonda_en(0) - (-1.5)).abs() < 1e-12);
        assert!((c.sonda_en(47) - 1.5).abs() < 1e-12);
        for k in 0..48 {
            assert!(c.residuo_en(k) < 1e-9, "k={k}: {}", c.residuo_en(k));
        }
        let (u, dy_du, du_dx, prod) = c.factores_en(47);
        assert!((u - 2.25).abs() < 1e-12);
        assert!((prod - ChainRule::derivada_total_en(1.5)).abs() < 1e-12);
        assert!((dy_du - 2.25f64.cos()).abs() < 1e-12);
        assert!((du_dx - 3.0).abs() < 1e-12);
        assert!((ChainRule::total_en(0.0) - 0.0).abs() < 1e-12);
        assert!(ChainRule::try_new(0.0, 0.1, 8).is_err());
        assert!(ChainRule::try_new(99.0, 1.0, 8).is_err());
    }

    #[test]
    fn taylor_resto_acotado_y_orden_suficiente() {
        let t = TaylorRemainder::try_new(1.0, 1, 7, 48).unwrap();
        assert_eq!(t.frames(), 48);
        assert_eq!(t.orden_en(0), 1);
        assert_eq!(t.orden_en(47), 7);
        let mut anterior = 0;
        for k in 0..48 {
            let n = t.orden_en(k);
            assert!(n >= anterior, "órdenes no decrecen en k={k}");
            anterior = n;
            // El resto real nunca supera la cota (salvo redondeo ínfimo).
            assert!(
                t.resto_en(k).abs() <= t.cota_en(k) * (1.0 + 1e-9),
                "k={k}: resto={} cota={}",
                t.resto_en(k),
                t.cota_en(k)
            );
        }
        // P₁(1) = 1 y la cota de grado 7 en x=1 es 1/8! ≈ 2.48e-5.
        assert!((t.aprox_en(0) - 1.0).abs() < 1e-12);
        assert!((t.cota_en(47) - 1.0 / 40320.0).abs() < 1e-12);
        assert_eq!(t.orden_suficiente(0.05), Some(3));
        // Ojo honesto: en sin los grados pares no agregan término (P₄ = P₃),
        // así que la cota que baja de 0.01 es la de grado 4 (1/120).
        assert_eq!(t.orden_suficiente(0.01), Some(4));
        assert_eq!(t.orden_suficiente(1e-12), None);
        assert_eq!(t.orden_suficiente(f64::NAN), None);
        assert!(TaylorRemainder::try_new(1.0, 0, 7, 8).is_err());
        assert!(TaylorRemainder::try_new(1.0, 1, 8, 8).is_err());
        assert!(TaylorRemainder::try_new(99.0, 1, 7, 8).is_err());
    }

    #[test]
    fn impropia_criterio_p() {
        let conv = ImproperIntegral::try_new(2.0, 1.0, 1000.0, 48).unwrap();
        assert_eq!(conv.estado(), Convergencia::Converge);
        assert_eq!(conv.limite(), Some(1.0));
        assert!((conv.b_en(0) - 1.0).abs() < 1e-12);
        assert!((conv.b_en(47) - 1000.0).abs() < 1e-6);
        assert!((conv.valor_en(47) - 0.999).abs() < 1e-9);
        let d0 = conv.distancia_en(0).unwrap();
        let d1 = conv.distancia_en(47).unwrap();
        assert!(d1 < d0, "se acerca al límite: {d0} → {d1}");
        let div = ImproperIntegral::try_new(1.0, 1.0, 1000.0, 48).unwrap();
        assert_eq!(div.estado(), Convergencia::Diverge);
        assert_eq!(div.limite(), None);
        assert_eq!(div.distancia_en(10), None);
        assert!((div.valor_en(47) - 1000.0f64.ln()).abs() < 1e-9);
        let div2 = ImproperIntegral::try_new(0.5, 1.0, 100.0, 8).unwrap();
        assert_eq!(div2.estado(), Convergencia::Diverge);
        assert!(div2.valor_en(7).is_finite());
        assert!(ImproperIntegral::try_new(0.1, 1.0, 10.0, 8).is_err());
        assert!(ImproperIntegral::try_new(2.0, 5.0, 5.0, 8).is_err());
        assert!(ImproperIntegral::try_new(2.0, 0.5, 10.0, 8).is_err());
    }

    #[test]
    fn campo_unitario_y_euler_cerca_exacta() {
        let g = SlopeFieldSpec::try_new(-3.0, 3.0, -3.0, 3.0, 8, 8).unwrap();
        assert_eq!(g.cantidad(), 64);
        for i in 0..8 {
            for j in 0..8 {
                let (x, y, ux, uy) = g.segmento_en(Campo::Decaimiento, i, j).unwrap();
                assert!(x.is_finite() && y.is_finite());
                assert!((ux * ux + uy * uy - 1.0).abs() < 1e-12, "unitario");
            }
        }
        assert!(g.segmento_en(Campo::Decaimiento, 8, 0).is_none());
        assert_eq!(Campo::Decaimiento.nombre(), "decaimiento");
        assert!(Campo::RectaMenosY.exacta(0.0, 1.0, 1.0).is_none());
        // Euler con h chico sigue la exacta del decaimiento.
        let eu = EulerIntro::try_new(Campo::Decaimiento, 0.0, 2.0, 0.01, 100).unwrap();
        assert_eq!(eu.pasos(), 100);
        assert_eq!(eu.punto_en(0), Some((0.0, 2.0)));
        assert!(eu.punto_en(101).is_none());
        let err = eu.error_en(100).unwrap();
        assert!(err < 0.02, "error euler h=0.01 x=1: {err}");
        // Determinista: dos pasadas dan lo mismo.
        assert_eq!(eu.punto_en(50), eu.punto_en(50));
        assert!(SlopeFieldSpec::try_new(1.0, 1.0, 0.0, 1.0, 8, 8).is_err());
        assert!(SlopeFieldSpec::try_new(-3.0, 3.0, -3.0, 3.0, 1, 8).is_err());
        assert!(SlopeFieldSpec::try_new(-3.0, 3.0, -3.0, 3.0, 33, 8).is_err());
        assert!(EulerIntro::try_new(Campo::Decaimiento, 0.0, 1.0, 0.0, 8).is_err());
        assert!(EulerIntro::try_new(Campo::Decaimiento, 0.0, 1.0, 0.01, 0).is_err());
        assert!(Campo::Decaimiento.pendiente(f64::NAN, 0.0).is_none());
    }

    #[test]
    fn errores_hablan_rioplatense() {
        let e = Am1Error::FramesFueraDeRango { got: 99, max: 48 };
        assert!(e.to_string().contains("99"));
        let e = Am1Error::ParametroInvalido {
            detalle: "x = 1".to_string(),
        };
        assert!(e.to_string().contains("parámetro inválido"));
        let e = Am1Error::VistaFueraDeRango {
            detalle: "1x1".to_string(),
        };
        assert!(e.to_string().contains("64..=4096"));
        let e = Am1Error::ExcedeMemoria { got: 1, max: 2 };
        assert!(e.to_string().contains("tope"));
    }
}
