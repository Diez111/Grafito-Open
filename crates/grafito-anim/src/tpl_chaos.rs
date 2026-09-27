//! Plantillas caos y fractales por tiempo de escape e integración (nivel 3Blue1Brown).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! nuevas (solo `std` + `crate`). Todo CPU con grillas acotadas por consts;
//! el render productivo vive en `grafito-app/src/anim_native.rs` (lo cablea
//! la coordinación): este módulo solo expone geometría + samplers y
//! [`TEMPLATE_IDS`] en kebab-case.
//!
//! Escenas (ver [`TEMPLATE_IDS`]):
//!
//! - `chaos-mandelbrot-zoom`: zoom profundo al conjunto de Mandelbrot
//!   (`z ← z² + c`) con tiempo de escape + color suave, del plano completo
//!   (`span 3`) al valle de los caballitos de mar
//!   (`[-0.745428, 0.113009]`, `span → 1e-12`). f64 pineado con límite de
//!   zoom honesto ([`ZOOM_SPAN_MIN`]): sin `BigFloat` ni perturbación no se
//!   baja de `1e-13` (ver nota de precisión abajo).
//! - `chaos-lorenz`: atractor de Lorenz (`σ = 10, ρ = 28, β = 8/3`) integrado
//!   con RK4 y rotando sobre el eje `z` con `alpha` (la rotación ES la
//!   animación; proyección ortográfica honesta, ver [`proyecta_orbita`]).
//! - `chaos-bifurcacion-barrido`: el diagrama de la logística
//!   `x ← rx(1-x)` CONSTRUYÉNDOSE por barrido en `r` + telaraña en el `r`
//!   activo y marcadores de Feigenbaum. NO duplica la canónica
//!   `logistic-bifurcation` (diagrama final estático): esta anima la
//!   construcción columna a columna.
//! - `chaos-julia-morph`: Julia `z ← z² + c` con `c` recorriendo la
//!   circunferencia `|c| = 0.7885` (cruza el borde de `M`: conexo ↔ polvo,
//!   dicotomía de Douady–Hubbard) + órbita de una sonda revelada.
//! - `chaos-pendulo-doble`: péndulo doble (`m = L = 1, g = 9.81`) con dos
//!   ramas separadas por [`ChaosParams::sep`] que divergen (sensibilidad a
//!   condiciones iniciales, pineada por test).
//!
//! ## Reúso (dispatcher + `anims.rs` / `textanim.rs` / `protocol.rs`)
//!
//! - El cableado (`lib.rs`, allowlist de `protocol.rs::CANONICAL_TEMPLATES`,
//!   `NATIVE_TEMPLATES`, `anim_ui.rs::PLANTILLAS_COMBO`) lo hace la
//!   coordinación: este módulo solo expone geometría + samplers.
//! - Revelado de trazos con `anims::{Animation, Scratch}`
//!   ([`escena_para`] recibe el `&mut Scratch` del llamador: cero allocs
//!   intermedios por frame).
//! - Títulos con `textanim::Titulo` ([`titulo_para`]); params vivos con
//!   `protocol::scene_param` ([`ChaosParams`]).
//!
//! ## Presupuestos (acotados, todo lo que excede es `Err` honesto)
//!
//! | Recurso | Tope | Paridad |
//! |---|---|---|
//! | Iteraciones de escape | 1..=256 ([`CHAOS_MAX_ITER`]) | tiempo de escape clásico |
//! | Grilla fractal | `nx` 2..=64, `ny` 2..=48, `nx·ny` ≤ 3072 | `line_cap` 64 KiB |
//! | Muestras de trazo/curva | 2..=512 ([`CHAOS_MAX_MUESTRAS`]) | `SCENE_MORPH_MAX_SAMPLES` |
//! | Puntos por polilínea | ≤4096 (chequeado en [`valida_trazo`]) | `MAX_MOBJECT_POINTS` |
//! | Pasos de trayectoria ODE | 2..=4096 ([`CHAOS_MAX_PASOS`]) | player 48 frames |
//! | Barrido de bifurcación | columnas ≤128, muestras ≤64, celdas ≤8192 | grilla calor/onda 6144 |
//! | Transitorio logístico | 0..=512 ([`BIF_MAX_TRANSITORIO`]) | — |
//! | Wire | cada polilínea ≤512 pts (~10 KiB JSON ≪ 64 KiB) | `line_cap` 64 KiB, `MAX_TEX_SVG_BYTES` |
//!
//! Las grillas (`grilla_mandelbrot`, `grilla_julia`, `grilla_barrido`) son el
//! caso que "pide grilla": van acotadas por los tres topes a la vez; el
//! frente compone por frames y jamás arma el `Vec` total en long-form
//! (contrato P0.1 del protocolo). Las trayectorias ODE se submuestrean a
//! ≤512 pts antes de viajar al wire ([`submuestrea`]).
//!
//! ## Precisión del zoom (límite honesto, sin `BigFloat`)
//!
//! `f64` guarda ~15–16 dígitos decimales: el píxel colapsa entre
//! `span ~ 1e-13` y `1e-15` (el centro ya no se distingue del vecino y la
//! imagen se vuelve bloques). Más abajo hace falta doble-doble (~31
//! dígitos, hasta `~1e-27`) o perturbación con órbita de referencia en
//! precisión arbitraria, y este crate no suma dependencias: [`valida_span`]
//! rechaza `span < 1e-13` con `Err` en vez de dibujar bloques. El zoom
//! animado llega a [`ZOOM_SPAN_FINAL`] `= 1e-12`, un orden de magnitud sobre
//! el piso.
//!
//! ## Fuentes (verificadas por búsqueda web en esta sesión)
//!
//! - Zoom profundo y pisos de precisión f64: Mandala "About Mandala"
//!   (<https://mandalart.net/about>: f64 directo hasta `~1e-13`, doble-doble
//!   hasta `~1e-27`, perturbación más abajo); "Deep zoom theory and
//!   practice" (<https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html>);
//!   demo f32-vs-f64 con piso df64 `~1e-13`
//!   (<https://x-gis.github.io/X-GIS/shader-dsl/examples>).
//! - Lorenz clásico `σ = 10, ρ = 28, β = 8/3` y mariposa:
//!   Burkardt "LORENZ_ODE"
//!   (<https://people.math.sc.edu/Burkardt/c_src/lorenz_ode/lorenz_ode.html>),
//!   Wikipedia "Lorenz system"
//!   (<https://en.wikipedia.org/wiki/Lorenz_system>),
//!   PhysSandbox "Lorenz Strange Attractor"
//!   (<https://physandbox.com/math/lorenz-attractor>); Hopf en
//!   `ρ ≈ 24.74` y triestabilidad (Sprott/Xiong,
//!   <https://sprott.physics.wisc.edu/pubs/paper477.pdf>).
//! - Logística y Feigenbaum: `δ ≈ 4.66920`, `r∞ ≈ 3.56994`, cascada
//!   `r₁ = 3` (MathWorld "Logistic Map",
//!   <https://mathworld.wolfram.com/LogisticMap.html>); puntos de
//!   bifurcación `R₁ ~ 3.0, R₂ ~ 3.44949, R₃ ~ 3.54409, R₄ ~ 3.564407`
//!   (notas UNM,
//!   <https://www.cs.unm.edu/~wjust/CS523/S2018/Lectures/Logistic%20Map%201_18_2018.pdf>).
//! - Julia `|c| = 0.7885` y péndulo doble: la búsqueda de esta sesión cubrió
//!   los 4 temas pedidos (Mandelbrot, Lorenz, Feigenbaum, atractores
//!   extraños) y NO devolvió URLs verificables para estos dos, así que no se
//!   cita URL: la construcción es la estándar de manual (tiempo de escape
//!   `z² + c`, dicotomía conexo/polvo según `c ∈ M`, ecuaciones del péndulo
//!   doble con `den = 2m₁ + m₂ − m₂·cos(2θ₁ − 2θ₂)`), pineada por los tests
//!   de abajo con formas cerradas.
//!
//! Sin `unwrap`/`expect` en producción (`unwrap_used = deny`).

use crate::anims::{Animation, Scratch};
use crate::player::{centroide_de, PlacedMobject};
use crate::protocol::scene_param;
use crate::scene::{Mobject, RateFunc, SceneError, SceneResult, MAX_MOBJECT_POINTS};
use crate::textanim::{TextAnimError, Titulo};
use std::collections::BTreeMap;

// ── Registro ─────────────────────────────────────────────────────────────

/// Plantillas de este módulo, en kebab-case para el dispatcher
/// (`anim_native` las cablea; el wire las registra después en
/// `CANONICAL_TEMPLATES`). Ninguna colisiona con las 13 canónicas
/// (`fractal` es el copo de Koch, `logistic-bifurcation` el diagrama final:
/// las de acá son escape-tiempo profundo y barrido constructivo).
pub const TEMPLATE_IDS: &[&str] = &[
    "chaos-mandelbrot-zoom",
    "chaos-lorenz",
    "chaos-bifurcacion-barrido",
    "chaos-julia-morph",
    "chaos-pendulo-doble",
];

/// Título + descripción corta para la UI/preview (sin construir nada).
pub fn describe(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "chaos-mandelbrot-zoom" => Some((
            "Zoom profundo a Mandelbrot",
            "tiempo de escape hasta span 1e-12 en f64 honesto",
        )),
        "chaos-lorenz" => Some((
            "Atractor de Lorenz",
            "la mariposa integrando RK4 y rotando sobre z",
        )),
        "chaos-bifurcacion-barrido" => Some((
            "Bifurcación construyéndose",
            "barrido en r con telaraña y marcadores de Feigenbaum",
        )),
        "chaos-julia-morph" => Some((
            "Julia morphing",
            "c recorre |c| = 0.7885: conexo contra polvo",
        )),
        "chaos-pendulo-doble" => Some((
            "Péndulo doble caótico",
            "dos ramas vecinas que divergen sin perdón",
        )),
        _ => None,
    }
}

/// Título centrado de la escena (reúso `textanim::Titulo`; valida canvas
/// 64..=4096 y fuente 8..=96 px como el resto del crate).
pub fn titulo_para(id: &str, canvas_w: u32, canvas_h: u32) -> Result<Titulo, TextAnimError> {
    match describe(id) {
        Some((titulo, subtitulo)) => {
            Titulo::try_new(titulo, Some(subtitulo), 32.0, canvas_w, canvas_h)
        }
        None => Titulo::try_new(
            "Caos",
            Some("plantilla desconocida"),
            32.0,
            canvas_w,
            canvas_h,
        ),
    }
}

// ── Presupuestos ─────────────────────────────────────────────────────────

