//! Plantillas AM2 / multivariable (estilo 3b1b, agente 8/20).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, solo `std` (sin
//! dependencias nuevas). Este módulo NO dibuja píxeles: emite la geometría
//! exacta que el renderer nativo (`grafito-app::anim_native`, que cablea el
//! lead) rasteriza a buffers RGBA. Por eso es autocontenido y se verifica
//! standalone con `rustc --test` sobre este archivo.
//!
//! ## Escenas (ver `TEMPLATE_IDS`)
//!
//! - `partial-derivatives`: cortes `y = cte` / `x = cte` de `f(x, y)` con sus
//!   rectas tangentes de pendientes `fx` / `fy`, y el plano tangente que las
//!   une (GeoGebra "Visualizing the Tangent Plane": la pendiente de cada
//!   corte es la derivada parcial).
//! - `gradient-descent`: descenso por gradiente sobre un bowl (paisaje de
//!   loss à la Li et al. 2018 / 3b1b "Gradient descent, how neural networks
//!   learn"): la partícula sigue `-∇L` y la pérdida baja monótona.
//! - `lagrange-multipliers`: restricción (círculo) vs curvas de nivel de
//!   `f(x, y) = x + y`; el óptimo cae donde son tangentes, i.e.
//!   `∇f = λ∇g` (OpenStax Calculus v3 §4.8; LibreTexts §14.8).
//! - `double-integral`: volumen bajo `z = f(x, y)` como suma de rebanadas
//!   `A(x) = ∫f dy` (Fubini / Strang §14.1: "sum of slices").
//! - `green-stokes`: circulación sobre el círculo y su igualdad con
//!   `∬(∂Q/∂x − ∂P/∂y)` (Green circulación, OpenStax §6.4; Stokes en el
//!   plano `z = 0`).
//! - `jacobian`: deformación polar `T(r, θ)` con `J = r` y
//!   `dA = |J| dr dθ` (OpenStax §5.7; UT Austin "Jacobians").
//!
//! ## Paridad con el resto del crate (espejos, no imports)
//!
//! El archivo es standalone a propósito (se compila solo con `rustc --test`),
//! así que duplica lo mínimo con los mismos números:
//!
//! - `suave(t) = 3t²−2t³`: idéntico a `anims::smooth` / `RateFunc::Smooth`.
//! - Muestreos de superficie: res `2..=128`, cotas `±1e6`, como
//!   `surfaces3d::Surface3D` (`SURF3D_MIN/MAX_RES`, `SURF3D_MAX_COORD`).
//! - Presupuestos: frames nativos 48 (`anim_native::NATIVE_ANIM_FRAME_COUNT`),
//!   corto 64 / largo 1500 (`protocol::PREVIEW_SHORT_MAX_FRAMES` /
//!   `VIDEO_LONGFORM_MAX_FRAMES`), set 64 MiB (`NATIVE_MAX_SET_BYTES`,
//!   `LONGFORM_CHUNK_MAX_BYTES`), línea 64 KiB (`DEFAULT_LINE_CAP_BYTES`),
//!   canvas `64..=4096` (`protocol::Resolution`), duración `100..=60000 ms`
//!   (`AnimDuration` / `MAX_TIMELINE_DURATION_MS`), puntos por polilínea
//!   `1..=4096` (`MAX_MOBJECT_POINTS`), títulos `1..=200` chars
//!   (`textanim::TEXTANIM_MAX_CHARS`).
//!
//! Todo lo que excede es `Err` honesto en rioplatense; jamás `unwrap` /
//! `expect` / pánicos.

use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};

// ── Presupuestos (paridad protocolo + nativo) ─────────────────────────────

/// Frames por escena (paridad `NATIVE_ANIM_FRAME_COUNT`).
pub const AM2_FRAMES: usize = 48;
/// Tope corto (`gif`/`png`, paridad `PREVIEW_SHORT_MAX_FRAMES`).
pub const AM2_TOPE_CORTO: usize = 64;
/// Tope largo (`mp4`/`webm`, paridad `VIDEO_LONGFORM_MAX_FRAMES`).
pub const AM2_TOPE_LARGO: usize = 1500;
/// Tope del set RGBA en RAM (paridad `NATIVE_MAX_SET_BYTES`).
pub const AM2_TOPE_SET_BYTES: usize = 64 * 1024 * 1024;
/// Tope por línea de wire (paridad `DEFAULT_LINE_CAP_BYTES`).
pub const AM2_LINE_CAP_BYTES: usize = 64 * 1024;
/// Lado mínimo del canvas (paridad `Resolution`).
pub const AM2_CANVAS_MIN: u32 = 64;
/// Lado máximo del canvas (paridad `Resolution`).
pub const AM2_CANVAS_MAX: u32 = 4096;
/// Duración mínima en ms (paridad `AnimDuration` 0.1 s).
pub const AM2_DUR_MIN_MS: u64 = 100;
/// Duración máxima en ms (paridad `MAX_TIMELINE_DURATION_MS`).
pub const AM2_DUR_MAX_MS: u64 = 60_000;
/// Duración default por escena (dentro de `100..=60000`).
pub const AM2_DUR_DEFAULT_MS: u64 = 4000;
/// Puntos máximos por polilínea (paridad `MAX_MOBJECT_POINTS`).
pub const AM2_TOPE_PUNTOS: usize = 4096;
/// Curvas de nivel máximas por escena (acota memoria de la UI).
pub const AM2_TOPE_NIVELES: usize = 32;
/// Pasos máximos del descenso (un punto por frame: `AM2_FRAMES`).
pub const AM2_TOPE_PASOS: usize = 48;
/// Chars máximos de títulos (paridad `TEXTANIM_MAX_CHARS`).
pub const AM2_MAX_TITULO_CHARS: usize = 200;
/// Res mínima de grilla de superficie (paridad `SURF3D_MIN_RES`).
pub const AM2_RES_MIN: usize = 2;
/// Res máxima de grilla de superficie (paridad `SURF3D_MAX_RES`).
pub const AM2_RES_MAX: usize = 128;
/// Cota de coordenadas (paridad `SURF3D_MAX_COORD`).
pub const AM2_MAX_COORD: f64 = 1_000_000.0;
/// Ventana de trabajo en el plano (`x`, `y` en `[-3, 3]`, paridad
/// `SCENE_PARAM_X0` / `span` de `derivative-slope`).
pub const AM2_VENTANA: f64 = 3.0;
/// Tolerancia de tangencia / nivel (cruces honesto, no exactitud fingida).
pub const AM2_TOL: f64 = 1e-9;

// ── Registro de plantillas ────────────────────────────────────────────────

/// Ids kebab-case de las 6 escenas AM2 (lo que el dispatcher cablea).
pub const TEMPLATE_IDS: &[&str] = &[
    "partial-derivatives",
    "gradient-descent",
    "lagrange-multipliers",
    "double-integral",
    "green-stokes",
    "jacobian",
];

/// ¿El id es una plantilla AM2 (kebab-case exacto)?
pub fn es_plantilla_am2(id: &str) -> bool {
    TEMPLATE_IDS.contains(&id)
}

/// Ficha para la UI: título + descripción (≤200 chars cada uno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ficha {
    /// Id kebab-case.
    pub id: &'static str,
    /// Título corto.
    pub titulo: &'static str,
    /// Qué muestra la escena.
    pub descripcion: &'static str,
}

/// Ficha de la plantilla (`None` honesto si el id no es AM2).
pub fn ficha(id: &str) -> Option<Ficha> {
    match id {
        "partial-derivatives" => Some(Ficha {
            id: "partial-derivatives",
            titulo: "Derivadas parciales y plano tangente",
            descripcion: "Cortes y=cte / x=cte de f(x,y) con sus tangentes y el plano que las une",
        }),
        "gradient-descent" => Some(Ficha {
            id: "gradient-descent",
            titulo: "Gradiente y descenso",
            descripcion: "La partícula sigue -grad L sobre el paisaje de pérdida hasta el mínimo",
        }),
        "lagrange-multipliers" => Some(Ficha {
            id: "lagrange-multipliers",
            titulo: "Multiplicadores de Lagrange",
            descripcion: "La restricción toca la curva de nivel óptima: ahí grad f = λ grad g",
        }),
        "double-integral" => Some(Ficha {
            id: "double-integral",
            titulo: "Integral doble por rebanadas",
            descripcion: "El volumen bajo z=f(x,y) se arma sumando rebanadas A(x)=∫f dy",
        }),
        "green-stokes" => Some(Ficha {
            id: "green-stokes",
            titulo: "Green / Stokes: circulación y flujo",
            descripcion: "La circulación sobre el borde iguala al flujo del rotor sobre la región",
        }),
        "jacobian" => Some(Ficha {
            id: "jacobian",
            titulo: "Jacobiano: deformación de área",
            descripcion: "La grilla polar se deforma y cada celda escala por |J|=r",
        }),
        _ => None,
    }
}

// ── Error ─────────────────────────────────────────────────────────────────

/// Error honesto AM2 (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq)]
pub enum Am2Error {
    /// Valor no finito donde se esperaba número.
    NoFinito(&'static str),
    /// Punto fuera de la ventana `[-3, 3]²`.
    PuntoFueraDeRango { x: f64, y: f64 },
    /// Tasa de aprendizaje fuera de `(0, 0.4]`.
    TasaInvalida(f64),
    /// Radio fuera de `(0, 3]`.
    RadioInvalido(f64),
    /// Ventana `[a, b]` con `a >= b` o fuera de rango.
    VentanaInvalida { a: f64, b: f64 },
    /// Conteo fuera de su tope.
    ConteoInvalido {
        campo: &'static str,
        got: usize,
        max: usize,
    },
    /// Canvas fuera de `64..=4096`.
    CanvasInvalido { w: u32, h: u32 },
    /// Duración fuera de `100..=60000 ms`.
    DuracionInvalida(u64),
    /// Id de plantilla desconocido.
    PlantillaDesconocida(String),
    /// El set RGBA excede el tope (el llamador reduce o parte en chunks).
    SetExcedeTope { bytes: usize, tope: usize },
}

impl Display for Am2Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::NoFinito(campo) => write!(f, "{campo} no es finito: pasame un número"),
            Self::PuntoFueraDeRango { x, y } => write!(
                f,
                "punto ({x}, {y}) fuera de la ventana ±{AM2_VENTANA}: traelo adentro"
            ),
            Self::TasaInvalida(v) => write!(
                f,
                "tasa {v} fuera de (0, 0.4]: bajala o el descenso diverge"
            ),
            Self::RadioInvalido(v) => {
                write!(f, "radio {v} fuera de (0, 3]: usá un círculo visible")
            }
            Self::VentanaInvalida { a, b } => {
                write!(
                    f,
                    "ventana [{a}, {b}] inválida: necesito a < b dentro de ±{AM2_VENTANA}"
                )
            }
            Self::ConteoInvalido { campo, got, max } => {
                write!(f, "{campo} = {got} fuera de 1..={max}: achicalo")
            }
            Self::CanvasInvalido { w, h } => {
                write!(f, "canvas {w}x{h} fuera de 64..=4096 por lado")
            }
            Self::DuracionInvalida(ms) => {
                write!(f, "duración {ms} ms fuera de 100..=60000")
            }
            Self::PlantillaDesconocida(id) => {
                write!(
                    f,
                    "{id:?} no es plantilla AM2: elegí una de {TEMPLATE_IDS:?}"
                )
            }
            Self::SetExcedeTope { bytes, tope } => write!(
                f,
                "set de {bytes} bytes excede el tope de {tope}: bajá resolución o partí en chunks"
            ),
        }
    }
}

impl std::error::Error for Am2Error {}

// ── Matemática compartida (espejos puros) ─────────────────────────────────

/// Guarda finita `0..1` (`NaN`/inf → 0). Pura. Espejo de `anims::clamp01`.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Smoothstep `3t²−2t³` (espejo exacto de `anims::smooth`). Pura y monótona.
pub fn suave(t: f64) -> f64 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

/// Interpolación lineal con guardia finita. Pura.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let t = clamp01(t);
    if !a.is_finite() || !b.is_finite() {
        return 0.0;
    }
    let v = a + (b - a) * t;
    if v.is_finite() {
        v
    } else {
        a
    }
}

/// Exige punto finito dentro de `±AM2_VENTANA`. Pura.
fn valida_punto(x: f64, y: f64) -> Result<[f64; 2], Am2Error> {
    if !x.is_finite() {
        return Err(Am2Error::NoFinito("x"));
    }
    if !y.is_finite() {
        return Err(Am2Error::NoFinito("y"));
    }
    if x.abs() > AM2_VENTANA || y.abs() > AM2_VENTANA {
        return Err(Am2Error::PuntoFueraDeRango { x, y });
    }
    Ok([x, y])
}

/// Exige conteo `1..=max`. Pura.
fn valida_conteo(campo: &'static str, got: usize, max: usize) -> Result<(), Am2Error> {
    if got == 0 || got > max {
        return Err(Am2Error::ConteoInvalido { campo, got, max });
    }
    Ok(())
}

/// Valida canvas `64..=4096` por lado. Pura.
pub fn valida_canvas(w: u32, h: u32) -> Result<(), Am2Error> {
    if !(AM2_CANVAS_MIN..=AM2_CANVAS_MAX).contains(&w)
        || !(AM2_CANVAS_MIN..=AM2_CANVAS_MAX).contains(&h)
    {
        return Err(Am2Error::CanvasInvalido { w, h });
    }
    Ok(())
}

/// Valida duración `100..=60000 ms`. Pura.
pub fn valida_duracion(ms: u64) -> Result<(), Am2Error> {
    if !(AM2_DUR_MIN_MS..=AM2_DUR_MAX_MS).contains(&ms) {
        return Err(Am2Error::DuracionInvalida(ms));
    }
    Ok(())
}

/// Estima los bytes RGBA del set (`w*h*4*frames`). `None` si desborda
/// (`checked`, sin pánicos). Espejo de `protocol::estimate_chunk_bytes`.
pub fn estima_set_bytes(w: usize, h: usize, frames: usize) -> Option<usize> {
    w.checked_mul(h)
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_mul(frames))
}

/// ¿El set `w×h×frames` RGBA entra en `tope`? `None` (desborde) = no entra.
/// Pura.
pub fn cabe_en_tope(w: usize, h: usize, frames: usize, tope: usize) -> bool {
    match estima_set_bytes(w, h, frames) {
        Some(b) => b <= tope,
        None => false,
    }
}

/// Valida el set contra el tope de 64 MiB (`Err` honesto con el número).
pub fn valida_set(w: usize, h: usize, frames: usize) -> Result<(), Am2Error> {
    match estima_set_bytes(w, h, frames) {
        Some(bytes) if bytes <= AM2_TOPE_SET_BYTES => Ok(()),
        Some(bytes) => Err(Am2Error::SetExcedeTope {
            bytes,
            tope: AM2_TOPE_SET_BYTES,
        }),
        None => Err(Am2Error::SetExcedeTope {
            bytes: usize::MAX,
            tope: AM2_TOPE_SET_BYTES,
        }),
    }
}