/// Iteraciones máximas de tiempo de escape (Mandelbrot/Julia).
pub const CHAOS_MAX_ITER: usize = 256;
/// Columnas máximas de una grilla fractal.
pub const CHAOS_MAX_NX: usize = 64;
/// Filas máximas de una grilla fractal.
pub const CHAOS_MAX_NY: usize = 48;
/// Celdas máximas de una grilla fractal (`64 × 48`).
pub const CHAOS_MAX_CELDAS: usize = CHAOS_MAX_NX * CHAOS_MAX_NY;
/// Muestras máximas por trazo o curva que viaja al wire (paridad
/// `SCENE_MORPH_MAX_SAMPLES`; ~10 KiB JSON ≪ `line_cap` 64 KiB).
pub const CHAOS_MAX_MUESTRAS: usize = 512;
/// Pasos máximos de una trayectoria ODE (Lorenz, péndulo).
pub const CHAOS_MAX_PASOS: usize = 4096;
/// Columnas máximas del barrido de bifurcación.
pub const CHAOS_MAX_COLUMNAS: usize = 128;
/// Muestras del atractor por columna del barrido.
pub const CHAOS_MAX_ATRACTOR: usize = 64;
/// Celdas máximas del barrido (`128 × 64`).
pub const CHAOS_MAX_BARRIDO_CELDAS: usize = CHAOS_MAX_COLUMNAS * CHAOS_MAX_ATRACTOR;
/// Iteraciones transitorias máximas antes de muestrear el atractor.
pub const BIF_MAX_TRANSITORIO: usize = 512;
/// Semiancho del viewport mundo (`[-3, 3]²`, como el resto del crate).
pub const CHAOS_SEMI: f64 = 3.0;

// ── Mandelbrot: zoom profundo ────────────────────────────────────────────

/// Centro del zoom: valle de los caballitos de mar (filamento clásico).
pub const ZOOM_CENTRO: [f64; 2] = [-0.745_428, 0.113_009];
/// Span inicial: el plano `[-2, 1] × [-1.5, 1.5]` entra en `span 3`.
pub const ZOOM_SPAN_INICIAL: f64 = 3.0;
/// Span final del zoom animado (un orden sobre el piso honesto).
pub const ZOOM_SPAN_FINAL: f64 = 1e-12;
/// Piso honesto del `f64`: debajo de `1e-13` el píxel colapsa (hace falta
/// doble-doble o perturbación; sin deps nuevas es `Err`, no bloques).
pub const ZOOM_SPAN_MIN: f64 = 1e-13;
/// Span máximo aceptado (el plano entero con margen).
pub const ZOOM_SPAN_MAX: f64 = 4.0;

// ── Lorenz ───────────────────────────────────────────────────────────────

/// Prandtl clásico (Lorenz 1963).
pub const LORENZ_SIGMA: f64 = 10.0;
/// Rayleigh clásico (régimen mariposa, sobre el Hopf `ρ ≈ 24.74`).
pub const LORENZ_RHO: f64 = 28.0;
/// Factor geométrico clásico.
pub const LORENZ_BETA: f64 = 8.0 / 3.0;
/// Paso default del RK4 (ventana validada en [`ChaosParams::dt_lorenz`]).
pub const LORENZ_DT: f64 = 0.01;
/// Pasos default de la trayectoria dibujada.
pub const LORENZ_PASOS: usize = 1024;

// ── Bifurcación ──────────────────────────────────────────────────────────

/// `r` mínima del barrido (punto fijo estable `x* = 1 − 1/r`).
pub const BIF_R_MIN: f64 = 2.5;
/// `r` máxima del barrido (caos desarrollado).
pub const BIF_R_MAX: f64 = 4.0;
/// `r` mínima de la ventana caótica (`modo 1`: zoom a la zona densa).
pub const BIF_VENTANA_MIN: f64 = 3.4;
/// Primera duplicación (`r₁ = 3` exacto).
pub const BIF_R1: f64 = 3.0;
/// Segunda duplicación (`R₂ ~ 3.44949`, notas UNM).
pub const BIF_R2: f64 = 3.449_489_7;
/// Tercera duplicación (`R₃ ~ 3.54409`, notas UNM).
pub const BIF_R3: f64 = 3.544_090_3;
/// Punto de acumulación (`r∞ ~ 3.5699456`, MathWorld).
pub const BIF_R_INF: f64 = 3.569_945_6;
/// Constante de Feigenbaum (`δ ≈ 4.6692016`, MathWorld).
pub const FEIGENBAUM_DELTA: f64 = 4.669_201_6;
/// Transitorio default antes de muestrear el atractor.
pub const BIF_TRANSITORIO: usize = 200;
/// Muestras default del atractor por columna.
pub const BIF_MUESTRAS: usize = 32;
/// Columnas default del barrido.
pub const BIF_COLUMNAS: usize = 96;

// ── Julia ────────────────────────────────────────────────────────────────

/// Radio del morph: `c(θ) = R·(cos θ, sin θ)` (cruza el borde de `M`).
pub const JULIA_R: f64 = 0.7885;
/// Escala del plano Julia (`|z| ≤ 2`) al mundo `[-3, 3]`.
pub const JULIA_ESCALA: f64 = 1.5;
/// Iteraciones de la órbita sonda dibujada.
pub const JULIA_SONDA_PASOS: usize = 48;

// ── Péndulo doble ────────────────────────────────────────────────────────

/// Masas unitarias (unidades arbitrarias de animación).
pub const PENDULO_M: f64 = 1.0;
/// Varillas unitarias (idem).
pub const PENDULO_L: f64 = 1.0;
/// Gravedad (idem).
pub const PENDULO_G: f64 = 9.81;
/// Paso default del RK4.
pub const PENDULO_DT: f64 = 0.01;
/// Paso máximo aceptado (RK4 explícito: más es inestable).
pub const PENDULO_DT_MAX: f64 = 0.02;
/// Horizonte temporal animado (`t = alpha · T`).
pub const PENDULO_T_MAX: f64 = 10.0;
/// Separación inicial default entre ramas (régimen caótico visible).
pub const PENDULO_SEP: f64 = 1e-6;
/// Denominador mínimo honesto (cerca de la singularidad es `Err`, no NaN).
pub const PENDULO_DEN_MIN: f64 = 1e-9;

// ── Claves vivas del wire ────────────────────────────────────────────────

/// Columnas de la grilla fractal.
pub const CHAOS_PARAM_NX: &str = "nx";
/// Filas de la grilla fractal.
pub const CHAOS_PARAM_NY: &str = "ny";
/// Iteraciones de escape.
pub const CHAOS_PARAM_ITER: &str = "iter";
/// Pasos de trayectoria ODE.
pub const CHAOS_PARAM_PASOS: &str = "pasos";
/// Selector de variante (bif 0 completo / 1 ventana; péndulo 0 dos ramas / 1 una).
pub const CHAOS_PARAM_MODO: &str = "modo";
/// Parámetro `r` de la logística.
pub const CHAOS_PARAM_R: &str = "r";
/// Parte real de `c` (Julia).
pub const CHAOS_PARAM_CRE: &str = "cre";
/// Parte imaginaria de `c` (Julia).
pub const CHAOS_PARAM_CIM: &str = "cim";
/// Paso de integración ODE.
pub const CHAOS_PARAM_DT: &str = "dt";
/// Separación inicial entre ramas del péndulo.
pub const CHAOS_PARAM_SEP: &str = "sep";

// ── Params ───────────────────────────────────────────────────────────────

/// Params vivos de las 5 plantillas (todo `f64` crudo del wire; cada getter
/// valida y convierte con `Err` honesto).
///
/// | Clave | Mandelbrot/Julia | Lorenz | Bifurcación | Péndulo |
/// |---|---|---|---|---|
/// | `nx` | columnas 2..=64 | — | columnas 1..=128 | — |
/// | `ny` | filas 2..=48 | — | — | — |
/// | `iter` | escape 1..=256 | — | — | — |
/// | `pasos` | — | trayectoria 2..=4096 | — | pasos 2..=4096 |
/// | `modo` | — | — | 0 completo / 1 ventana | 0 dos ramas / 1 una |
/// | `r` | — | — | 2.5..=4.0 | — |
/// | `cre/cim` | Julia `|c| ≤ 2` | — | — | — |
/// | `dt` | — | `(0, 0.05]` | — | `(0, 0.02]` |
/// | `sep` | — | — | — | `1e-12..=1e-3` |
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChaosParams {
    /// Columnas de grilla / barrido.
    pub nx: f64,
    /// Filas de la grilla fractal.
    pub ny: f64,
    /// Iteraciones de escape.
    pub iter: f64,
    /// Pasos de trayectoria ODE.
    pub pasos: f64,
    /// Selector de variante.
    pub modo: f64,
    /// Parámetro `r` de la logística.
    pub r: f64,
    /// Parte real de `c` (Julia).
    pub cre: f64,
    /// Parte imaginaria de `c` (Julia).
    pub cim: f64,
    /// Paso de integración.
    pub dt: f64,
    /// Separación inicial entre ramas.
    pub sep: f64,
}

impl ChaosParams {
    /// Defaults por plantilla (lo que se ve sin mover sliders).
    pub fn por_defecto(id: &str) -> Self {
        let base = Self {
            nx: 48.0,
            ny: 36.0,
            iter: 128.0,
            pasos: LORENZ_PASOS as f64,
            modo: 0.0,
            r: 3.2,
            cre: -0.8,
            cim: 0.156,
            dt: LORENZ_DT,
            sep: PENDULO_SEP,
        };
        match id {
            "chaos-bifurcacion-barrido" => Self {
                nx: BIF_COLUMNAS as f64,
                ..base
            },
            "chaos-pendulo-doble" => Self {
                pasos: 1000.0,
                dt: PENDULO_DT,
                ..base
            },
            _ => base,
        }
    }

    /// Mezcla el mapa del wire sobre los defaults (ausente/NaN/inf →
    /// default, vía `protocol::scene_param`; el rango lo valida cada getter).
    pub fn desde_mapa(id: &str, params: &BTreeMap<String, f64>) -> Self {
        let base = Self::por_defecto(id);
        Self {
            nx: scene_param(params, CHAOS_PARAM_NX, base.nx),
            ny: scene_param(params, CHAOS_PARAM_NY, base.ny),
            iter: scene_param(params, CHAOS_PARAM_ITER, base.iter),
            pasos: scene_param(params, CHAOS_PARAM_PASOS, base.pasos),
            modo: scene_param(params, CHAOS_PARAM_MODO, base.modo),
            r: scene_param(params, CHAOS_PARAM_R, base.r),
            cre: scene_param(params, CHAOS_PARAM_CRE, base.cre),
            cim: scene_param(params, CHAOS_PARAM_CIM, base.cim),
            dt: scene_param(params, CHAOS_PARAM_DT, base.dt),
            sep: scene_param(params, CHAOS_PARAM_SEP, base.sep),
        }
    }