// ── 1. Derivadas parciales + plano tangente ───────────────────────────────
//
// Demo: paraboloide `f(x, y) = (x² + y²)/4` (GeoGebra "Visualizing Partial
// Derivatives": el corte `y = cte` es una parábola cuya pendiente es `fx`).

/// Paraboloide demo `f(x, y) = (x² + y²)/4`. Pura.
pub fn paraboloide(x: f64, y: f64) -> f64 {
    (x * x + y * y) / 4.0
}

/// Gradiente analítico `(fx, fy) = (x/2, y/2)`. Pura.
pub fn grad_paraboloide(x: f64, y: f64) -> [f64; 2] {
    [x / 2.0, y / 2.0]
}

/// Escena de derivadas parciales: punto de tangencia + barrido en `x`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parciales {
    punto: [f64; 2],
    x_desde: f64,
    x_hasta: f64,
}

impl Parciales {
    /// Constructor validado (punto en `±3`, ventana `a < b` en `±3`).
    pub fn try_new(px: f64, py: f64, x_desde: f64, x_hasta: f64) -> Result<Self, Am2Error> {
        let punto = valida_punto(px, py)?;
        for (nombre, v) in [("x_desde", x_desde), ("x_hasta", x_hasta)] {
            if !v.is_finite() {
                return Err(Am2Error::NoFinito(if nombre == "x_desde" {
                    "x_desde"
                } else {
                    "x_hasta"
                }));
            }
            if v.abs() > AM2_VENTANA {
                return Err(Am2Error::PuntoFueraDeRango { x: v, y: 0.0 });
            }
        }
        if !matches!(x_desde.partial_cmp(&x_hasta), Some(Ordering::Less)) {
            return Err(Am2Error::VentanaInvalida {
                a: x_desde,
                b: x_hasta,
            });
        }
        Ok(Self {
            punto,
            x_desde,
            x_hasta,
        })
    }

    /// Punto de tangencia.
    pub fn punto(self) -> [f64; 2] {
        self.punto
    }

    /// `f` en el punto.
    pub fn altura(self) -> f64 {
        paraboloide(self.punto[0], self.punto[1])
    }

    /// `(fx, fy)` en el punto.
    pub fn pendientes(self) -> [f64; 2] {
        grad_paraboloide(self.punto[0], self.punto[1])
    }

    /// Plano tangente `z = f(p) + fx·(x−px) + fy·(y−py)` evaluado en `(x, y)`.
    /// `None` honesto si la entrada no es finita.
    pub fn plano_en(self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let g = self.pendientes();
        let z = self.altura() + g[0] * (x - self.punto[0]) + g[1] * (y - self.punto[1]);
        if z.is_finite() {
            Some(z)
        } else {
            None
        }
    }

    /// Corte `y = py` (parábola en `x`): `n` puntos en `[x_desde, x_hasta]`.
    /// `Err` si `n` sale de `2..=4096`.
    pub fn corte_x(self, n: usize) -> Result<Vec<[f64; 3]>, Am2Error> {
        valida_conteo("corte_x.n", n, AM2_TOPE_PUNTOS)?;
        if n < 2 {
            return Err(Am2Error::ConteoInvalido {
                campo: "corte_x.n",
                got: n,
                max: AM2_TOPE_PUNTOS,
            });
        }
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let x = self.x_desde + (self.x_hasta - self.x_desde) * (k as f64) / ((n - 1) as f64);
            let z = paraboloide(x, self.punto[1]);
            if z.is_finite() {
                out.push([x, self.punto[1], z]);
            }
        }
        Ok(out)
    }

    /// Cuadro `frame` (`0..AM2_FRAMES`): el punto móvil barre en `x` con
    /// easing suave; devuelve `(posición, fx, fy)` en ese punto.
    pub fn cuadro(self, frame: usize) -> CuadroParciales {
        let t = if AM2_FRAMES <= 1 {
            1.0
        } else {
            clamp01(frame as f64 / (AM2_FRAMES - 1) as f64)
        };
        let x = lerp(self.x_desde, self.x_hasta, suave(t));
        let y = self.punto[1];
        let g = grad_paraboloide(x, y);
        CuadroParciales {
            pos: [x, y, paraboloide(x, y)],
            fx: g[0],
            fy: g[1],
            alpha: t,
        }
    }
}

/// Un frame de la escena de parciales (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroParciales {
    /// Punto móvil sobre la superficie.
    pub pos: [f64; 3],
    /// `fx` ahí (pendiente del corte `y = cte`).
    pub fx: f64,
    /// `fy` ahí (pendiente del corte `x = cte`).
    pub fy: f64,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── 2. Gradiente + descenso ───────────────────────────────────────────────
//
// Paisaje `L(x, y) = x² + 2y²` (bowl anisotrópico: el descenso zigzaguea
// suave como en 3b1b/Li et al.). `∇L = (2x, 4y)`, autovalores 2 y 4, así
// que `lr ≤ 0.4` es estable y converge.

/// Pérdida demo `L(x, y) = x² + 2y²`. Pura.
pub fn perdida(x: f64, y: f64) -> f64 {
    x * x + 2.0 * y * y
}

/// Gradiente analítico `(2x, 4y)`. Pura.
pub fn grad_perdida(x: f64, y: f64) -> [f64; 2] {
    [2.0 * x, 4.0 * y]
}

/// Descenso por gradiente precomputado (un punto por frame).
#[derive(Debug, Clone, PartialEq)]
pub struct Descenso {
    tray: Vec<[f64; 2]>,
    lr: f64,
}

impl Descenso {
    /// Constructor validado: `lr` en `(0, 0.4]`, inicio en `±3`, pasos
    /// `1..=48`. La trayectoria se precomputa acá (acotada, sin allocs en
    /// el muestreo).
    pub fn try_new(x0: f64, y0: f64, lr: f64, pasos: usize) -> Result<Self, Am2Error> {
        let inicio = valida_punto(x0, y0)?;
        if !(lr.is_finite() && 0.0 < lr && lr <= 0.4) {
            return Err(Am2Error::TasaInvalida(lr));
        }
        valida_conteo("descenso.pasos", pasos, AM2_TOPE_PASOS)?;
        let mut tray = Vec::with_capacity(pasos);
        let mut p = inicio;
        tray.push(p);
        for _ in 1..pasos {
            let g = grad_perdida(p[0], p[1]);
            let q = [p[0] - lr * g[0], p[1] - lr * g[1]];
            if !q[0].is_finite() || !q[1].is_finite() {
                break;
            }
            p = q;
            tray.push(p);
        }
        Ok(Self { tray, lr })
    }

    /// Tasa de aprendizaje.
    pub fn tasa(self) -> f64 {
        self.lr
    }

    /// Trayectoria completa (largo = pasos pedidos, salvo corte honesto).
    pub fn trayectoria(&self) -> &[[f64; 2]] {
        &self.tray
    }

    /// Pérdida a lo largo: `tray.len()` valores. Pura (aloja lo justo).
    pub fn perdidas(&self) -> Vec<f64> {
        self.tray.iter().map(|p| perdida(p[0], p[1])).collect()
    }