    /// Columnas de grilla fractal 2..=64.
    pub fn columnas(&self) -> SceneResult<usize> {
        entero_en("ChaosParams.nx", self.nx, 2, CHAOS_MAX_NX)
    }

    /// Filas de grilla fractal 2..=48.
    pub fn filas(&self) -> SceneResult<usize> {
        entero_en("ChaosParams.ny", self.ny, 2, CHAOS_MAX_NY)
    }

    /// Iteraciones de escape 1..=256.
    pub fn max_iter(&self) -> SceneResult<usize> {
        entero_en("ChaosParams.iter", self.iter, 1, CHAOS_MAX_ITER)
    }

    /// Pasos de trayectoria ODE 2..=4096.
    pub fn pasos_trayectoria(&self) -> SceneResult<usize> {
        entero_en("ChaosParams.pasos", self.pasos, 2, CHAOS_MAX_PASOS)
    }

    /// Columnas del barrido 1..=128.
    pub fn columnas_barrido(&self) -> SceneResult<usize> {
        entero_en("ChaosParams.nx", self.nx, 1, CHAOS_MAX_COLUMNAS)
    }

    /// Paso de Lorenz `(0, 0.05]` finito.
    pub fn dt_lorenz(&self) -> SceneResult<f64> {
        if !self.dt.is_finite() || self.dt <= 0.0 || self.dt > 0.05 {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.dt",
                detalle: format!("dt {} fuera de 0 < dt ≤ 0.05", self.dt),
            });
        }
        Ok(self.dt)
    }

    /// Paso del péndulo `(0, 0.02]` finito (RK4 explícito).
    pub fn dt_pendulo(&self) -> SceneResult<f64> {
        if !self.dt.is_finite() || self.dt <= 0.0 || self.dt > PENDULO_DT_MAX {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.dt",
                detalle: format!("dt {} fuera de 0 < dt ≤ {PENDULO_DT_MAX}", self.dt),
            });
        }
        Ok(self.dt)
    }

    /// `r` de la logística en `2.5..=4.0`.
    pub fn r_val(&self) -> SceneResult<f64> {
        if !self.r.is_finite() || !(BIF_R_MIN..=BIF_R_MAX).contains(&self.r) {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.r",
                detalle: format!("r {} fuera de {BIF_R_MIN}..={BIF_R_MAX}", self.r),
            });
        }
        Ok(self.r)
    }

    /// `c` de Julia con `|c| ≤ 2` (más allá todo escapa: nada que dibujar).
    pub fn c_julia(&self) -> SceneResult<[f64; 2]> {
        if !self.cre.is_finite() || !self.cim.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.c",
                detalle: "c no finita: pasame cre/cim finitas".to_string(),
            });
        }
        if self.cre.abs() > 2.0 || self.cim.abs() > 2.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.c",
                detalle: format!("c = {} + {}i fuera de ±2 (todo escapa)", self.cre, self.cim),
            });
        }
        Ok([self.cre, self.cim])
    }

    /// Separación inicial entre ramas `1e-12..=1e-3`.
    pub fn sep_val(&self) -> SceneResult<f64> {
        if !self.sep.is_finite() || !(1e-12..=1e-3).contains(&self.sep) {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.sep",
                detalle: format!("sep {} fuera de 1e-12..=1e-3", self.sep),
            });
        }
        Ok(self.sep)
    }

    /// Ventana del barrido según `modo`: 0 → `[2.5, 4]`, 1 → `[3.4, 4]`.
    pub fn ventana_barrido(&self) -> SceneResult<(f64, f64)> {
        if !self.modo.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.modo",
                detalle: "modo no finito: pasame 0 o 1".to_string(),
            });
        }
        match self.modo as usize {
            0 => Ok((BIF_R_MIN, BIF_R_MAX)),
            1 => Ok((BIF_VENTANA_MIN, BIF_R_MAX)),
            _ => Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.modo",
                detalle: format!("modo {} inválido: 0 completo, 1 ventana", self.modo),
            }),
        }
    }

    /// Ramas del péndulo según `modo`: 0 → 2, 1 → 1.
    pub fn ramas_pendulo(&self) -> SceneResult<usize> {
        if !self.modo.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.modo",
                detalle: "modo no finito: pasame 0 o 1".to_string(),
            });
        }
        match self.modo as usize {
            0 => Ok(2),
            1 => Ok(1),
            _ => Err(SceneError::MobjectInvalido {
                donde: "ChaosParams.modo",
                detalle: format!("modo {} inválido: 0 dos ramas, 1 una", self.modo),
            }),
        }
    }
}

/// Entero `min..=max` desde un `f64` crudo (no finito o fuera de rango es
/// `Err` honesto; el truncado es hacia cero como el slider vivo).
fn entero_en(donde: &'static str, v: f64, min: usize, max: usize) -> SceneResult<usize> {
    if !v.is_finite() {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: "valor no finito".to_string(),
        });
    }
    let n = v as usize;
    if !(min..=max).contains(&n) {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("{v} fuera de {min}..={max}"),
        });
    }
    Ok(n)
}

// ── Utilidades ───────────────────────────────────────────────────────────

/// Guarda finita 0..1 (`NaN`/inf → 0). Pura.
fn clamp01(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Trazo con 1..=4096 puntos finitos (guarda del wire). Pura en validación.
fn valida_trazo(pts: &[[f64; 2]], donde: &'static str) -> SceneResult<()> {
    if pts.is_empty() || pts.len() > MAX_MOBJECT_POINTS {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("{} puntos (válido 1..={MAX_MOBJECT_POINTS})", pts.len()),
        });
    }
    for (i, p) in pts.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde,
                detalle: format!("punto no finito en el índice {i}"),
            });
        }
    }
    Ok(())
}

/// Submuestreo por stride a `max` puntos (las trayectorias ODE de hasta 4096
/// pasos viajan al wire con ≤512: la forma se conserva, el JSON no).
/// `max < 2` o trazo corto → copia honesta. Pura.
fn submuestrea(trazo: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    if max < 2 || trazo.len() <= max {
        return trazo.to_vec();
    }
    let stride = trazo.len().div_ceil(max);
    let mut out = Vec::with_capacity(trazo.len().div_ceil(stride) + 1);
    let mut i = 0;
    while i < trazo.len() {
        out.push(trazo[i]);
        i += stride;
    }
    if out.last() != trazo.last() {
        out.push(trazo[trazo.len() - 1]);
    }
    out.truncate(max);
    out
}

/// Colocado opaco centrado en su centroide honesto. Puro.
fn colocado(mobject: Mobject, opacity: f32) -> SceneResult<PlacedMobject> {
    let centro = centroide_de(&mobject);
    PlacedMobject::try_new(mobject, opacity, 1.0, centro)
}

/// Revelado de un trazo (`Create` con su propio rate).
fn anim_revela(trazo: Vec<[f64; 2]>, frames: usize, run_ms: u64) -> SceneResult<Animation> {
    Animation::create(trazo, frames, run_ms, RateFunc::Smooth, false)
}

// ── 1. Mandelbrot: tiempo de escape ──────────────────────────────────────

/// Valida un `span` de zoom: `[1e-13, 4]`. Debajo del piso f64 es `Err`
/// honesto (hace falta doble-doble o perturbación, sin deps acá).
pub fn valida_span(span: f64) -> SceneResult<f64> {
    if !span.is_finite() || !(ZOOM_SPAN_MIN..=ZOOM_SPAN_MAX).contains(&span) {
        return Err(SceneError::MobjectInvalido {
            donde: "valida_span",
            detalle: format!(
                "span {span} fuera de {ZOOM_SPAN_MIN}..={ZOOM_SPAN_MAX} (piso f64 honesto)"
            ),
        });
    }
    Ok(span)
}

/// Iteraciones hasta escapar (`|z|² > 4`, índice 1-based) o `max_iter` si el
/// punto queda acotado. `max_iter` 1..=256, `c` finita. Pura.
pub fn mandelbrot_escape(c_re: f64, c_im: f64, max_iter: usize) -> SceneResult<u32> {
    if !(1..=CHAOS_MAX_ITER).contains(&max_iter) {
        return Err(SceneError::MobjectInvalido {
            donde: "mandelbrot_escape",
            detalle: format!("max_iter {max_iter} fuera de 1..={CHAOS_MAX_ITER}"),
        });
    }
    if !c_re.is_finite() || !c_im.is_finite() {
        return Err(SceneError::MobjectInvalido {
            donde: "mandelbrot_escape",
            detalle: "c no finita".to_string(),
        });
    }
    let (mut z_re, mut z_im) = (0.0, 0.0);
    for i in 1..=max_iter {
        let (nr, ni) = (z_re * z_re - z_im * z_im + c_re, 2.0 * z_re * z_im + c_im);
        z_re = nr;
        z_im = ni;
        if z_re * z_re + z_im * z_im > 4.0 {
            return Ok(i as u32);
        }
    }
    Ok(max_iter as u32)
}

/// Conteo suave `μ = i + 1 − log₂(log₂|z|)` (quita las bandas de iteración;
/// acotado a `0..=max_iter`). Para puntos acotados devuelve `max_iter`.
/// Pura.
pub fn escape_suave(iter: u32, max_iter: usize, znorm2: f64) -> f64 {
    let tope = max_iter as f64;
    if iter as usize >= max_iter || !znorm2.is_finite() || znorm2 <= 1.0 {
        return tope;
    }
    let mu = f64::from(iter) + 1.0 - (znorm2.ln() / 2.0f64.ln()).log2();
    if mu.is_finite() {
        mu.clamp(0.0, tope)
    } else {
        tope
    }
}

/// Span del zoom en `alpha` (exponencial honesto: cada frame multiplica por
/// la misma razón, como el vuelo de un explorador de fractales).
/// `alpha` no finita → 0. Pura.
pub fn span_en(alpha: f64) -> f64 {
    let a = clamp01(alpha);
    ZOOM_SPAN_INICIAL * (ZOOM_SPAN_FINAL / ZOOM_SPAN_INICIAL).powf(a)
}

/// Grilla de escape `ny` filas × `nx` columnas sobre `[cx ± span/2]` (aspecto
/// cuadrado; el frente la compone por frames). Topes: `nx` 2..=64, `ny`
/// 2..=48, `nx·ny` ≤ 3072, `max_iter` 1..=256, `span` en el piso f64.
pub fn grilla_mandelbrot(
    cx: f64,
    cy: f64,
    span: f64,
    nx: usize,
    ny: usize,
    max_iter: usize,
) -> SceneResult<Vec<Vec<u32>>> {
    if ![cx, cy].iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_mandelbrot",
            detalle: "centro no finito".to_string(),
        });
    }
    valida_span(span)?;
    if !(2..=CHAOS_MAX_NX).contains(&nx) || !(2..=CHAOS_MAX_NY).contains(&ny) {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla {nx}×{ny} (válido 2..={CHAOS_MAX_NX} × 2..={CHAOS_MAX_NY})"),
        });
    }
    let celdas = nx.saturating_mul(ny);
    if celdas > CHAOS_MAX_CELDAS {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla {nx}×{ny} = {celdas} celdas (tope {CHAOS_MAX_CELDAS})"),
        });
    }
    if !(1..=CHAOS_MAX_ITER).contains(&max_iter) {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_mandelbrot",
            detalle: format!("max_iter {max_iter} fuera de 1..={CHAOS_MAX_ITER}"),
        });
    }
    let mut filas = Vec::with_capacity(ny);
    for j in 0..ny {
        let mut fila = Vec::with_capacity(nx);
        for i in 0..nx {
            let c_re = cx + span * (i as f64 / (nx - 1) as f64 - 0.5);
            let c_im = cy + span * (j as f64 / (ny - 1) as f64 - 0.5);
            fila.push(mandelbrot_escape(c_re, c_im, max_iter)?);
        }
        filas.push(fila);
    }
    Ok(filas)
}

/// Órbita de `c` (hasta 64 iterados, corta al escapar): la estela que dibuja
/// la escena sobre el centro del zoom. Pura en validación.
pub fn orbita_escape(c_re: f64, c_im: f64, pasos: usize) -> SceneResult<Vec<[f64; 2]>> {
    if !c_re.is_finite() || !c_im.is_finite() {
        return Err(SceneError::MobjectInvalido {
            donde: "orbita_escape",
            detalle: "c no finita".to_string(),
        });
    }
    if !(2..=64).contains(&pasos) {
        return Err(SceneError::MobjectInvalido {
            donde: "orbita_escape",
            detalle: format!("pasos {pasos} fuera de 2..=64"),
        });
    }
    let mut pts = Vec::with_capacity(pasos + 1);
    let (mut z_re, mut z_im) = (0.0, 0.0);
    pts.push([0.0, 0.0]);
    for _ in 0..pasos {
        let (nr, ni) = (z_re * z_re - z_im * z_im + c_re, 2.0 * z_re * z_im + c_im);
        if !nr.is_finite() || !ni.is_finite() {
            break;
        }
        z_re = nr;
        z_im = ni;
        pts.push([z_re, z_im]);
        if z_re * z_re + z_im * z_im > 4.0 {
            break;
        }
    }
    valida_trazo(&pts, "orbita_escape")?;
    Ok(pts)
}

// ── 2. Lorenz ────────────────────────────────────────────────────────────

/// Derivada del sistema (`σ(y−x)`, `x(ρ−z) − y`, `xy − βz`). Pura.
pub fn lorenz_derivada(p: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = p;
    [
        LORENZ_SIGMA * (y - x),
        x * (LORENZ_RHO - z) - y,
        x * y - LORENZ_BETA * z,
    ]
}

/// Un paso RK4 clásico. Pura.
pub fn lorenz_paso_rk4(p: [f64; 3], dt: f64) -> [f64; 3] {
    let k1 = lorenz_derivada(p);
    let p2 = [
        p[0] + 0.5 * dt * k1[0],
        p[1] + 0.5 * dt * k1[1],
        p[2] + 0.5 * dt * k1[2],
    ];
    let k2 = lorenz_derivada(p2);
    let p3 = [
        p[0] + 0.5 * dt * k2[0],
        p[1] + 0.5 * dt * k2[1],
        p[2] + 0.5 * dt * k2[2],
    ];
    let k3 = lorenz_derivada(p3);
    let p4 = [p[0] + dt * k3[0], p[1] + dt * k3[1], p[2] + dt * k3[2]];
    let k4 = lorenz_derivada(p4);
    [
        p[0] + dt / 6.0 * (k1[0] + 2.0 * k2[0] + 2.0 * k3[0] + k4[0]),
        p[1] + dt / 6.0 * (k1[1] + 2.0 * k2[1] + 2.0 * k3[1] + k4[1]),
        p[2] + dt / 6.0 * (k1[2] + 2.0 * k2[2] + 2.0 * k3[2] + k4[2]),
    ]
}

/// Punto fijo no trivial `C± = (±√(β(ρ−1)), ±√(β(ρ−1)), ρ−1)`
/// (`signo ≥ 0 → C+`, si no `C−`). Puro.
pub fn punto_fijo_lorenz(signo: f64) -> [f64; 3] {
    let q = (LORENZ_BETA * (LORENZ_RHO - 1.0)).sqrt();
    let s = if signo >= 0.0 { 1.0 } else { -1.0 };
    [s * q, s * q, LORENZ_RHO - 1.0]
}

/// Trayectoria RK4 (`pasos` 2..=4096, `dt` en `(0, 0.05]`, `p0` finito).
/// Corta honesto si deja de ser finita; `< 2` puntos es `Err`.
pub fn trayectoria_lorenz(p0: [f64; 3], pasos: usize, dt: f64) -> SceneResult<Vec<[f64; 3]>> {
    if !p0.iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_lorenz",
            detalle: "p0 no finito".to_string(),
        });
    }
    if !(2..=CHAOS_MAX_PASOS).contains(&pasos) {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_lorenz",
            detalle: format!("pasos {pasos} fuera de 2..={CHAOS_MAX_PASOS}"),
        });
    }
    if !dt.is_finite() || dt <= 0.0 || dt > 0.05 {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_lorenz",
            detalle: format!("dt {dt} fuera de 0 < dt ≤ 0.05"),
        });
    }
    let mut pts = Vec::with_capacity(pasos + 1);
    let mut p = p0;
    pts.push(p);
    for _ in 0..pasos {
        p = lorenz_paso_rk4(p, dt);
        if !p.iter().all(|v| v.is_finite()) {
            break;
        }
        pts.push(p);
    }
    if pts.len() < 2 {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_lorenz",
            detalle: "la trayectoria muere en el primer paso".to_string(),
        });
    }
    Ok(pts)
}

/// Proyección ortográfica con giro sobre `z`: rota `(x, y)` por `ang` y
/// mapea al mundo (`x/10`, `(z−25)/10`: el atractor `x ∈ ±20, z ∈ 0..50`
/// entra en `[-3, 3]`). Pura.
pub fn proyecta_orbita(tray: &[[f64; 3]], ang: f64) -> Vec<[f64; 2]> {
    let a = if ang.is_finite() { ang } else { 0.0 };
    let (c, s) = (a.cos(), a.sin());
    tray.iter()
        .map(|p| {
            let xr = p[0] * c - p[1] * s;
            let x = if xr.is_finite() {
                (xr / 10.0).clamp(-CHAOS_SEMI, CHAOS_SEMI)
            } else {
                0.0
            };
            let y = if p[2].is_finite() {
                ((p[2] - 25.0) / 10.0).clamp(-CHAOS_SEMI, CHAOS_SEMI)
            } else {
                0.0
            };
            [x, y]
        })
        .collect()
}

// ── 3. Bifurcación por barrido ───────────────────────────────────────────

/// La logística `rx(1−x)` (`x, r` finitos; si no, 0 honesto). Pura.
pub fn logistica(x: f64, r: f64) -> f64 {
    if !x.is_finite() || !r.is_finite() {
        return 0.0;
    }
    let v = r * x * (1.0 - x);
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Punto fijo no trivial `x* = 1 − 1/r` (estable para `r < 3`). Pura.
pub fn punto_fijo_logistico(r: f64) -> f64 {
    1.0 - 1.0 / r
}

/// Columna del atractor: `transitorio` iterados desde `x0 = 0.3` y después
/// `muestras` puntos (`transitorio` 0..=512, `muestras` 1..=64,
/// `r` en `2.5..=4`).
///
/// La semilla es `0.3` a propósito: `0.5` es preimagen del fijo `0` en
/// `r = 4` (`0.5 → 1 → 0 → 0…`) y pinearia caos como un cero muerto.
pub fn columna_atractor(r: f64, transitorio: usize, muestras: usize) -> SceneResult<Vec<f64>> {
    if !r.is_finite() || !(BIF_R_MIN..=BIF_R_MAX).contains(&r) {
        return Err(SceneError::MobjectInvalido {
            donde: "columna_atractor",
            detalle: format!("r {r} fuera de {BIF_R_MIN}..={BIF_R_MAX}"),
        });
    }
    if transitorio > BIF_MAX_TRANSITORIO {
        return Err(SceneError::MobjectInvalido {
            donde: "columna_atractor",
            detalle: format!("transitorio {transitorio} fuera de 0..={BIF_MAX_TRANSITORIO}"),
        });
    }
    if !(1..=CHAOS_MAX_ATRACTOR).contains(&muestras) {
        return Err(SceneError::MobjectInvalido {
            donde: "columna_atractor",
            detalle: format!("muestras {muestras} fuera de 1..={CHAOS_MAX_ATRACTOR}"),
        });
    }
    let mut x = 0.3;
    for _ in 0..transitorio {
        x = logistica(x, r);
    }
    let mut out = Vec::with_capacity(muestras);
    for _ in 0..muestras {
        x = logistica(x, r);
        out.push(x);
    }
    Ok(out)
}

/// Barrido completo: `ncols` columnas en `[r0, r1]` (cada una con su
/// transitorio + muestras). Topes: columnas 1..=128, celdas ≤ 8192.
pub fn grilla_barrido(
    r0: f64,
    r1: f64,
    ncols: usize,
    transitorio: usize,
    muestras: usize,
) -> SceneResult<Vec<Vec<f64>>> {
    let ventana_crece = matches!(r0.partial_cmp(&r1), Some(std::cmp::Ordering::Less));
    if ![r0, r1].iter().all(|v| v.is_finite()) || !ventana_crece || r0 < BIF_R_MIN || r1 > BIF_R_MAX
    {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_barrido",
            detalle: format!(
                "ventana [{r0}, {r1}] inválida: necesito {BIF_R_MIN} ≤ r0 < r1 ≤ {BIF_R_MAX}"
            ),
        });
    }
    if !(1..=CHAOS_MAX_COLUMNAS).contains(&ncols) {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("barrido ncols {ncols} fuera de 1..={CHAOS_MAX_COLUMNAS}"),
        });
    }
    let celdas = ncols.saturating_mul(muestras);
    if celdas > CHAOS_MAX_BARRIDO_CELDAS {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!(
                "barrido {ncols}×{muestras} = {celdas} celdas (tope {CHAOS_MAX_BARRIDO_CELDAS})"
            ),
        });
    }
    let mut out = Vec::with_capacity(ncols);
    for k in 0..ncols {
        let r = if ncols == 1 {
            r0
        } else {
            r0 + (r1 - r0) * k as f64 / (ncols - 1) as f64
        };
        out.push(columna_atractor(r, transitorio, muestras)?);
    }
    Ok(out)
}