    /// Posición interpolada en `alpha` crudo `0..1` sobre la trayectoria.
    /// `None` honesto si la trayectoria degeneró a vacía (no debería).
    pub fn pos_en(&self, alpha: f64) -> Option<CuadroDescenso> {
        let n = self.tray.len();
        if n == 0 {
            return None;
        }
        if n == 1 {
            let p = self.tray[0];
            return Some(CuadroDescenso {
                pos: p,
                loss: perdida(p[0], p[1]),
                alpha: clamp01(alpha),
            });
        }
        let t = clamp01(alpha) * (n - 1) as f64;
        let i = (t.floor() as usize).min(n - 2);
        let u = t - i as f64;
        let a = self.tray[i];
        let b = self.tray[i + 1];
        let x = a[0] + (b[0] - a[0]) * u;
        let y = a[1] + (b[1] - a[1]) * u;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        Some(CuadroDescenso {
            pos: [x, y],
            loss: perdida(x, y),
            alpha: clamp01(alpha),
        })
    }
}

/// Un frame del descenso (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroDescenso {
    /// Posición en el plano.
    pub pos: [f64; 2],
    /// `L` ahí.
    pub loss: f64,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── 3. Lagrange: restricción vs curvas de nivel ───────────────────────────
//
// `f(x, y) = x + y` con `g(x, y) = x² + y² = 1`. El nivel `c` barre
// `[-2, 2]`; los extremos `±√2` tocan al círculo en `(±√2/2, ±√2/2)`,
// donde `∇f = (1,1)` es paralelo a `∇g = (2x, 2y)`.

/// Objetivo `f(x, y) = x + y`. Pura.
pub fn lagrange_f(x: f64, y: f64) -> f64 {
    x + y
}

/// Restricción `g(x, y) = x² + y² − 1` (cero sobre el círculo). Pura.
pub fn lagrange_g(x: f64, y: f64) -> f64 {
    x * x + y * y - 1.0
}

/// `∇f = (1, 1)` constante. Pura.
pub fn lagrange_grad_f() -> [f64; 2] {
    [1.0, 1.0]
}

/// `∇g = (2x, 2y)`. Pura.
pub fn lagrange_grad_g(x: f64, y: f64) -> [f64; 2] {
    [2.0 * x, 2.0 * y]
}

/// Escena de Lagrange: barrido del nivel `c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lagrange {
    c_desde: f64,
    c_hasta: f64,
}

impl Lagrange {
    /// Constructor validado (niveles finitos con `a < b`; default `-2..2`).
    pub fn try_new(c_desde: f64, c_hasta: f64) -> Result<Self, Am2Error> {
        if !c_desde.is_finite() {
            return Err(Am2Error::NoFinito("c_desde"));
        }
        if !c_hasta.is_finite() {
            return Err(Am2Error::NoFinito("c_hasta"));
        }
        if !matches!(c_desde.partial_cmp(&c_hasta), Some(Ordering::Less)) {
            return Err(Am2Error::VentanaInvalida {
                a: c_desde,
                b: c_hasta,
            });
        }
        Ok(Self { c_desde, c_hasta })
    }

    /// Nivel en `alpha` crudo (barrido lineal: el renderer lo anima).
    pub fn nivel_en(self, alpha: f64) -> f64 {
        lerp(self.c_desde, self.c_hasta, alpha)
    }

    /// Puntos de tangencia analíticos `(±√2/2, ±√2/2)`. Puros.
    pub fn tangencias() -> [[f64; 2]; 2] {
        let s = std::f64::consts::SQRT_2 / 2.0;
        [[s, s], [-s, -s]]
    }

    /// Niveles extremos `±√2`. Puros.
    pub fn niveles_optimos() -> [f64; 2] {
        [std::f64::consts::SQRT_2, -std::f64::consts::SQRT_2]
    }

    /// ¿`p` es candidato? Chequea las dos condiciones de Lagrange:
    /// sobre la restricción (`|g| ≤ tol`) y gradientes paralelos
    /// (`|∇f × ∇g| ≤ tol`). Pura.
    pub fn es_candidato(p: [f64; 2], tol: f64) -> bool {
        if !p[0].is_finite() || !p[1].is_finite() {
            return false;
        }
        if !tol.is_finite() || tol < 0.0 {
            return false;
        }
        if lagrange_g(p[0], p[1]).abs() > tol {
            return false;
        }
        let gf = lagrange_grad_f();
        let gg = lagrange_grad_g(p[0], p[1]);
        let cruz = gf[0] * gg[1] - gf[1] * gg[0];
        cruz.abs() <= tol
    }

    /// Multiplicador `λ` en un candidato: `∇f = λ∇g` → `λ = 1/(2x)`.
    /// `None` honesto si `x ≈ 0` o no es candidato.
    pub fn lambda_en(p: [f64; 2], tol: f64) -> Option<f64> {
        if !Self::es_candidato(p, tol) {
            return None;
        }
        if p[0].abs() < 1e-12 {
            return None;
        }
        let l = 1.0 / (2.0 * p[0]);
        if l.is_finite() {
            Some(l)
        } else {
            None
        }
    }

    /// Cuadro `frame`: nivel actual + si ya tocó tangencia (`|c| ≥ √2`
    /// del lado que corresponda se marca alcanzado).
    pub fn cuadro(self, frame: usize) -> CuadroLagrange {
        let t = if AM2_FRAMES <= 1 {
            1.0
        } else {
            clamp01(frame as f64 / (AM2_FRAMES - 1) as f64)
        };
        let c = self.nivel_en(t);
        let opt = Self::niveles_optimos();
        CuadroLagrange {
            nivel: c,
            toco_max: c >= opt[0],
            toco_min: c <= opt[1],
            alpha: t,
        }
    }
}

/// Un frame de Lagrange (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroLagrange {
    /// Nivel `c` de la curva `x + y = c`.
    pub nivel: f64,
    /// ¿Ya pasó por `+√2` (máximo)?
    pub toco_max: bool,
    /// ¿Ya pasó por `−√2` (mínimo)?
    pub toco_min: bool,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── 4. Integral doble por rebanadas ───────────────────────────────────────
//
// `f(x, y) = 4 − x² − y²` sobre `R = [−1, 1]²` (Fubini/Strang: el volumen
// es la suma de rebanadas `A(x) = ∫f dy = 22/3 − 2x²`, total `40/3`).

/// Integrando demo `f(x, y) = 4 − x² − y²`. Pura.
pub fn doble_f(x: f64, y: f64) -> f64 {
    4.0 - x * x - y * y
}

/// Escena de integral doble: intervalo `[a, b]` en `x` con `y` fijo en
/// `[-1, 1]` (el rectángulo demo; el renderer lo generaliza después).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DobleIntegral {
    a: f64,
    b: f64,
}

impl DobleIntegral {
    /// Constructor validado (`a < b`, ambos en `±3`).
    pub fn try_new(a: f64, b: f64) -> Result<Self, Am2Error> {
        if !a.is_finite() {
            return Err(Am2Error::NoFinito("a"));
        }
        if !b.is_finite() {
            return Err(Am2Error::NoFinito("b"));
        }
        if a.abs() > AM2_VENTANA
            || b.abs() > AM2_VENTANA
            || !matches!(a.partial_cmp(&b), Some(Ordering::Less))
        {
            return Err(Am2Error::VentanaInvalida { a, b });
        }
        Ok(Self { a, b })
    }

    /// Área de la rebanada `A(x) = ∫_{−1}^{1} f dy = 2(4−x²) − 2/3`.
    /// `None` honesto si `x` no es finita.
    pub fn rebanada_en(x: f64) -> Option<f64> {
        if !x.is_finite() {
            return None;
        }
        let a = 2.0 * (4.0 - x * x) - 2.0 / 3.0;
        if a.is_finite() {
            Some(a)
        } else {
            None
        }
    }