/// Telaraña (cobweb) desde `x0`: `(x,x) → (x,f) → (f,f) → …` con `pasos`
/// segmentos (`2..=512`), en el cuadrado unidad. Pura en validación.
pub fn telarana(r: f64, x0: f64, pasos: usize) -> SceneResult<Vec<[f64; 2]>> {
    if !r.is_finite() || !(BIF_R_MIN..=BIF_R_MAX).contains(&r) {
        return Err(SceneError::MobjectInvalido {
            donde: "telarana",
            detalle: format!("r {r} fuera de {BIF_R_MIN}..={BIF_R_MAX}"),
        });
    }
    if !x0.is_finite() || !(0.0..=1.0).contains(&x0) {
        return Err(SceneError::MobjectInvalido {
            donde: "telarana",
            detalle: format!("x0 {x0} fuera de 0..=1"),
        });
    }
    if !(2..=CHAOS_MAX_MUESTRAS).contains(&pasos) {
        return Err(SceneError::MobjectInvalido {
            donde: "telarana",
            detalle: format!("pasos {pasos} fuera de 2..={CHAOS_MAX_MUESTRAS}"),
        });
    }
    let mut pts = Vec::with_capacity(pasos + 1);
    let mut x = x0;
    pts.push([x, x]);
    for _ in 0..pasos {
        let fx = logistica(x, r);
        pts.push([x, fx]);
        pts.push([fx, fx]);
        x = fx;
    }
    pts.truncate(pasos + 1);
    valida_trazo(&pts, "telarana")?;
    Ok(pts)
}

/// Mapea el cuadrado unidad `[0,1]²` al mundo (`±2.4`, deja aire para ejes).
/// Pura.
pub fn unidad_a_mundo(p: [f64; 2]) -> [f64; 2] {
    [-2.4 + 4.8 * p[0], -2.4 + 4.8 * p[1]]
}

// ── 4. Julia morphing ────────────────────────────────────────────────────

/// Tiempo de escape de Julia `z ← z² + c` desde `z0` (misma convención que
/// Mandelbrot: índice 1-based o `max_iter` si queda acotado).
pub fn julia_escape(
    z_re: f64,
    z_im: f64,
    c_re: f64,
    c_im: f64,
    max_iter: usize,
) -> SceneResult<u32> {
    if !(1..=CHAOS_MAX_ITER).contains(&max_iter) {
        return Err(SceneError::MobjectInvalido {
            donde: "julia_escape",
            detalle: format!("max_iter {max_iter} fuera de 1..={CHAOS_MAX_ITER}"),
        });
    }
    if ![z_re, z_im, c_re, c_im].iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "julia_escape",
            detalle: "z0 o c no finita".to_string(),
        });
    }
    let (mut zr, mut zi) = (z_re, z_im);
    for i in 1..=max_iter {
        let (nr, ni) = (zr * zr - zi * zi + c_re, 2.0 * zr * zi + c_im);
        zr = nr;
        zi = ni;
        if zr * zr + zi * zi > 4.0 {
            return Ok(i as u32);
        }
    }
    Ok(max_iter as u32)
}

/// `c` del morph en `alpha`: circunferencia `|c| = 0.7885`
/// (`theta = 2π·alpha`; no finita → 0). Pura.
pub fn julia_c_en(alpha: f64) -> [f64; 2] {
    let a = clamp01(alpha);
    let t = 2.0 * std::f64::consts::PI * a;
    [JULIA_R * t.cos(), JULIA_R * t.sin()]
}

/// Grilla de escape de Julia (`ny × nx`, mismos topes que Mandelbrot).
pub fn grilla_julia(
    c_re: f64,
    c_im: f64,
    span: f64,
    nx: usize,
    ny: usize,
    max_iter: usize,
) -> SceneResult<Vec<Vec<u32>>> {
    if ![c_re, c_im].iter().all(|v| v.is_finite()) || c_re.abs() > 2.0 || c_im.abs() > 2.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_julia",
            detalle: format!("c = {c_re} + {c_im}i fuera de ±2"),
        });
    }
    valida_span(span)?;
    if !(2..=CHAOS_MAX_NX).contains(&nx) || !(2..=CHAOS_MAX_NY).contains(&ny) {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla {nx}×{ny} (válido 2..={CHAOS_MAX_NX} × 2..={CHAOS_MAX_NY})"),
        });
    }
    let celdas = nx.saturating_mul(ny);
    if celdas > CHAOS_MAX_CELDAS {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla {nx}×{ny} = {celdas} celdas (tope {CHAOS_MAX_CELDAS})"),
        });
    }
    if !(1..=CHAOS_MAX_ITER).contains(&max_iter) {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_julia",
            detalle: format!("max_iter {max_iter} fuera de 1..={CHAOS_MAX_ITER}"),
        });
    }
    let mut filas = Vec::with_capacity(ny);
    for j in 0..ny {
        let mut fila = Vec::with_capacity(nx);
        for i in 0..nx {
            let z_re = span * (i as f64 / (nx - 1) as f64 - 0.5);
            let z_im = span * (j as f64 / (ny - 1) as f64 - 0.5);
            fila.push(julia_escape(z_re, z_im, c_re, c_im, max_iter)?);
        }
        filas.push(fila);
    }
    Ok(filas)
}

/// Órbita sonda `z ← z² + c` desde `z0` (hasta `JULIA_SONDA_PASOS`, corta al
/// escapar o al dejar de ser finita). Pura en validación.
pub fn orbita_julia(z0: [f64; 2], c: [f64; 2]) -> SceneResult<Vec<[f64; 2]>> {
    if ![z0[0], z0[1], c[0], c[1]].iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "orbita_julia",
            detalle: "z0 o c no finita".to_string(),
        });
    }
    let mut pts = Vec::with_capacity(JULIA_SONDA_PASOS + 1);
    let (mut zr, mut zi) = (z0[0], z0[1]);
    pts.push([zr * JULIA_ESCALA, zi * JULIA_ESCALA]);
    for _ in 0..JULIA_SONDA_PASOS {
        let (nr, ni) = (zr * zr - zi * zi + c[0], 2.0 * zr * zi + c[1]);
        if !nr.is_finite() || !ni.is_finite() {
            break;
        }
        zr = nr;
        zi = ni;
        pts.push([zr * JULIA_ESCALA, zi * JULIA_ESCALA]);
        if zr * zr + zi * zi > 4.0 {
            break;
        }
    }
    valida_trazo(&pts, "orbita_julia")?;
    Ok(pts)
}

// ── 5. Péndulo doble ─────────────────────────────────────────────────────

/// Estado `[θ₁, ω₁, θ₂, ω₂]` (ángulos desde la vertical inferior).
pub type EstadoPendulo = [f64; 4];

/// Derivada del péndulo doble unitario (convención myphysicslab: denominador
/// `2m₁ + m₂ − m₂·cos(2θ₁ − 2θ₂)`). Cerca de la singularidad es `Err`
/// honesto, jamás NaN silencioso.
pub fn pendulo_derivada(e: EstadoPendulo) -> SceneResult<EstadoPendulo> {
    if !e.iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "pendulo_derivada",
            detalle: "estado no finito".to_string(),
        });
    }
    let [t1, w1, t2, w2] = e;
    let (m1, m2, l1, l2, g) = (PENDULO_M, PENDULO_M, PENDULO_L, PENDULO_L, PENDULO_G);
    let den = 2.0 * m1 + m2 - m2 * (2.0 * t1 - 2.0 * t2).cos();
    if !den.is_finite() || den.abs() < PENDULO_DEN_MIN {
        return Err(SceneError::MobjectInvalido {
            donde: "pendulo_derivada",
            detalle: format!("denominador {den} singular: configuración degenerada"),
        });
    }
    let d = t1 - t2;
    let a1 = (-g * (2.0 * m1 + m2) * t1.sin()
        - m2 * g * (t1 - 2.0 * t2).sin()
        - 2.0 * d.sin() * m2 * (w2 * w2 * l2 + w1 * w1 * l1 * d.cos()))
        / (l1 * den);
    let a2 = (2.0
        * d.sin()
        * (w1 * w1 * l1 * (m1 + m2) + g * (m1 + m2) * t1.cos() + w2 * w2 * l2 * m2 * d.cos()))
        / (l2 * den);
    let out = [w1, a1, w2, a2];
    if !out.iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "pendulo_derivada",
            detalle: "derivada no finita: energía desbocada".to_string(),
        });
    }
    Ok(out)
}

/// Un paso RK4 sobre el estado. Propaga el `Err` singular honesto.
pub fn pendulo_paso_rk4(e: EstadoPendulo, dt: f64) -> SceneResult<EstadoPendulo> {
    if !dt.is_finite() || dt <= 0.0 || dt > PENDULO_DT_MAX {
        return Err(SceneError::MobjectInvalido {
            donde: "pendulo_paso_rk4",
            detalle: format!("dt {dt} fuera de 0 < dt ≤ {PENDULO_DT_MAX}"),
        });
    }
    let k1 = pendulo_derivada(e)?;
    let e2 = [
        e[0] + 0.5 * dt * k1[0],
        e[1] + 0.5 * dt * k1[1],
        e[2] + 0.5 * dt * k1[2],
        e[3] + 0.5 * dt * k1[3],
    ];
    let k2 = pendulo_derivada(e2)?;
    let e3 = [
        e[0] + 0.5 * dt * k2[0],
        e[1] + 0.5 * dt * k2[1],
        e[2] + 0.5 * dt * k2[2],
        e[3] + 0.5 * dt * k2[3],
    ];
    let k3 = pendulo_derivada(e3)?;
    let e4 = [
        e[0] + dt * k3[0],
        e[1] + dt * k3[1],
        e[2] + dt * k3[2],
        e[3] + dt * k3[3],
    ];
    let k4 = pendulo_derivada(e4)?;
    let out = [
        e[0] + dt / 6.0 * (k1[0] + 2.0 * k2[0] + 2.0 * k3[0] + k4[0]),
        e[1] + dt / 6.0 * (k1[1] + 2.0 * k2[1] + 2.0 * k3[1] + k4[1]),
        e[2] + dt / 6.0 * (k1[2] + 2.0 * k2[2] + 2.0 * k3[2] + k4[2]),
        e[3] + dt / 6.0 * (k1[3] + 2.0 * k2[3] + 2.0 * k3[3] + k4[3]),
    ];
    if !out.iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "pendulo_paso_rk4",
            detalle: "paso no finito: bajá el dt".to_string(),
        });
    }
    Ok(out)
}

/// Posiciones `[x₁, y₁, x₂, y₂]` con pivote en el origen (`y` hacia arriba).
/// Pura.
pub fn posiciones_pendulo(t1: f64, t2: f64) -> [f64; 4] {
    let (x1, y1) = (PENDULO_L * t1.sin(), -PENDULO_L * t1.cos());
    let (x2, y2) = (x1 + PENDULO_L * t2.sin(), y1 - PENDULO_L * t2.cos());
    [x1, y1, x2, y2]
}

/// Energía mecánica (cinética + potencial con `y` hacia arriba). Pura.
pub fn energia_pendulo(e: EstadoPendulo) -> f64 {
    let [t1, w1, t2, w2] = e;
    let v1_2 = PENDULO_L * PENDULO_L * w1 * w1;
    let v2_2 = v1_2
        + PENDULO_L * PENDULO_L * w2 * w2
        + 2.0 * PENDULO_L * PENDULO_L * w1 * w2 * (t1 - t2).cos();
    let p = posiciones_pendulo(t1, t2);
    0.5 * PENDULO_M * v1_2
        + 0.5 * PENDULO_M * v2_2
        + PENDULO_M * PENDULO_G * p[1]
        + PENDULO_M * PENDULO_G * p[3]
}

/// Trayectoria de `pasos` estados (`2..=4096`); corta honesto en la primera
/// singularidad si ya hay `≥ 2` estados, si no es `Err`.
pub fn trayectoria_pendulo(
    e0: EstadoPendulo,
    pasos: usize,
    dt: f64,
) -> SceneResult<Vec<EstadoPendulo>> {
    if !e0.iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_pendulo",
            detalle: "e0 no finito".to_string(),
        });
    }
    if !(2..=CHAOS_MAX_PASOS).contains(&pasos) {
        return Err(SceneError::MobjectInvalido {
            donde: "trayectoria_pendulo",
            detalle: format!("pasos {pasos} fuera de 2..={CHAOS_MAX_PASOS}"),
        });
    }
    let mut out = Vec::with_capacity(pasos);
    let mut e = e0;
    out.push(e);
    for _ in 1..pasos {
        match pendulo_paso_rk4(e, dt) {
            Ok(siguiente) => {
                e = siguiente;
                out.push(e);
            }
            Err(_) if out.len() >= 2 => break,
            Err(primero) => return Err(primero),
        }
    }
    Ok(out)
}

/// Separación euclídea en el espacio de ángulos (mide la divergencia).
/// Pura.
pub fn separacion_angulos(a: EstadoPendulo, b: EstadoPendulo) -> f64 {
    let d = (a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2);
    if d.is_finite() {
        d.sqrt()
    } else {
        0.0
    }
}

/// Estela del segundo eslabón `[x₂, y₂]` submuestreada a ≤512. Pura en
/// validación.
pub fn estela_pendulo(tray: &[EstadoPendulo]) -> SceneResult<Vec<[f64; 2]>> {
    if tray.len() < 2 {
        return Err(SceneError::MobjectInvalido {
            donde: "estela_pendulo",
            detalle: "trayectoria de menos de 2 estados".to_string(),
        });
    }
    let cruda: Vec<[f64; 2]> = tray
        .iter()
        .map(|e| {
            let p = posiciones_pendulo(e[0], e[2]);
            [p[2], p[3]]
        })
        .collect();
    let fina = submuestrea(&cruda, CHAOS_MAX_MUESTRAS);
    valida_trazo(&fina, "estela_pendulo")?;
    Ok(fina)
}

// ── Escena por id (lo que cablea el dispatcher) ──────────────────────────

/// Fotograma `alpha` 0..1 (no finito → 0) de la plantilla `id` con los
/// params vivos. Desconocido → `Err` honesto (jamás placeholder con curva
/// falsa). El `Scratch` lo provee el llamador (reúso `anims.rs`).
pub fn escena_para(
    id: &str,
    params: &ChaosParams,
    alpha: f64,
    scratch: &mut Scratch,
) -> SceneResult<Vec<PlacedMobject>> {
    let a = clamp01(alpha);
    match id {
        "chaos-mandelbrot-zoom" => {
            params.max_iter()?;
            let orbita = orbita_escape(ZOOM_CENTRO[0], ZOOM_CENTRO[1], 64)?;
            let mundo: Vec<[f64; 2]> = orbita
                .iter()
                .map(|p| {
                    [
                        (p[0] * JULIA_ESCALA).clamp(-CHAOS_SEMI, CHAOS_SEMI),
                        (p[1] * JULIA_ESCALA).clamp(-CHAOS_SEMI, CHAOS_SEMI),
                    ]
                })
                .collect();
            valida_trazo(&mundo, "escena_mandelbrot")?;
            let anim = anim_revela(mundo, 24, 2000)?;
            let centro = [
                (ZOOM_CENTRO[0] * JULIA_ESCALA).clamp(-CHAOS_SEMI, CHAOS_SEMI),
                (ZOOM_CENTRO[1] * JULIA_ESCALA).clamp(-CHAOS_SEMI, CHAOS_SEMI),
            ];
            let out = vec![
                colocado(Mobject::Axes, 0.5)?,
                anim.sample_con(a, scratch)?,
                colocado(
                    Mobject::Dot {
                        x: centro[0],
                        y: centro[1],
                    },
                    1.0,
                )?,
            ];
            Ok(out)
        }
        "chaos-lorenz" => {
            let pasos = params.pasos_trayectoria()?;
            let dt = params.dt_lorenz()?;
            let tray = trayectoria_lorenz([1.0, 1.0, 1.0], pasos, dt)?;
            let plano = proyecta_orbita(&tray, a * 2.0 * std::f64::consts::PI);
            let fino = submuestrea(&plano, CHAOS_MAX_MUESTRAS);
            valida_trazo(&fino, "escena_lorenz")?;
            let ultimo = fino[fino.len() - 1];
            let out = vec![
                colocado(Mobject::Axes, 0.5)?,
                colocado(Mobject::Polygon { pts: fino }, 1.0)?,
                colocado(
                    Mobject::Dot {
                        x: ultimo[0],
                        y: ultimo[1],
                    },
                    1.0,
                )?,
            ];
            Ok(out)
        }
        "chaos-bifurcacion-barrido" => {
            let (r0, r1) = params.ventana_barrido()?;
            let ncols = params.columnas_barrido()?;
            let r_activo = r0 + a * (r1 - r0);
            let tela = telarana(r_activo, 0.1, 128)?;
            let tela_mundo: Vec<[f64; 2]> = tela.iter().map(|p| unidad_a_mundo(*p)).collect();
            valida_trazo(&tela_mundo, "escena_bifurcacion")?;
            let anim = anim_revela(tela_mundo, 24, 2000)?;
            let atr = columna_atractor(r_activo, BIF_TRANSITORIO, BIF_MUESTRAS.min(ncols))?;
            let mut out = Vec::with_capacity(atr.len() + 3);
            out.push(colocado(Mobject::Axes, 0.5)?);
            out.push(anim.sample_con(a, scratch)?);
            for x in atr {
                let y = unidad_a_mundo([0.0, x])[1];
                let xr = unidad_a_mundo([(r_activo - r0) / (r1 - r0), 0.0])[0];
                out.push(colocado(Mobject::Dot { x: xr, y }, 1.0)?);
            }
            Ok(out)
        }
        "chaos-julia-morph" => {
            let c = julia_c_en(a);
            params.c_julia()?;
            let sonda = orbita_julia([0.2, 0.3], c)?;
            let anim = anim_revela(sonda, 24, 2000)?;
            let mut circ = Vec::with_capacity(65);
            for i in 0..=64 {
                let t = 2.0 * std::f64::consts::PI * i as f64 / 64.0;
                circ.push([JULIA_R * t.cos(), JULIA_R * t.sin()]);
            }
            valida_trazo(&circ, "escena_julia")?;
            let out = vec![
                colocado(Mobject::Axes, 0.5)?,
                colocado(Mobject::Polygon { pts: circ }, 0.45)?,
                anim.sample_con(a, scratch)?,
                colocado(Mobject::Dot { x: c[0], y: c[1] }, 1.0)?,
            ];
            Ok(out)
        }
        "chaos-pendulo-doble" => {
            let pasos = params.pasos_trayectoria()?;
            let dt = params.dt_pendulo()?;
            let sep = params.sep_val()?;
            let ramas = params.ramas_pendulo()?;
            let hasta = ((a * pasos as f64).ceil() as usize).min(pasos).max(2);
            let e0: EstadoPendulo = [1.9, 0.0, 2.6, 0.0];
            let tray_a_full = trayectoria_pendulo(e0, pasos, dt)?;
            let tray_a: Vec<EstadoPendulo> = tray_a_full[..hasta.min(tray_a_full.len())].to_vec();
            let ultima_a = tray_a[tray_a.len() - 1];
            let pa = posiciones_pendulo(ultima_a[0], ultima_a[2]);
            let mut out = Vec::with_capacity(6);
            out.push(colocado(Mobject::Axes, 0.4)?);
            let estela_a = estela_pendulo(&tray_a)?;
            let anim_a = anim_revela(estela_a, 24, 2000)?;
            out.push(anim_a.sample_con(a, scratch)?);
            out.push(colocado(
                Mobject::Line {
                    from: [0.0, 0.0],
                    to: [pa[0], pa[1]],
                },
                1.0,
            )?);
            out.push(colocado(
                Mobject::Line {
                    from: [pa[0], pa[1]],
                    to: [pa[2], pa[3]],
                },
                1.0,
            )?);
            if ramas == 2 {
                let e1 = [e0[0] + sep, e0[1], e0[2], e0[3]];
                let tray_b_full = trayectoria_pendulo(e1, pasos, dt)?;
                let tray_b: Vec<EstadoPendulo> =
                    tray_b_full[..hasta.min(tray_b_full.len())].to_vec();
                let estela_b = estela_pendulo(&tray_b)?;
                out.push(colocado(Mobject::Polygon { pts: estela_b }, 0.55)?);
            }
            Ok(out)
        }
        otro => Err(SceneError::EscenaInvalida {
            detalle: format!("plantilla {otro:?} desconocida: elegí una de {TEMPLATE_IDS:?}"),
        }),
    }
}

// ── Tests inline ─────────────────────────────────────────────────────────

#[cfg(test)]
mod chaos_tests {
    use super::*;