    /// Volumen parcial `V(x0) = ∫_{a}^{x0} A(x) dx` (clamp a `[a, b]`).
    /// `None` honesto si `x0` no es finita.
    pub fn volumen_hasta(self, x0: f64) -> Option<f64> {
        if !x0.is_finite() {
            return None;
        }
        let x = x0.clamp(self.a, self.b);
        // Primitiva: (22/3)x − (2/3)x³.
        let prim = |t: f64| (22.0 / 3.0) * t - (2.0 / 3.0) * t * t * t;
        let v = prim(x) - prim(self.a);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }

    /// Volumen total sobre `[a, b]`.
    pub fn volumen_total(self) -> f64 {
        // `volumen_hasta(b)` con `b` finito nunca es `None`.
        self.volumen_hasta(self.b).unwrap_or(0.0)
    }

    /// Cuadro `frame`: frente de integración barriendo `a → b` con easing.
    pub fn cuadro(self, frame: usize) -> CuadroDoble {
        let t = if AM2_FRAMES <= 1 {
            1.0
        } else {
            clamp01(frame as f64 / (AM2_FRAMES - 1) as f64)
        };
        let x = lerp(self.a, self.b, suave(t));
        let parcial = self.volumen_hasta(x).unwrap_or(0.0);
        CuadroDoble {
            frente_x: x,
            volumen_parcial: parcial,
            volumen_total: self.volumen_total(),
            alpha: t,
        }
    }
}

/// Un frame de la integral doble (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroDoble {
    /// Frente `x0` hasta donde ya se integró.
    pub frente_x: f64,
    /// `V(x0)` acumulado.
    pub volumen_parcial: f64,
    /// `V(b)` de referencia.
    pub volumen_total: f64,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── 5. Green / Stokes: circulación y flujo ────────────────────────────────
//
// Campo rotación `F(x, y) = (−y, x)` sobre el círculo de radio `r`
// (OpenStax §6.4: `∮F·dr = ∬(∂Q/∂x − ∂P/∂y)`; con `P = −y`, `Q = x` el
// rotor es `2` constante y ambos lados dan `2πr²).

/// Campo demo `F(x, y) = (−y, x)`. Pura.
pub fn campo_rot(x: f64, y: f64) -> [f64; 2] {
    [-y, x]
}

/// Rotor `∂Q/∂x − ∂P/∂y = 2` constante. Pura.
pub fn rotor_rot() -> f64 {
    2.0
}

/// Escena de circulación sobre el círculo de radio `r`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GreenStokes {
    r: f64,
}

impl GreenStokes {
    /// Constructor validado (`r` en `(0, 3]`).
    pub fn try_new(r: f64) -> Result<Self, Am2Error> {
        if !(r.is_finite() && 0.0 < r && r <= AM2_VENTANA) {
            return Err(Am2Error::RadioInvalido(r));
        }
        Ok(Self { r })
    }

    /// Radio.
    pub fn radio(self) -> f64 {
        self.r
    }

    /// Punto del borde en `alpha` (`θ = 2π·alpha`, antihorario = positivo).
    pub fn punto_en(self, alpha: f64) -> [f64; 2] {
        let th = clamp01(alpha) * std::f64::consts::TAU;
        let (s, c) = th.sin_cos();
        [self.r * c, self.r * s]
    }

    /// Circulación acumulada `∫₀^θ r² dφ = r²θ` con `θ = 2π·alpha`. Pura.
    pub fn circulacion_hasta(self, alpha: f64) -> f64 {
        self.r * self.r * clamp01(alpha) * std::f64::consts::TAU
    }

    /// Flujo del rotor sobre el disco: `2·πr²` (Green lado derecho). Pura.
    pub fn flujo_total(self) -> f64 {
        rotor_rot() * std::f64::consts::PI * self.r * self.r
    }

    /// Cuadro `frame`: partícula + circulación acumulada + flujo objetivo.
    pub fn cuadro(self, frame: usize) -> CuadroGreen {
        let t = if AM2_FRAMES <= 1 {
            1.0
        } else {
            clamp01(frame as f64 / (AM2_FRAMES - 1) as f64)
        };
        CuadroGreen {
            punto: self.punto_en(t),
            circulacion: self.circulacion_hasta(t),
            flujo: self.flujo_total(),
            alpha: t,
        }
    }
}

/// Un frame de Green/Stokes (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroGreen {
    /// Partícula sobre el borde.
    pub punto: [f64; 2],
    /// Circulación acumulada hasta acá.
    pub circulacion: f64,
    /// Flujo total del rotor (meta: coinciden al cerrar).
    pub flujo: f64,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── 6. Jacobiano: deformación polar de área ───────────────────────────────
//
// `T(r, θ) = (r cosθ, r sinθ)` con `J = r` (OpenStax §5.7 / UT Austin):
// cada celda `dr × dθ` se vuelve un sector de área `≈ r·dr·dθ`.

/// Transformación polar `T(r, θ)`. Entradas no finitas → `None` honesto.
pub fn polar_a_xy(r: f64, th: f64) -> Option<[f64; 2]> {
    if !r.is_finite() || !th.is_finite() {
        return None;
    }
    let (s, c) = th.sin_cos();
    let p = [r * c, r * s];
    if p[0].is_finite() && p[1].is_finite() {
        Some(p)
    } else {
        None
    }
}

/// Jacobiano `|J| = |r|` (área siempre positiva). `None` si no es finito.
pub fn jacobiano_polar(r: f64) -> Option<f64> {
    if !r.is_finite() {
        return None;
    }
    let j = r.abs();
    if j.is_finite() {
        Some(j)
    } else {
        None
    }
}

/// Escena del jacobiano: anillo `r ∈ [r0, r1]` que se revela por ángulo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jacobiano {
    r0: f64,
    r1: f64,
}

impl Jacobiano {
    /// Constructor validado (`0 ≤ r0 < r1 ≤ 3`).
    pub fn try_new(r0: f64, r1: f64) -> Result<Self, Am2Error> {
        if !r0.is_finite() {
            return Err(Am2Error::NoFinito("r0"));
        }
        if !r1.is_finite() {
            return Err(Am2Error::NoFinito("r1"));
        }
        if !(0.0 <= r0 && r0 < r1 && r1 <= AM2_VENTANA) {
            return Err(Am2Error::RadioInvalido(r1));
        }
        Ok(Self { r0, r1 })
    }

    /// Área de una celda deformada `≈ |J|·dr·dθ` en `(r, θ)`.
    /// `None` honesto si algo no es finito o es no positivo.
    pub fn celda_area(r: f64, dr: f64, dth: f64) -> Option<f64> {
        let j = jacobiano_polar(r)?;
        if !dr.is_finite() || !dth.is_finite() || dr <= 0.0 || dth <= 0.0 {
            return None;
        }
        let a = j * dr * dth;
        if a.is_finite() {
            Some(a)
        } else {
            None
        }
    }

    /// Área revelada hasta `alpha`: sector `½(r1²−r0²)·θ` con `θ = 2π·alpha`.
    pub fn area_hasta(self, alpha: f64) -> f64 {
        let th = clamp01(alpha) * std::f64::consts::TAU;
        0.5 * (self.r1 * self.r1 - self.r0 * self.r0) * th
    }

    /// Área total del anillo `π(r1²−r0²)`.
    pub fn area_total(self) -> f64 {
        std::f64::consts::PI * (self.r1 * self.r1 - self.r0 * self.r0)
    }