    fn debe<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("esperaba Ok, fue Err: {e:?}"),
        }
    }

    fn es_kebab(id: &str) -> bool {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !id.starts_with('-')
            && !id.ends_with('-')
            && !id.contains("--")
    }

    #[test]
    fn template_ids_kebab_unicos_y_descriptos() {
        assert_eq!(TEMPLATE_IDS.len(), 5);
        for (i, a) in TEMPLATE_IDS.iter().enumerate() {
            assert!(es_kebab(a), "no kebab: {a}");
            for b in &TEMPLATE_IDS[i + 1..] {
                assert_ne!(a, b, "duplicado: {a}");
            }
            assert!(describe(a).is_some(), "sin describe: {a}");
            // No duplica `fractal` (Koch) ni `logistic-bifurcation`
            // (diagrama final) ni el resto de canónicas y vecinas.
            for canonica in [
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
                "edo-campo-direcciones",
                "edo-convolucion",
                "edo-laplace",
                "edo-fourier-epiciclos",
                "edo-calor-onda",
            ] {
                assert_ne!(*a, canonica);
            }
        }
        assert!(describe("no-existe").is_none());
        let t = debe(titulo_para("chaos-lorenz", 640, 480));
        let l = t.layout();
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
        assert_eq!(l.offset_y, (480.0 - l.bloque_h) / 2.0);
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(CHAOS_MAX_ITER, 256);
        assert_eq!(CHAOS_MAX_NX, 64);
        assert_eq!(CHAOS_MAX_NY, 48);
        assert_eq!(CHAOS_MAX_CELDAS, 3072);
        assert_eq!(CHAOS_MAX_MUESTRAS, 512);
        assert_eq!(CHAOS_MAX_PASOS, 4096);
        assert_eq!(CHAOS_MAX_COLUMNAS, 128);
        assert_eq!(CHAOS_MAX_ATRACTOR, 64);
        assert_eq!(CHAOS_MAX_BARRIDO_CELDAS, 8192);
        assert_eq!(BIF_MAX_TRANSITORIO, 512);
        assert_eq!((ZOOM_SPAN_MIN, ZOOM_SPAN_MAX), (1e-13, 4.0));
        assert_eq!(ZOOM_SPAN_FINAL, 1e-12);
        assert_eq!(
            (LORENZ_SIGMA, LORENZ_RHO, LORENZ_BETA),
            (10.0, 28.0, 8.0 / 3.0)
        );
        assert_eq!((BIF_R_MIN, BIF_R_MAX), (2.5, 4.0));
        assert!((FEIGENBAUM_DELTA - 4.669_201_6).abs() < 1e-9);
    }

    #[test]
    fn mandelbrot_escape_pineado() {
        // c = 0 queda acotado (el cardioide lo contiene).
        assert_eq!(debe(mandelbrot_escape(0.0, 0.0, 256)), 256);
        // c = −1 orbita 0 → −1 → 0 (período 2, acotado).
        assert_eq!(debe(mandelbrot_escape(-1.0, 0.0, 256)), 256);
        // c = 2 escapa en 2 iterados (0 → 2 → 6).
        assert_eq!(debe(mandelbrot_escape(2.0, 0.0, 256)), 2);
        assert!(mandelbrot_escape(0.0, 0.0, 0).is_err());
        assert!(mandelbrot_escape(0.0, 0.0, 257).is_err());
        assert!(mandelbrot_escape(f64::NAN, 0.0, 64).is_err());
        // Zoom exponencial honesto: arranca en 3, termina en 1e-12.
        assert_eq!(span_en(0.0), ZOOM_SPAN_INICIAL);
        assert!((span_en(1.0) - 1e-12).abs() < 1e-18);
        assert!(span_en(0.5) > span_en(0.6));
        assert_eq!(span_en(f64::NAN), ZOOM_SPAN_INICIAL);
        // Piso f64: debajo de 1e-13 es Err, no bloques.
        assert!(valida_span(1e-13).is_ok());
        assert!(valida_span(1e-14).is_err());
        assert!(valida_span(4.0).is_ok());
        assert!(valida_span(4.1).is_err());
        // Color suave acotado y monótono cerca del borde.
        let s1 = escape_suave(5, 256, 9.0);
        let s2 = escape_suave(6, 256, 9.0);
        assert!(s1 < s2 && s2 <= 256.0);
        assert_eq!(escape_suave(256, 256, 2.0), 256.0);
        // Grilla mínima: el centro (c = 0) queda acotado.
        let g = debe(grilla_mandelbrot(0.0, 0.0, 3.0, 3, 3, 32));
        assert_eq!(g[1][1], 32);
        assert!(grilla_mandelbrot(0.0, 0.0, 1e-14, 4, 4, 32).is_err());
        assert!(grilla_mandelbrot(0.0, 0.0, 3.0, 65, 4, 32).is_err());
        assert!(grilla_mandelbrot(0.0, 0.0, 3.0, 4, 49, 32).is_err());
        // Órbita del centro del zoom: arranca en el origen.
        let o = debe(orbita_escape(ZOOM_CENTRO[0], ZOOM_CENTRO[1], 64));
        assert_eq!(o[0], [0.0, 0.0]);
        assert!(o.len() >= 2);
    }

    #[test]
    fn lorenz_pineado_a_clasico() {
        // C+ es equilibrio: derivada ~ 0.
        let c = punto_fijo_lorenz(1.0);
        assert!((c[0] - 72.0f64.sqrt()).abs() < 1e-12);
        assert_eq!(c[2], 27.0);
        let d = lorenz_derivada(c);
        assert!(d.iter().all(|v| v.abs() < 1e-9), "{d:?}");
        let cm = punto_fijo_lorenz(-1.0);
        assert_eq!(cm[0], -c[0]);
        // RK4 quieto en el equilibrio (un paso no lo mueve).
        let q = lorenz_paso_rk4(c, 0.01);
        assert!((q[0] - c[0]).abs() < 1e-9);
        // Trayectoria acotada en el atractor y sensible a C.I. (t = 30: la
        // divergencia ya saturó al diámetro del atractor).
        let t1 = debe(trayectoria_lorenz([1.0, 1.0, 1.0], 3000, 0.01));
        assert_eq!(t1.len(), 3001);
        for p in &t1 {
            assert!(p.iter().all(|v| v.is_finite() && v.abs() < 100.0));
        }
        let t2 = debe(trayectoria_lorenz([1.0 + 1e-9, 1.0, 1.0], 3000, 0.01));
        let sep = ((t1[3000][0] - t2[3000][0]).powi(2)
            + (t1[3000][1] - t2[3000][1]).powi(2)
            + (t1[3000][2] - t2[3000][2]).powi(2))
        .sqrt();
        assert!(sep > 1e-3, "divergencia caótica: {sep}");
        assert!(trayectoria_lorenz([1.0, 1.0, 1.0], 1, 0.01).is_err());
        assert!(trayectoria_lorenz([1.0, 1.0, 1.0], 4097, 0.01).is_err());
        assert!(trayectoria_lorenz([1.0, 1.0, 1.0], 64, 0.0).is_err());
        assert!(trayectoria_lorenz([f64::INFINITY, 0.0, 0.0], 64, 0.01).is_err());
        // Proyección al mundo: giro 0 achata x/10, z centrado en 25.
        let plano = proyecta_orbita(&[[10.0, 0.0, 25.0]], 0.0);
        assert_eq!(plano, vec![[1.0, 0.0]]);
        let giro = proyecta_orbita(&[[10.0, 0.0, 25.0]], std::f64::consts::FRAC_PI_2);
        assert!(giro[0][0].abs() < 1e-12 && giro[0][1].abs() < 1e-12);
        for p in proyecta_orbita(&t1, 1.0) {
            assert!(p[0].abs() <= 3.0 && p[1].abs() <= 3.0);
        }
    }

    #[test]
    fn bifurcacion_pineada_a_formas_cerradas() {
        // r = 2.5: punto fijo 1 − 1/r = 0.6.
        let col = debe(columna_atractor(2.5, 200, 8));
        for x in &col {
            assert!((x - 0.6).abs() < 1e-6, "{x}");
        }
        assert!((punto_fijo_logistico(2.5) - 0.6).abs() < 1e-12);
        // r = 3.2: 2-ciclo {(4.2 ± √0.84)/6.4}.
        let col = debe(columna_atractor(3.2, 300, 8));
        let (a, b) = (0.513_044_9, 0.799_455_9);
        for par in col.chunks(2) {
            assert_eq!(par.len(), 2);
            let (x, y) = (par[0], par[1]);
            let ok = (x - a).abs() < 1e-3 && (y - b).abs() < 1e-3
                || (x - b).abs() < 1e-3 && (y - a).abs() < 1e-3;
            assert!(ok, "{x} {y}");
        }
        // r = 4: caos desarrollado (la columna cubre más de medio intervalo).
        let col = debe(columna_atractor(4.0, 200, 64));
        let (mn, mx) = col
            .iter()
            .fold((1.0f64, 0.0f64), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(mx - mn > 0.5, "{mn} {mx}");
        assert!(columna_atractor(2.4, 10, 4).is_err());
        assert!(columna_atractor(3.0, 513, 4).is_err());
        assert!(columna_atractor(3.0, 10, 65).is_err());
        // Razón de Feigenbaum con los tres primeros: (R₂−R₁)/(R₃−R₂) ≈ δ.
        let delta = (BIF_R2 - BIF_R1) / (BIF_R3 - BIF_R2);
        assert!((delta - FEIGENBAUM_DELTA).abs() < 0.3, "{delta}");
        // Telaraña en punto fijo: converge a 0.6 (último tramo quieto).
        let tela = debe(telarana(2.5, 0.1, 64));
        let fin = tela[tela.len() - 1];
        assert!((fin[0] - 0.6).abs() < 1e-3 && (fin[1] - 0.6).abs() < 1e-3);
        assert!(telarana(2.5, 0.1, 1).is_err());
        assert!(telarana(2.5, 2.0, 8).is_err());
        // Barrido acotado por tres topes.
        let g = debe(grilla_barrido(2.5, 4.0, 8, 50, 8));
        assert_eq!((g.len(), g[0].len()), (8, 8));
        let g = debe(grilla_barrido(2.5, 4.0, 128, 64, 64));
        assert_eq!(g.len() * g[0].len(), 8192);
        assert!(grilla_barrido(2.5, 4.0, 129, 10, 8).is_err());
        assert!(grilla_barrido(4.0, 2.5, 8, 10, 8).is_err());
        assert_eq!(unidad_a_mundo([0.0, 0.0]), [-2.4, -2.4]);
        assert_eq!(unidad_a_mundo([1.0, 1.0]), [2.4, 2.4]);
    }

    #[test]
    fn julia_pineada_a_simetrias() {
        // c = 0: dentro del disco unidad queda acotado, fuera escapa ya.
        assert_eq!(debe(julia_escape(0.0, 0.0, 0.0, 0.0, 64)), 64);
        assert_eq!(debe(julia_escape(2.0, 0.0, 0.0, 0.0, 64)), 1);
        // Simetría z → −z (el mapa es par).
        for (zr, zi) in [(0.3, 0.4), (-0.7, 0.2), (1.2, -0.5)] {
            let (a, b) = (
                debe(julia_escape(zr, zi, -0.8, 0.156, 64)),
                debe(julia_escape(-zr, -zi, -0.8, 0.156, 64)),
            );
            assert_eq!(a, b, "{zr} {zi}");
        }
        assert!(julia_escape(0.0, 0.0, 0.0, 0.0, 0).is_err());
        assert!(julia_escape(f64::NAN, 0.0, 0.0, 0.0, 8).is_err());
        // El morph recorre la circunferencia entera y cierra.
        let c0 = julia_c_en(0.0);
        assert!((c0[0] - JULIA_R).abs() < 1e-12 && c0[1].abs() < 1e-12);
        let c1 = julia_c_en(1.0);
        assert!((c1[0] - c0[0]).abs() < 1e-12 && (c1[1] - c0[1]).abs() < 1e-12);
        let cm = julia_c_en(0.25);
        assert!(cm[0].abs() < 1e-12 && (cm[1] - JULIA_R).abs() < 1e-12);
        for a in [0.0, 0.3, 0.7, 1.0] {
            let c = julia_c_en(a);
            assert!((c[0] * c[0] + c[1] * c[1] - JULIA_R * JULIA_R).abs() < 1e-12);
        }
        // Grilla centrada: con c = 0 el centro queda acotado.
        let g = debe(grilla_julia(0.0, 0.0, 3.0, 3, 3, 32));
        assert_eq!(g[1][1], 32);
        assert!(grilla_julia(3.0, 0.0, 3.0, 4, 4, 32).is_err());
        assert!(grilla_julia(0.0, 0.0, 1e-14, 4, 4, 32).is_err());
        // Sonda: arranca en z0 escalado al mundo.
        let o = debe(orbita_julia([0.2, 0.3], [-0.8, 0.156]));
        assert_eq!(o[0], [0.2 * JULIA_ESCALA, 0.3 * JULIA_ESCALA]);
        assert!(o.len() >= 2);
    }

    #[test]
    fn pendulo_conserva_y_diverge() {
        // Colgado quieto: derivada ~ 0.
        let d = debe(pendulo_derivada([0.0, 0.0, 0.0, 0.0]));
        assert!(d.iter().all(|v| v.abs() < 1e-9), "{d:?}");
        // Energía casi conservada (RK4, dt chico, t = 2).
        let e0: EstadoPendulo = [1.0, 0.5, 0.5, -0.3];
        let energia0 = energia_pendulo(e0);
        let mut e = e0;
        for _ in 0..400 {
            e = debe(pendulo_paso_rk4(e, 0.005));
        }
        let deriva = ((energia_pendulo(e) - energia0) / energia0).abs();
        assert!(deriva < 0.02, "{deriva}");
        // Dos ramas vecinas (1e-9) en régimen caótico divergen.
        let a0: EstadoPendulo = [1.9, 0.0, 2.6, 0.0];
        let b0: EstadoPendulo = [1.9 + 1e-9, 0.0, 2.6, 0.0];
        let ta = debe(trayectoria_pendulo(a0, 1500, 0.01));
        let tb = debe(trayectoria_pendulo(b0, 1500, 0.01));
        let sep = separacion_angulos(ta[1499], tb[1499]);
        assert!(sep > 1e-3, "divergencia: {sep}");
        assert!(separacion_angulos(a0, b0) < 1e-8);
        assert!(pendulo_paso_rk4(e0, 0.0).is_err());
        assert!(pendulo_paso_rk4(e0, 0.03).is_err());
        assert!(trayectoria_pendulo(e0, 1, 0.01).is_err());
        // Cinemática: colgado recto pone los eslabones en (0,−1) y (0,−2).
        let p = posiciones_pendulo(0.0, 0.0);
        assert_eq!(p, [0.0, -1.0, 0.0, -2.0]);
        // Estela acotada al wire aunque la trayectoria sea máxima.
        let larga = debe(trayectoria_pendulo(a0, 4096, 0.01));
        let estela = debe(estela_pendulo(&larga));
        assert!(estela.len() <= CHAOS_MAX_MUESTRAS);
        assert!(estela_pendulo(&larga[..1]).is_err());
    }

    #[test]
    fn params_vivos_mezclan_y_validan() {
        let vacio = BTreeMap::new();
        let base = ChaosParams::desde_mapa("chaos-mandelbrot-zoom", &vacio);
        assert_eq!(debe(base.max_iter()), 128);
        assert_eq!(debe(base.columnas()), 48);
        assert_eq!(debe(base.filas()), 36);
        let mut mapa = BTreeMap::new();
        mapa.insert(CHAOS_PARAM_ITER.to_string(), 64.0);
        mapa.insert(CHAOS_PARAM_PASOS.to_string(), 512.0);
        mapa.insert(CHAOS_PARAM_R.to_string(), 3.5);
        mapa.insert(CHAOS_PARAM_DT.to_string(), f64::NAN);
        let p = ChaosParams::desde_mapa("chaos-lorenz", &mapa);
        assert_eq!(debe(p.max_iter()), 64);
        assert_eq!(debe(p.pasos_trayectoria()), 512);
        assert_eq!(debe(p.r_val()), 3.5);
        assert_eq!(debe(p.dt_lorenz()), LORENZ_DT);
        assert_eq!(debe(p.dt_pendulo()), PENDULO_DT);
        assert_eq!(debe(p.sep_val()), PENDULO_SEP);
        assert_eq!(debe(p.ventana_barrido()), (BIF_R_MIN, BIF_R_MAX));
        assert_eq!(debe(p.ramas_pendulo()), 2);
        assert_eq!(debe(p.c_julia()), [-0.8, 0.156]);
        mapa.insert(CHAOS_PARAM_ITER.to_string(), 999.0);
        assert!(ChaosParams::desde_mapa("x", &mapa).max_iter().is_err());
        mapa.insert(CHAOS_PARAM_R.to_string(), 9.0);
        assert!(ChaosParams::desde_mapa("x", &mapa).r_val().is_err());
        let mut mala = ChaosParams::por_defecto("chaos-bifurcacion-barrido");
        mala.modo = 1.0;
        assert_eq!(debe(mala.ventana_barrido()), (BIF_VENTANA_MIN, BIF_R_MAX));
        mala.modo = 1.0;
        assert_eq!(debe(mala.ramas_pendulo()), 1);
        mala.modo = 2.0;
        assert!(mala.ventana_barrido().is_err());
        assert!(mala.ramas_pendulo().is_err());
        mala.cre = 9.0;
        assert!(mala.c_julia().is_err());
        mala.sep = 1.0;
        assert!(mala.sep_val().is_err());
        let bif = ChaosParams::por_defecto("chaos-bifurcacion-barrido");
        assert_eq!(debe(bif.columnas_barrido()), BIF_COLUMNAS);
    }

    #[test]
    fn escena_para_cubre_las_cinco_y_rechaza_resto() {
        let mut scratch = Scratch::nuevo();
        for id in TEMPLATE_IDS {
            let params = ChaosParams::por_defecto(id);
            let frame = debe(escena_para(id, &params, 0.5, &mut scratch));
            assert!(!frame.is_empty(), "{id} sin objetos");
            for colocado in &frame {
                match colocado.mobject.validate() {
                    Ok(()) => {}
                    Err(e) => panic!("{id}: mobject inválido: {e:?}"),
                }
                assert!((0.0..=1.0).contains(&colocado.opacity));
            }
            // alpha no finita no paniquea ni inventa: equivale a 0.
            let cero = debe(escena_para(id, &params, 0.0, &mut scratch));
            let nan = debe(escena_para(id, &params, f64::NAN, &mut scratch));
            assert_eq!(cero.len(), nan.len(), "{id}");
        }
        let params = ChaosParams::por_defecto("chaos-lorenz");
        assert!(escena_para("no-existe", &params, 0.5, &mut scratch).is_err());
        // Péndulo de una rama trae 4 objetos (ejes, estela, 2 varillas).
        let mut una = ChaosParams::por_defecto("chaos-pendulo-doble");
        una.modo = 1.0;
        let frame = debe(escena_para("chaos-pendulo-doble", &una, 0.5, &mut scratch));
        assert_eq!(frame.len(), 4);
    }

    #[test]
    fn tiempos_linea_base() {
        // Límites generosos (CI cargado + llvm-cov 2-5x): las ops reales van
        // en ms de un dígito; se pinean para cazar regresiones gordas.
        let inicio = std::time::Instant::now();
        let g = debe(grilla_mandelbrot(
            ZOOM_CENTRO[0],
            ZOOM_CENTRO[1],
            0.5,
            CHAOS_MAX_NX,
            CHAOS_MAX_NY,
            CHAOS_MAX_ITER,
        ));
        assert_eq!((g.len(), g[0].len()), (CHAOS_MAX_NY, CHAOS_MAX_NX));
        let ms = inicio.elapsed().as_millis();
        println!("mandelbrot {CHAOS_MAX_NX}x{CHAOS_MAX_NY}@256: {ms}ms");
        assert!(ms < 1500, "{ms}ms");

        let inicio = std::time::Instant::now();
        let g = debe(grilla_julia(
            -0.8,
            0.156,
            3.0,
            CHAOS_MAX_NX,
            CHAOS_MAX_NY,
            CHAOS_MAX_ITER,
        ));
        assert_eq!((g.len(), g[0].len()), (CHAOS_MAX_NY, CHAOS_MAX_NX));
        let ms = inicio.elapsed().as_millis();
        println!("julia {CHAOS_MAX_NX}x{CHAOS_MAX_NY}@256: {ms}ms");
        assert!(ms < 1500, "{ms}ms");

        let inicio = std::time::Instant::now();
        let t = debe(trayectoria_lorenz([1.0, 1.0, 1.0], CHAOS_MAX_PASOS, 0.01));
        assert_eq!(t.len(), CHAOS_MAX_PASOS + 1);
        let ms = inicio.elapsed().as_millis();
        println!("lorenz {CHAOS_MAX_PASOS} pasos: {ms}ms");
        assert!(ms < 1500, "{ms}ms");

        let inicio = std::time::Instant::now();
        let g = debe(grilla_barrido(
            BIF_R_MIN,
            BIF_R_MAX,
            CHAOS_MAX_COLUMNAS,
            BIF_TRANSITORIO,
            CHAOS_MAX_ATRACTOR,
        ));
        assert_eq!(g.len() * g[0].len(), CHAOS_MAX_BARRIDO_CELDAS);
        let ms = inicio.elapsed().as_millis();
        println!("barrido {CHAOS_MAX_COLUMNAS}x{CHAOS_MAX_ATRACTOR}: {ms}ms");
        assert!(ms < 1500, "{ms}ms");

        let inicio = std::time::Instant::now();
        let a = debe(trayectoria_pendulo([1.9, 0.0, 2.6, 0.0], 2048, 0.01));
        let b = debe(trayectoria_pendulo([1.9 + 1e-6, 0.0, 2.6, 0.0], 2048, 0.01));
        assert_eq!((a.len(), b.len()), (2048, 2048));
        let ms = inicio.elapsed().as_millis();
        println!("pendulo 2x2048 pasos: {ms}ms");
        assert!(ms < 1500, "{ms}ms");
    }
}