    /// Grilla deformada `nu × nv` sobre el sector revelado en `alpha`
    /// (`nu`/`nv` en `2..=128`, paridad `SURF3D_MIN/MAX_RES`).
    /// Devuelve los puntos `(r, θ) → (x, y)` en fila-mayor.
    pub fn grilla_hasta(self, alpha: f64, nu: usize, nv: usize) -> Result<Vec<[f64; 2]>, Am2Error> {
        if !(AM2_RES_MIN..=AM2_RES_MAX).contains(&nu) || !(AM2_RES_MIN..=AM2_RES_MAX).contains(&nv)
        {
            return Err(Am2Error::ConteoInvalido {
                campo: "jacobiano.res",
                got: nu.max(nv),
                max: AM2_RES_MAX,
            });
        }
        let th_max = clamp01(alpha) * std::f64::consts::TAU;
        let cols = nu + 1;
        let filas = nv + 1;
        let total = match cols.checked_mul(filas) {
            Some(v) => v,
            None => {
                return Err(Am2Error::ConteoInvalido {
                    campo: "jacobiano.res",
                    got: usize::MAX,
                    max: AM2_RES_MAX,
                });
            }
        };
        let mut out = Vec::with_capacity(total);
        for j in 0..filas {
            for i in 0..cols {
                let r = self.r0 + (self.r1 - self.r0) * (i as f64) / (nu as f64);
                let th = th_max * (j as f64) / (nv as f64);
                match polar_a_xy(r, th) {
                    Some(p) => out.push(p),
                    None => {
                        return Err(Am2Error::NoFinito("grilla polar"));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Cuadro `frame`: ángulo revelado + área acumulada + área total.
    pub fn cuadro(self, frame: usize) -> CuadroJacobiano {
        let t = if AM2_FRAMES <= 1 {
            1.0
        } else {
            clamp01(frame as f64 / (AM2_FRAMES - 1) as f64)
        };
        CuadroJacobiano {
            theta: clamp01(t) * std::f64::consts::TAU,
            area: self.area_hasta(t),
            area_total: self.area_total(),
            alpha: t,
        }
    }
}

/// Un frame del jacobiano (dato puro para el renderer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuadroJacobiano {
    /// Ángulo revelado `θ = 2π·alpha`.
    pub theta: f64,
    /// Área acumulada del sector.
    pub area: f64,
    /// Área total del anillo.
    pub area_total: f64,
    /// Progreso crudo `0..1`.
    pub alpha: f64,
}

// ── Tests inline (mismo gate, sin cableado extra) ──────────────────────────

#[cfg(test)]
mod am2_tests {
    use super::*;

    fn cerca(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn registro_kebab_y_fichas_acotadas() {
        assert_eq!(TEMPLATE_IDS.len(), 6);
        for id in TEMPLATE_IDS {
            assert!(!id.is_empty(), "id vacío");
            assert_eq!(*id, id.to_lowercase(), "kebab-case minúsculas: {id}");
            assert!(!id.contains('_'), "kebab, no snake: {id}");
            assert!(!id.contains(' '), "sin espacios: {id}");
            assert!(es_plantilla_am2(id));
            let f = ficha(id);
            assert!(f.is_some(), "sin ficha: {id}");
            if let Some(ficha) = f {
                assert_eq!(ficha.id, *id);
                assert!(!ficha.titulo.is_empty());
                assert!(ficha.titulo.chars().count() <= AM2_MAX_TITULO_CHARS);
                assert!(ficha.descripcion.chars().count() <= AM2_MAX_TITULO_CHARS);
            }
        }
        assert!(!es_plantilla_am2("lagrange"));
        assert!(!es_plantilla_am2(""));
        assert!(ficha("derivative-slope").is_none());
        // Presupuestos pineados (paridad protocolo + nativo).
        assert_eq!(AM2_FRAMES, 48);
        assert_eq!(AM2_TOPE_CORTO, 64);
        assert_eq!(AM2_TOPE_LARGO, 1500);
        assert_eq!(AM2_TOPE_SET_BYTES, 64 * 1024 * 1024);
        assert_eq!(AM2_LINE_CAP_BYTES, 64 * 1024);
        assert_eq!((AM2_CANVAS_MIN, AM2_CANVAS_MAX), (64, 4096));
        assert_eq!((AM2_DUR_MIN_MS, AM2_DUR_MAX_MS), (100, 60_000));
    }

    #[test]
    fn easing_suave_monotono_y_bordes() {
        assert_eq!(suave(0.0), 0.0);
        assert_eq!(suave(1.0), 1.0);
        assert!(cerca(suave(0.5), 0.5, 1e-12));
        let mut prev = 0.0;
        for k in 0..=100 {
            let v = suave(k as f64 / 100.0);
            assert!(v + 1e-12 >= prev, "retrocede en k={k}");
            prev = v;
        }
        assert!(suave(0.1) < 0.1, "arranque suave");
        assert!(suave(0.9) > 0.9, "llegada suave");
        for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -2.0, 3.0] {
            let v = suave(t);
            assert!(
                v.is_finite() && (0.0..=1.0).contains(&v),
                "suave({t}) = {v}"
            );
        }
        assert_eq!(lerp(2.0, 6.0, 0.25), 3.0);
        assert_eq!(lerp(f64::NAN, 1.0, 0.5), 0.0);
    }

    #[test]
    fn parciales_pendientes_plano_y_cortes() {
        let e = Parciales::try_new(1.5, -1.0, -2.0, 2.0);
        assert!(e.is_ok());
        if let Ok(esc) = e {
            // fx = x/2, fy = y/2 en el punto.
            let g = esc.pendientes();
            assert!(cerca(g[0], 0.75, 1e-12), "fx={}", g[0]);
            assert!(cerca(g[1], -0.5, 1e-12), "fy={}", g[1]);
            // El plano pasa por la superficie en el punto.
            let z_plano = esc.plano_en(1.5, -1.0);
            assert!(z_plano.is_some());
            if let Some(z) = z_plano {
                assert!(cerca(z, esc.altura(), 1e-12));
            }
            // Derivada numérica del corte == fx analítica.
            let h = 1e-5;
            let num = (paraboloide(1.5 + h, -1.0) - paraboloide(1.5 - h, -1.0)) / (2.0 * h);
            assert!(cerca(num, g[0], 1e-7), "num={num} vs fx={}", g[0]);
            // Corte: parábola muestreada, monótona en x.
            let corte = esc.corte_x(65);
            assert!(corte.is_ok());
            if let Ok(pts) = corte {
                assert_eq!(pts.len(), 65);
                for par in pts.windows(2) {
                    assert!(par[1][0] > par[0][0]);
                }
                // Vértice en x=0 con z = py²/4 = 0.25.
                let mut mejor = &pts[0];
                for p in &pts {
                    if p[0].abs() < mejor[0].abs() {
                        mejor = p;
                    }
                }
                assert!(cerca(mejor[2], 0.25, 1e-9), "vértice {mejor:?}");
            }
            // Barrido: 48 cuadros de x_desde a x_hasta.
            let c0 = esc.cuadro(0);
            let c1 = esc.cuadro(AM2_FRAMES - 1);
            assert!(cerca(c0.pos[0], -2.0, 1e-12));
            assert!(cerca(c1.pos[0], 2.0, 1e-12));
            assert!(cerca(c1.fx, 1.0, 1e-12));
        }
        // Bordes honestos.
        assert!(Parciales::try_new(4.0, 0.0, -1.0, 1.0).is_err());
        assert!(Parciales::try_new(0.0, 0.0, 1.0, 1.0).is_err());
        assert!(Parciales::try_new(0.0, 0.0, 1.0, -1.0).is_err());
        assert!(Parciales::try_new(f64::NAN, 0.0, -1.0, 1.0).is_err());
        if let Ok(esc) = Parciales::try_new(0.0, 0.0, -1.0, 1.0) {
            assert!(esc.plano_en(f64::INFINITY, 0.0).is_none());
            assert!(esc.corte_x(1).is_err());
            assert!(esc.corte_x(AM2_TOPE_PUNTOS + 1).is_err());
        }
    }

    #[test]
    fn descenso_baja_monotono_y_converge() {
        let d = Descenso::try_new(-2.2, 1.6, 0.15, AM2_FRAMES);
        assert!(d.is_ok());
        if let Ok(desc) = d {
            assert_eq!(desc.trayectoria().len(), AM2_FRAMES);
            let ps = desc.perdidas();
            assert_eq!(ps.len(), AM2_FRAMES);
            for par in ps.windows(2) {
                assert!(par[1] <= par[0] + 1e-12, "sube: {} -> {}", par[0], par[1]);
            }
            let ultima = ps[ps.len() - 1];
            assert!(ultima < 1e-6, "no converge: {ultima}");
            // El muestreo interpola sin saltos y arranca/termina exacto.
            let ini = desc.pos_en(0.0);
            let fin = desc.pos_en(1.0);
            assert!(ini.is_some() && fin.is_some());
            if let (Some(a), Some(b)) = (ini, fin) {
                assert!(cerca(a.loss, ps[0], 1e-9));
                assert!(cerca(b.loss, ultima, 1e-9));
                assert!(b.loss < a.loss);
            }
            let medio = desc.pos_en(0.5);
            assert!(medio.is_some());
            if let Some(m) = medio {
                assert!(m.loss < ps[0] && m.loss > ultima);
            }
        }
        // Tasa fuera de (0, 0.4] diverge o es inválida: error honesto.
        assert!(Descenso::try_new(1.0, 1.0, 0.0, 8).is_err());
        assert!(Descenso::try_new(1.0, 1.0, 0.41, 8).is_err());
        assert!(Descenso::try_new(1.0, 1.0, f64::NAN, 8).is_err());
        assert!(Descenso::try_new(9.0, 0.0, 0.1, 8).is_err());
        assert!(Descenso::try_new(0.0, 0.0, 0.1, 0).is_err());
        assert!(Descenso::try_new(0.0, 0.0, 0.1, AM2_TOPE_PASOS + 1).is_err());
    }

    #[test]
    fn lagrange_tangencia_y_lambda() {
        let l = Lagrange::try_new(-2.0, 2.0);
        assert!(l.is_ok());
        if let Ok(esc) = l {
            // Barrido lineal de -2 a 2.
            assert!(cerca(esc.nivel_en(0.0), -2.0, 1e-12));
            assert!(cerca(esc.nivel_en(1.0), 2.0, 1e-12));
            assert!(cerca(esc.nivel_en(0.5), 0.0, 1e-12));
            // Niveles óptimos ±√2.
            let opt = Lagrange::niveles_optimos();
            assert!(cerca(opt[0], std::f64::consts::SQRT_2, 1e-12));
            assert!(cerca(opt[1], -std::f64::consts::SQRT_2, 1e-12));
            // Tangencias: candidatas con λ = ±√2/2.
            for p in Lagrange::tangencias() {
                assert!(Lagrange::es_candidato(p, 1e-9), "no candidata: {p:?}");
                assert!(cerca(
                    lagrange_f(p[0], p[1]).abs(),
                    std::f64::consts::SQRT_2,
                    1e-9
                ));
            }
            let lam = Lagrange::lambda_en(Lagrange::tangencias()[0], 1e-9);
            assert!(lam.is_some());
            if let Some(v) = lam {
                assert!(cerca(v, std::f64::consts::SQRT_2 / 2.0, 1e-9), "λ={v}");
            }
            // (1,0) está en el círculo pero los gradientes no son paralelos.
            assert!(!Lagrange::es_candidato([1.0, 0.0], 1e-9));
            // (0,0) ni siquiera está en la restricción.
            assert!(!Lagrange::es_candidato([0.0, 0.0], 1e-9));
            assert!(!Lagrange::es_candidato([f64::NAN, 0.0], 1e-9));
            assert!(Lagrange::lambda_en([1.0, 0.0], 1e-9).is_none());
            // El cuadro marca las tangencias al pasar.
            let c0 = esc.cuadro(0);
            let c1 = esc.cuadro(AM2_FRAMES - 1);
            assert!(!c0.toco_max && c0.toco_min);
            assert!(c1.toco_max && !c1.toco_min);
        }
        assert!(Lagrange::try_new(1.0, 1.0).is_err());
        assert!(Lagrange::try_new(2.0, -2.0).is_err());
        assert!(Lagrange::try_new(f64::INFINITY, 2.0).is_err());
    }

    #[test]
    fn doble_integral_rebanadas_y_volumen() {
        let d = DobleIntegral::try_new(-1.0, 1.0);
        assert!(d.is_ok());
        if let Ok(esc) = d {
            // A(0) = 22/3, A(±1) = 16/3.
            let a0 = DobleIntegral::rebanada_en(0.0);
            assert!(a0.is_some());
            if let Some(v) = a0 {
                assert!(cerca(v, 22.0 / 3.0, 1e-12), "A(0)={v}");
            }
            for x in [-1.0, 1.0] {
                let a = DobleIntegral::rebanada_en(x);
                assert!(a.is_some());
                if let Some(v) = a {
                    assert!(cerca(v, 16.0 / 3.0, 1e-12), "A({x})={v}");
                }
            }
            assert!(DobleIntegral::rebanada_en(f64::NAN).is_none());
            // Volumen total 40/3 ≈ 13.333 (Fubini a mano).
            assert!(
                cerca(esc.volumen_total(), 40.0 / 3.0, 1e-9),
                "V={}",
                esc.volumen_total()
            );
            // Parcial: arranca en 0, termina en el total, monótono.
            let v_ini = esc.volumen_hasta(-1.0);
            let v_fin = esc.volumen_hasta(1.0);
            assert!(v_ini.is_some() && v_fin.is_some());
            if let (Some(a), Some(b)) = (v_ini, v_fin) {
                assert!(cerca(a, 0.0, 1e-12));
                assert!(cerca(b, 40.0 / 3.0, 1e-9));
            }
            let mut prev = 0.0;
            for k in 0..AM2_FRAMES {
                let c = esc.cuadro(k);
                assert!(c.volumen_parcial + 1e-9 >= prev, "baja en k={k}");
                assert!(c.volumen_parcial <= c.volumen_total + 1e-9);
                prev = c.volumen_parcial;
            }
            let ult = esc.cuadro(AM2_FRAMES - 1);
            assert!(cerca(ult.frente_x, 1.0, 1e-12));
            assert!(cerca(ult.volumen_parcial, ult.volumen_total, 1e-9));
            assert!(esc.volumen_hasta(f64::INFINITY).is_none());
        }
        assert!(DobleIntegral::try_new(1.0, 1.0).is_err());
        assert!(DobleIntegral::try_new(1.0, -1.0).is_err());
        assert!(DobleIntegral::try_new(-4.0, 1.0).is_err());
    }

    #[test]
    fn green_circulacion_iguala_flujo() {
        let g = GreenStokes::try_new(1.5);
        assert!(g.is_ok());
        if let Ok(esc) = g {
            assert_eq!(rotor_rot(), 2.0);
            // Green: ∮F·dr (α=1) == ∬2 dA == 2πr².
            let circ = esc.circulacion_hasta(1.0);
            assert!(
                cerca(circ, esc.flujo_total(), 1e-9),
                "{circ} vs {}",
                esc.flujo_total()
            );
            assert!(cerca(circ, 2.0 * std::f64::consts::PI * 2.25, 1e-9));
            // A mitad de vuelta, la mitad.
            assert!(cerca(esc.circulacion_hasta(0.5), circ / 2.0, 1e-12));
            assert_eq!(esc.circulacion_hasta(0.0), 0.0);
            // La partícula cierra el círculo.
            let p0 = esc.punto_en(0.0);
            let p1 = esc.punto_en(1.0);
            assert!(cerca(p0[0], 1.5, 1e-12) && cerca(p0[1], 0.0, 1e-12));
            assert!(cerca(p1[0], p0[0], 1e-9) && cerca(p1[1], p0[1], 1e-9));
            // Cuarto de vuelta: (0, r).
            let p4 = esc.punto_en(0.25);
            assert!(cerca(p4[0], 0.0, 1e-9) && cerca(p4[1], 1.5, 1e-9));
            // El campo es tangente al borde: F·n̂ = 0 sobre el círculo.
            for k in 0..8 {
                let p = esc.punto_en(k as f64 / 8.0);
                let fv = campo_rot(p[0], p[1]);
                let normal = [p[0] / 1.5, p[1] / 1.5];
                let dot = fv[0] * normal[0] + fv[1] * normal[1];
                assert!(dot.abs() < 1e-9, "flujo radial en {p:?}: {dot}");
            }
            let c = esc.cuadro(AM2_FRAMES - 1);
            assert!(cerca(c.circulacion, c.flujo, 1e-9));
        }
        assert!(GreenStokes::try_new(0.0).is_err());
        assert!(GreenStokes::try_new(-1.0).is_err());
        assert!(GreenStokes::try_new(3.1).is_err());
        assert!(GreenStokes::try_new(f64::NAN).is_err());
    }

    #[test]
    fn jacobiano_escala_areas_por_r() {
        let j = Jacobiano::try_new(0.5, 2.0);
        assert!(j.is_ok());
        if let Ok(esc) = j {
            // J = r en varios puntos.
            for r in [0.5, 1.0, 1.7, 2.0] {
                let jj = jacobiano_polar(r);
                assert!(jj.is_some());
                if let Some(v) = jj {
                    assert!(cerca(v, r, 1e-12), "J({r})={v}");
                }
            }
            assert!(jacobiano_polar(f64::NAN).is_none());
            // Celda: |J|·dr·dθ.
            let celda = Jacobiano::celda_area(2.0, 0.1, 0.1);
            assert!(celda.is_some());
            if let Some(v) = celda {
                assert!(cerca(v, 0.02, 1e-12), "celda={v}");
            }
            assert!(Jacobiano::celda_area(1.0, 0.0, 0.1).is_none());
            assert!(Jacobiano::celda_area(1.0, 0.1, -0.1).is_none());
            // Área total del anillo π(4 − 0.25) = 3.75π.
            assert!(cerca(esc.area_total(), 3.75 * std::f64::consts::PI, 1e-9));
            // Revelado: 0 al inicio, total al final, mitad a mitad.
            assert_eq!(esc.area_hasta(0.0), 0.0);
            assert!(cerca(esc.area_hasta(1.0), esc.area_total(), 1e-9));
            assert!(cerca(esc.area_hasta(0.5), esc.area_total() / 2.0, 1e-9));
            // T mapea el círculo r=1, θ=0 → (1, 0).
            let p = polar_a_xy(1.0, 0.0);
            assert!(p.is_some());
            if let Some(q) = p {
                assert!(cerca(q[0], 1.0, 1e-12) && cerca(q[1], 0.0, 1e-12));
            }
            assert!(polar_a_xy(f64::INFINITY, 0.0).is_none());
            // Grilla 9×9: deforma pero conserva el orden radial.
            let grilla = esc.grilla_hasta(1.0, 8, 8);
            assert!(grilla.is_ok());
            if let Ok(pts) = grilla {
                assert_eq!(pts.len(), 81);
                for p in &pts {
                    let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                    assert!((0.5 - 1e-9..=2.0 + 1e-9).contains(&r), "r={r} en {p:?}");
                }
            }
            assert!(esc.grilla_hasta(0.5, 1, 8).is_err());
            assert!(esc.grilla_hasta(0.5, 8, 129).is_err());
        }
        assert!(Jacobiano::try_new(2.0, 0.5).is_err());
        assert!(Jacobiano::try_new(-0.1, 1.0).is_err());
        assert!(Jacobiano::try_new(0.0, 3.1).is_err());
    }

    #[test]
    fn presupuestos_canvas_duracion_y_set() {
        assert!(valida_canvas(640, 480).is_ok());
        assert!(valida_canvas(64, 64).is_ok());
        assert!(valida_canvas(4096, 4096).is_ok());
        assert!(valida_canvas(63, 480).is_err());
        assert!(valida_canvas(640, 4097).is_err());
        assert!(valida_duracion(100).is_ok());
        assert!(valida_duracion(AM2_DUR_DEFAULT_MS).is_ok());
        assert!(valida_duracion(60_000).is_ok());
        assert!(valida_duracion(99).is_err());
        assert!(valida_duracion(60_001).is_err());
        // Set canónico 640×480×48 ≈ 56 MiB: entra; 4096²×48 no.
        assert_eq!(estima_set_bytes(640, 480, 48), Some(58_982_400));
        assert!(cabe_en_tope(640, 480, 48, AM2_TOPE_SET_BYTES));
        assert!(valida_set(640, 480, 48).is_ok());
        assert!(!cabe_en_tope(4096, 4096, 48, AM2_TOPE_SET_BYTES));
        assert!(valida_set(4096, 4096, 48).is_err());
        assert_eq!(estima_set_bytes(usize::MAX, 4, 4), None);
        assert!(!cabe_en_tope(usize::MAX, 4, 4, AM2_TOPE_SET_BYTES));
        // El set AM2 entra en la línea de wire solo si es chico: el tope de
        // 64 KiB es para mensajes, no para frames (documentado, no error).
        assert_eq!(AM2_LINE_CAP_BYTES, 65_536);
    }

    #[test]
    fn errores_hablan_rioplatense_y_no_panican() {
        let e = Am2Error::TasaInvalida(0.9);
        let s = format!("{e}");
        assert!(s.contains("tasa") && s.contains("diverge"));
        let e = Am2Error::PlantillaDesconocida("foo".to_string());
        assert!(format!("{e}").contains("no es plantilla AM2"));
        // Display de todas las variantes sin pánico.
        let todos = [
            Am2Error::NoFinito("x"),
            Am2Error::PuntoFueraDeRango { x: 9.0, y: 0.0 },
            Am2Error::RadioInvalido(-2.0),
            Am2Error::VentanaInvalida { a: 1.0, b: 0.0 },
            Am2Error::ConteoInvalido {
                campo: "n",
                got: 0,
                max: 8,
            },
            Am2Error::CanvasInvalido { w: 8, h: 8 },
            Am2Error::DuracionInvalida(1),
            Am2Error::SetExcedeTope { bytes: 99, tope: 8 },
        ];
        for e in todos {
            assert!(!format!("{e}").is_empty());
        }
    }
}
