//! Plantillas EDO / Laplace / Fourier aplicado (nivel 3Blue1Brown).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! nuevas (solo `std` + `crate`). Los comandos del workspace ya calculan
//! (calor/onda en `grafito-geometry/src/pde.rs`, Fourier en
//! `grafito-geometry/src/fourier.rs`, Laplace/EDO en
//! `grafito-geometry/src/ode.rs`): acá se ANIMAN, con matemática propia
//! autocontenida para que la escena jamás dependa del CAS.
//!
//! Escenas (ver [`TEMPLATE_IDS`]):
//!
//! - `edo-campo-direcciones`: campo de direcciones `y' = f(x, y)` en
//!   `[-3, 3]²` + curvas solución integradas con RK2 punto medio y
//!   reveladas con [`Animation::create`] (3Blue1Brown: el campo como
//!   "espacio de estados" con flechas y la solución que lo recorre).
//! - `edo-convolucion`: flip-and-slide `(f*g)(t) = ∫₀ᵗ f(τ)g(t-τ)dτ` con
//!   `f` pulso, `g` exponencial causal: `g` volteada y deslizada, producto
//!   y área acumulada `h(t)`.
//! - `edo-laplace`: Laplace como descomposición en exponenciales:
//!   núcleos `e^{-st}` para varios `s`, producto `f(t)e^{-st}` con
//!   `f = 1`, y curva `F(s) = 1/s` con el punto `s*` activo.
//! - `edo-fourier-epiciclos`: serie seno de onda cuadrada como cadena de
//!   epiciclos (vectores de radio `4/(πk)`, `k` impar, girando a `kt`;
//!   la altura de la punta ES la suma parcial) + traza de `S_N(t)`.
//! - `edo-calor-onda`: calor `u_t = αu_xx` por modos
//!   `sin(nπx)e^{-(nπ)²αt}` que decaen, y onda `u_tt = c²u_xx` por
//!   d'Alembert `½[φ(x-ct)+φ(x+ct)]` (pulso que se parte y viaja, sin
//!   reflexiones: ventana corta, documentado).
//!
//! ## Reúso (dispatcher + `anims.rs` / `textanim.rs` / `protocol.rs`)
//!
//! - El render productivo vive en `grafito-app/src/anim_native.rs`
//!   (`render_anim_for_concept_with_params`, 48 frames por set en
//!   `NATIVE_ANIM_FRAME_COUNT`); el cableado (`lib.rs`, allowlist de
//!   `protocol.rs::CANONICAL_TEMPLATES`, `NATIVE_TEMPLATES`,
//!   `anim_ui.rs::PLANTILLAS_COMBO`) lo hace la coordinación: este módulo
//!   solo expone geometría + samplers y [`TEMPLATE_IDS`] en kebab-case.
//! - Progreso de curvas con `anims::{Animation, Scratch}`
//!   ([`anim_solucion`], [`muestra_anim_con`], y `escena_para` recibe el
//!   `&mut Scratch` del llamador: cero allocs intermedios por frame).
//! - Títulos con `textanim::Titulo` ([`titulo_para`]); params vivos con
//!   `protocol::{scene_param, SCENE_PARAM_TERMS}` ([`EdoParams`]).
//!
//! ## Presupuestos (acotados, todo lo que excede es `Err` honesto)
//!
//! | Recurso | Tope | Paridad |
//! |---|---|---|
//! | Términos/armónicos/núcleos | 1..=16 ([`EDO_MAX_TERMINOS`]) | `terms` vivo |
//! | Retícula del campo | 1..=64 por lado | `MAX_FIELD_DIVISIONS` |
//! | Muestras de trazo/curva | 2..=512 ([`EDO_MAX_MUESTRAS`]) | `SCENE_MORPH_MAX_SAMPLES` |
//! | Puntos por polilínea | ≤4096 (chequeado en [`valida_trazo`]) | `MAX_MOBJECT_POINTS` |
//! | Grilla calor/onda | `nx` 2..=128, `nt` 1..=48, `nx·nt` ≤ 6144 | player 48 frames |
//! | Wire | cada polilínea ≤512 pts (~10 KiB JSON ≪ 64 KiB) | `line_cap` 64 KiB, `MAX_TEX_SVG_BYTES` |
//!
//! Las grillas de calor/onda son el caso que "pide grilla": van por
//! [`grilla_calor_onda`] con los tres topes a la vez; el frente compone por
//! frames y jamás arma el `Vec` total en long-form (contrato P0.1 del
//! protocolo). Epiciclos NO se agrupan en `Group` a propósito (16 armónicos
//! darían 33 hijos > `MAX_GROUP_CHILDREN = 32`): [`escena_para`] devuelve el
//! `Vec<PlacedMobject>` plano.
//!
//! ## Fuentes (verificadas por búsqueda web en esta sesión)
//!
//! - EDO como campo/espacio de estados: 3Blue1Brown "Differential
//!   equations, studying the unsolvable"
//!   (<https://www.3blue1brown.com/lessons/differential-equations/>) y
//!   "Differential equations, a tourist's guide (DE1)"
//!   (<https://www.youtube.com/watch?v=p_di4Zn4wz4>); campos de pendientes:
//!   DE-BOOK "Slope Fields"
//!   (<https://geoff-cox.github.io/debookrs/interactive/slope-fields.html>),
//!   DIFFYQS "Slope fields"
//!   (<https://web.uvic.ca/~tbazett/diffyqsold/slopefields_section.html>),
//!   NovaMath "Slope Field Visualizer"
//!   (<https://www.novamath.org/tools/slope-field-visualizer.html>).
//! - Convolución flip-and-slide: "Convolution Visualizer"
//!   (<https://shelvean.github.io/math-tools/convolution.html>),
//!   BetterExplained "Intuitive Guide to Convolution"
//!   (<https://betterexplained.com/articles/intuitive-convolution/>),
//!   Stanford "Spatial convolution"
//!   (<http://www.graphics.stanford.edu/courses/cs178-10/applets/convolution.html>),
//!   MIT Mathlets "Convolution: Flip and Drag"
//!   (<https://mathlets.org/daimp/ConvFlipDrag.html>),
//!   "Signal Processing Toolkit"
//!   (<https://lms-spt.e-technik.uni-erlangen.de/demos/convolution/>).
//! - Laplace como proyección sobre exponenciales (`L{f'} = sF - f(0)` vía
//!   integración por partes): DE-BOOK "Laplace Transforms"
//!   (<https://geoff-cox.github.io/debookrs/interactive/chpt-laplace-transforms.html>),
//!   notas MIT 18.031 (<https://math.mit.edu/~stoopn/18.031/laplace-notes.pdf>),
//!   "Laplace Transform Intuition"
//!   (<https://www.overleaf.com/articles/laplace-transform-intuition/vtzkrpspkhmw.pdf>),
//!   Wikipedia "Laplace transform"
//!   (<https://en.wikipedia.org/wiki/Laplace_transform>),
//!   DIFFYQS "The Laplace transform"
//!   (<https://web.uvic.ca/~tbazett/diffyqsold/laplace_section.html>),
//!   MathOverflow "Motivating the Laplace transform definition"
//!   (<https://mathoverflow.net/questions/383/motivating-the-laplace-transform-definition>).
//! - Fourier con epiciclos y ecuación del calor/onda: la búsqueda web de
//!   esta sesión NO devolvió resultados verificables, así que no se cita
//!   URL: la construcción es la estándar de manual (serie seno de onda
//!   cuadrada `4/(πk)`, cadena de fasores cuya altura es la suma parcial,
//!   modos `sin(nπx)` del calor, d'Alembert para la onda), pineada por los
//!   tests de abajo con formas cerradas.
//!
//! Sin `unwrap`/`expect` en producción (`unwrap_used = deny`).

use crate::anims::{smooth, Animation, Scratch};
use crate::player::{centroide_de, PlacedMobject, PLAYER_MAX_FRAMES};
use crate::protocol::{scene_param, SCENE_PARAM_TERMS};
use crate::scene::{
    Mobject, RateFunc, SceneError, SceneResult, MAX_FIELD_DIVISIONS, MAX_MOBJECT_POINTS,
};
use crate::textanim::{TextAnimError, Titulo};
use std::collections::BTreeMap;

// ── Registro ─────────────────────────────────────────────────────────────

/// Plantillas de este módulo, en kebab-case para el dispatcher
/// (`anim_native` las cablea; el wire las registra después en
/// `CANONICAL_TEMPLATES`). Ninguna colisiona con las 13 canónicas
/// (`fourier` existe; la de acá es `edo-fourier-epiciclos`).
pub const TEMPLATE_IDS: &[&str] = &[
    "edo-campo-direcciones",
    "edo-convolucion",
    "edo-laplace",
    "edo-fourier-epiciclos",
    "edo-calor-onda",
];

/// Título + descripción corta para la UI/preview (sin construir nada).
pub fn describe(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "edo-campo-direcciones" => Some((
            "Campos de direcciones",
            "y' = f(x, y) como flechas y sus curvas solución",
        )),
        "edo-convolucion" => Some(("Convolución", "voltear, deslizar, multiplicar e integrar")),
        "edo-laplace" => Some((
            "Transformada de Laplace",
            "proyectar f(t) sobre las exponenciales e^{-st}",
        )),
        "edo-fourier-epiciclos" => {
            Some(("Serie de Fourier", "epiciclos que dibujan la onda cuadrada"))
        }
        "edo-calor-onda" => Some(("Calor y onda", "modos que decaen y pulsos que viajan")),
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
            "EDO",
            Some("plantilla desconocida"),
            32.0,
            canvas_w,
            canvas_h,
        ),
    }
}

// ── Presupuestos ─────────────────────────────────────────────────────────

/// Términos/armónicos/núcleos máximos por escena (el slider vivo `terms`
/// clampa acá; 16 epiciclos dan 33 mobjects planos, bajo todo tope).
pub const EDO_MAX_TERMINOS: usize = 16;
/// Muestras máximas por trazo o curva (paridad `SCENE_MORPH_MAX_SAMPLES`).
pub const EDO_MAX_MUESTRAS: usize = 512;
/// Muestras espaciales máximas de calor/onda por perfil.
pub const EDO_HEAT_MAX_NX: usize = 128;
/// Perfiles temporales máximos de la grilla (paridad `PLAYER_MAX_FRAMES`).
pub const EDO_HEAT_MAX_NT: usize = PLAYER_MAX_FRAMES;
/// Celdas máximas de la grilla (`128 × 48`; el frente compone por frames).
pub const EDO_HEAT_MAX_CELDAS: usize = EDO_HEAT_MAX_NX * EDO_HEAT_MAX_NT;
/// Semiancho del campo de direcciones (viewport `[-3, 3]²`).
pub const EDO_CAMPO_SEMI: f64 = 3.0;
/// Tope de `|k|` en los RHS paramétricos (evita overflow en la retícula).
pub const EDO_K_MAX: f64 = 100.0;
/// Horizonte de la convolución animada (`t ∈ [0, 4]`, pulso en `[0, 2]`).
pub const CONV_T_MAX: f64 = 4.0;
/// Truncado de la integral de Laplace (`∫₀²⁰`; cola `e^{-20} ≈ 2e-9`).
pub const LAPLACE_T_MAX: f64 = 20.0;
/// Ventana visible de los núcleos (los 20 s no entran en plano).
pub const LAPLACE_T_VIZ: f64 = 6.0;
/// Rango de `s` para `F(s)` (`1/s` en `[0.25, 6]`).
pub const LAPLACE_S_MIN: f64 = 0.25;
/// Rango de `s` para `F(s)` (`1/s` en `[0.25, 6]`).
pub const LAPLACE_S_MAX: f64 = 6.0;
/// Difusividad de la escena de calor (unidades arbitrarias de animación).
pub const CALOR_ALFA: f64 = 0.05;
/// Velocidad de la escena de onda (idem).
pub const ONDA_C: f64 = 0.5;
/// Ancho del pulso gaussiano inicial (idem).
pub const ONDA_SIGMA: f64 = 0.08;
/// Horizonte temporal animado de calor/onda.
pub const HEAT_T_MAX: f64 = 1.0;

// ── Claves vivas del wire ────────────────────────────────────────────────

/// Retícula x / muestras / espacial (según plantilla; ver [`EdoParams`]).
pub const EDO_PARAM_NX: &str = "nx";
/// Retícula y / conteo temporal de la grilla calor-onda.
pub const EDO_PARAM_NY: &str = "ny";
/// Punto `s*` activo de la escena de Laplace.
pub const EDO_PARAM_S: &str = "s";
/// Instante reservado (la escena la maneja `alpha`; debe ser finito ≥ 0).
pub const EDO_PARAM_TIEMPO: &str = "t";
/// Parámetro `k` de los RHS del campo.
pub const EDO_PARAM_K: &str = "k";
/// Selector de variante (campo 0/1/2, calor-onda 0/1).
pub const EDO_PARAM_MODO: &str = "modo";

// ── Params ───────────────────────────────────────────────────────────────

/// Params vivos de las 5 plantillas (todo `f64` crudo del wire; cada getter
/// valida y convierte a entero/rango con `Err` honesto).
///
/// | Clave | Campo | Convolución | Laplace | Fourier | Calor/onda |
/// |---|---|---|---|---|---|
/// | `terms` | — | — | nº núcleos 1..=16 | nº armónicos 1..=16 | nº modos 1..=16 |
/// | `nx` | retícula x 1..=64 | muestras 2..=512 | muestras 2..=512 | muestras traza 2..=512 | espacial 2..=128 |
/// | `ny` | retícula y 1..=64 | — | — | — | perfiles `nt` 1..=48 |
/// | `s` | — | — | `s*` 0.1..=10 | — | — |
/// | `t` | reservado | reservado | reservado | reservado | reservado |
/// | `k` | `|k| ≤ 100` | — | — | — | — |
/// | `modo` | 0 exp / 1 log / 2 suma | — | — | — | 0 calor / 1 onda |
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdoParams {
    /// Nº de términos/armónicos/núcleos.
    pub terms: f64,
    /// Retícula x / muestras / espacial.
    pub nx: f64,
    /// Retícula y / conteo temporal.
    pub ny: f64,
    /// Punto `s*` de Laplace.
    pub s: f64,
    /// Instante reservado (finito ≥ 0; la escena la maneja `alpha`).
    pub t: f64,
    /// Parámetro de los RHS.
    pub k: f64,
    /// Selector de variante.
    pub modo: f64,
}

impl EdoParams {
    /// Defaults por plantilla (lo que se ve sin mover sliders).
    pub fn por_defecto(id: &str) -> Self {
        match id {
            "edo-campo-direcciones" => Self {
                terms: 6.0,
                nx: 13.0,
                ny: 9.0,
                s: 1.5,
                t: 0.0,
                k: 1.0,
                modo: 0.0,
            },
            "edo-calor-onda" => Self {
                terms: 6.0,
                nx: 64.0,
                ny: 1.0,
                s: 1.5,
                t: 0.0,
                k: 1.0,
                modo: 0.0,
            },
            _ => Self {
                terms: 6.0,
                nx: 241.0,
                ny: 1.0,
                s: 1.5,
                t: 0.0,
                k: 1.0,
                modo: 0.0,
            },
        }
    }

    /// Mezcla el mapa del wire sobre los defaults (ausente/NaN/inf →
    /// default, vía `protocol::scene_param`; el rango lo valida cada getter).
    pub fn desde_mapa(id: &str, params: &BTreeMap<String, f64>) -> Self {
        let base = Self::por_defecto(id);
        Self {
            terms: scene_param(params, SCENE_PARAM_TERMS, base.terms),
            nx: scene_param(params, EDO_PARAM_NX, base.nx),
            ny: scene_param(params, EDO_PARAM_NY, base.ny),
            s: scene_param(params, EDO_PARAM_S, base.s),
            t: scene_param(params, EDO_PARAM_TIEMPO, base.t),
            k: scene_param(params, EDO_PARAM_K, base.k),
            modo: scene_param(params, EDO_PARAM_MODO, base.modo),
        }
    }

    /// Términos 1..=16 (todos los contadores de la escena).
    pub fn terminos(&self) -> SceneResult<usize> {
        if !self.terms.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.terms",
                detalle: "terms no finito: pasame 1..=16".to_string(),
            });
        }
        let n = self.terms as usize;
        if !(1..=EDO_MAX_TERMINOS).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.terms",
                detalle: format!("terms {} fuera de 1..={EDO_MAX_TERMINOS}", self.terms),
            });
        }
        Ok(n)
    }

    /// Retícula del campo 1..=64 por lado (paridad `MAX_FIELD_DIVISIONS`).
    pub fn reticula(&self) -> SceneResult<(usize, usize)> {
        let mut out = (0usize, 0usize);
        for (k, v, slot) in [("nx", self.nx, &mut out.0), ("ny", self.ny, &mut out.1)] {
            if !v.is_finite() {
                return Err(SceneError::MobjectInvalido {
                    donde: "EdoParams.reticula",
                    detalle: format!("{k} no finito: pasame 1..={MAX_FIELD_DIVISIONS}"),
                });
            }
            let n = v as usize;
            if !(1..=MAX_FIELD_DIVISIONS).contains(&n) {
                return Err(SceneError::MobjectInvalido {
                    donde: "EdoParams.reticula",
                    detalle: format!("{k} {v} fuera de 1..={MAX_FIELD_DIVISIONS}"),
                });
            }
            *slot = n;
        }
        Ok(out)
    }

    /// Muestras de trazo/curva 2..=512 (convolución, Laplace, Fourier).
    pub fn muestras(&self) -> SceneResult<usize> {
        if !self.nx.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.muestras",
                detalle: "nx no finito: pasame 2..=512".to_string(),
            });
        }
        let n = self.nx as usize;
        if !(2..=EDO_MAX_MUESTRAS).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.muestras",
                detalle: format!("nx {} fuera de 2..={EDO_MAX_MUESTRAS}", self.nx),
            });
        }
        Ok(n)
    }

    /// Espacial calor/onda 2..=128.
    pub fn nx_espacial(&self) -> SceneResult<usize> {
        if !self.nx.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.nx_espacial",
                detalle: "nx no finito: pasame 2..=128".to_string(),
            });
        }
        let n = self.nx as usize;
        if !(2..=EDO_HEAT_MAX_NX).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.nx_espacial",
                detalle: format!("nx {} fuera de 2..={EDO_HEAT_MAX_NX}", self.nx),
            });
        }
        Ok(n)
    }

    /// Perfiles temporales de la grilla 1..=48 (lee `ny`).
    pub fn nt(&self) -> SceneResult<usize> {
        if !self.ny.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.nt",
                detalle: "ny no finito: pasame 1..=48".to_string(),
            });
        }
        let n = self.ny as usize;
        if !(1..=EDO_HEAT_MAX_NT).contains(&n) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.nt",
                detalle: format!("ny {} fuera de 1..={EDO_HEAT_MAX_NT}", self.ny),
            });
        }
        Ok(n)
    }

    /// Punto `s*` 0.1..=10 (dentro del rango visible de `F`).
    pub fn s_estrella(&self) -> SceneResult<f64> {
        if !self.s.is_finite() || !(0.1..=10.0).contains(&self.s) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.s",
                detalle: format!("s {} fuera de 0.1..=10", self.s),
            });
        }
        Ok(self.s)
    }

    /// `k` finito con `|k| ≤ 100`.
    pub fn k_val(&self) -> SceneResult<f64> {
        if !self.k.is_finite() || self.k.abs() > EDO_K_MAX {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.k",
                detalle: format!("k {} fuera de ±{EDO_K_MAX}", self.k),
            });
        }
        Ok(self.k)
    }

    /// Instante reservado: finito ≥ 0 (hoy lo maneja `alpha`; se valida
    /// igual para no dejar pasar basura del wire en silencio).
    pub fn t_reservado(&self) -> SceneResult<f64> {
        if !self.t.is_finite() || self.t < 0.0 {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.t",
                detalle: format!("t {} inválido: pasame un finito ≥ 0", self.t),
            });
        }
        Ok(self.t)
    }

    /// Variante del campo (0 exp / 1 logística / 2 suma).
    pub fn modo_campo(&self) -> SceneResult<CampoEdo> {
        CampoEdo::desde_modo(self.modo)
    }

    /// `true` = onda, `false` = calor (`modo` 0/1).
    pub fn es_onda(&self) -> SceneResult<bool> {
        if !self.modo.is_finite() || !(0.0..=1.0).contains(&self.modo) {
            return Err(SceneError::MobjectInvalido {
                donde: "EdoParams.modo",
                detalle: format!("modo {} fuera de 0..=1 (0 calor, 1 onda)", self.modo),
            });
        }
        Ok(self.modo >= 0.5)
    }
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

/// Trazo con 1..=4096 puntos finitos (guarda del wire: cada polilínea viaja
/// con holgura bajo el `line_cap` de 64 KiB). Pura en validación.
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

/// Colocado opaco centrado en su centroide honesto. Puro.
fn colocado(mobject: Mobject, opacity: f32) -> SceneResult<PlacedMobject> {
    let centro = centroide_de(&mobject);
    PlacedMobject::try_new(mobject, opacity, 1.0, centro)
}

// ── 1. Campo de direcciones + curvas solución ────────────────────────────

/// RHS del campo (`k` ya validado por [`EdoParams::k_val`]).
///
/// - Exponencial: `k·y` (crecimiento/decaimiento).
/// - Logístico: `k·y·(1-y)` (capacidad 1, equilibrios en 0 y 1).
/// - Suma: `x + k·y` (el ejemplo de manual para ver cizalla).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoEdo {
    /// `y' = k·y`.
    Exponencial,
    /// `y' = k·y·(1-y)`.
    Logistico,
    /// `y' = x + k·y`.
    Suma,
}

impl CampoEdo {
    /// `0 → Exponencial, 1 → Logistico, 2 → Suma`; resto `Err` honesto.
    pub fn desde_modo(v: f64) -> SceneResult<Self> {
        if !v.is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "CampoEdo",
                detalle: "modo no finito: pasame 0, 1 o 2".to_string(),
            });
        }
        match v as usize {
            0 => Ok(Self::Exponencial),
            1 => Ok(Self::Logistico),
            2 => Ok(Self::Suma),
            _ => Err(SceneError::MobjectInvalido {
                donde: "CampoEdo",
                detalle: format!("modo {v} inválido: 0 exponencial, 1 logístico, 2 suma"),
            }),
        }
    }

    /// Pendiente `f(x, y)` (total: entradas finitas + `|k| ≤ 100` dan salida
    /// finita; si igual desborda, el integrador corta honesto).
    pub fn rhs(self, k: f64, x: f64, y: f64) -> f64 {
        match self {
            Self::Exponencial => k * y,
            Self::Logistico => k * y * (1.0 - y),
            Self::Suma => x + k * y,
        }
    }
}

/// Flechas del campo en `[-3, 3]²` como segmentos (`nx × ny` ≤ 4096,
/// 1..=64 por lado). Cada flecha apunta según `atan(f)` con largo de celda.
pub fn campo_flechas(cual: CampoEdo, k: f64, nx: usize, ny: usize) -> SceneResult<Vec<Mobject>> {
    if !k.is_finite() || k.abs() > EDO_K_MAX {
        return Err(SceneError::MobjectInvalido {
            donde: "campo_flechas",
            detalle: format!("k {k} fuera de ±{EDO_K_MAX}"),
        });
    }
    if !(1..=MAX_FIELD_DIVISIONS).contains(&nx) || !(1..=MAX_FIELD_DIVISIONS).contains(&ny) {
        return Err(SceneError::MobjectInvalido {
            donde: "campo_flechas",
            detalle: format!("retícula {nx}×{ny} (válido 1..={MAX_FIELD_DIVISIONS} por lado)"),
        });
    }
    let semi = EDO_CAMPO_SEMI;
    let paso_x = 2.0 * semi / nx as f64;
    let paso_y = 2.0 * semi / ny as f64;
    let largo = 0.8 * paso_x.min(paso_y) / 2.0;
    let mut out = Vec::with_capacity(nx * ny);
    for i in 0..nx {
        for j in 0..ny {
            let x = -semi + (i as f64 + 0.5) * paso_x;
            let y = -semi + (j as f64 + 0.5) * paso_y;
            let m = cual.rhs(k, x, y);
            let m = if m.is_finite() { m } else { 0.0 };
            let ang = m.atan();
            let (dx, dy) = (ang.cos() * largo, ang.sin() * largo);
            out.push(Mobject::Line {
                from: [x - dx, y - dy],
                to: [x + dx, y + dy],
            });
        }
    }
    Ok(out)
}

/// Curva solución por RK2 punto medio desde `(x0, y0)` hasta `x1`
/// (`pasos` 1..=512). Corta honesto si sale del viewport (`|y| > 4`) o deja
/// de ser finita; `< 2` puntos es `Err` (nada que dibujar).
pub fn integra_solucion(
    cual: CampoEdo,
    k: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    pasos: usize,
) -> SceneResult<Vec<[f64; 2]>> {
    if !k.is_finite() || k.abs() > EDO_K_MAX {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: format!("k {k} fuera de ±{EDO_K_MAX}"),
        });
    }
    if ![x0, y0, x1].iter().all(|v| v.is_finite()) {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: "condición inicial o extremo no finito".to_string(),
        });
    }
    let tramo_crece = matches!(x0.partial_cmp(&x1), Some(std::cmp::Ordering::Less));
    if !tramo_crece || x1 - x0 > 12.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: format!("tramo [{x0}, {x1}] inválido: necesito x1 > x0 con span ≤ 12"),
        });
    }
    if y0.abs() > 4.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: format!("y0 {y0} fuera del viewport (|y| ≤ 4)"),
        });
    }
    if !(1..=EDO_MAX_MUESTRAS).contains(&pasos) {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: format!("pasos {pasos} fuera de 1..={EDO_MAX_MUESTRAS}"),
        });
    }
    let h = (x1 - x0) / pasos as f64;
    let mut pts = Vec::with_capacity(pasos + 1);
    let (mut x, mut y) = (x0, y0);
    pts.push([x, y]);
    for _ in 0..pasos {
        let s1 = cual.rhs(k, x, y);
        let s2 = cual.rhs(k, x + 0.5 * h, y + 0.5 * h * s1);
        if !s1.is_finite() || !s2.is_finite() {
            break;
        }
        let y_nueva = y + h * s2;
        if !y_nueva.is_finite() || y_nueva.abs() > 4.0 {
            break;
        }
        x += h;
        y = y_nueva;
        pts.push([x, y]);
    }
    if pts.len() < 2 {
        return Err(SceneError::MobjectInvalido {
            donde: "integra_solucion",
            detalle: "la curva muere en el primer paso: probá otro y0".to_string(),
        });
    }
    valida_trazo(&pts, "integra_solucion")?;
    Ok(pts)
}

/// Revelado de la curva (`Create` de `anims.rs` con su propio rate).
pub fn anim_solucion(
    trazo: Vec<[f64; 2]>,
    frames: usize,
    run_ms: u64,
    rate: RateFunc,
) -> SceneResult<Animation> {
    Animation::create(trazo, frames, run_ms, rate, false)
}

/// Muestra del revelado reutilizando el `Scratch` del llamador.
pub fn muestra_anim_con(
    anim: &Animation,
    alpha: f64,
    scratch: &mut Scratch,
) -> SceneResult<PlacedMobject> {
    anim.sample_con(alpha, scratch)
}

// ── 2. Convolución flip-and-slide ────────────────────────────────────────

/// `f(τ)`: pulso unitario en `[0, 2]` (la señal que se barre).
pub fn conv_f(tau: f64) -> f64 {
    if tau.is_finite() && (0.0..=2.0).contains(&tau) {
        1.0
    } else {
        0.0
    }
}

/// `g(τ)`: exponencial causal `e^{-τ}·[τ ≥ 0]` (el núcleo que se voltea).
pub fn conv_g(tau: f64) -> f64 {
    if tau.is_finite() && tau >= 0.0 {
        (-tau).exp()
    } else {
        0.0
    }
}

/// Núcleo volteado y deslizado `g(t-τ)` (lo verde que barre a `f`). Puro.
pub fn conv_g_desplazada(t: f64, tau: f64) -> f64 {
    if !t.is_finite() || !tau.is_finite() {
        return 0.0;
    }
    conv_g(t - tau)
}

/// Grilla `τ ∈ [0, t_max]` con `muestras` 2..=512 puntos.
pub fn malla_tau(t_max: f64, muestras: usize) -> SceneResult<Vec<f64>> {
    if !t_max.is_finite() || t_max <= 0.0 || t_max > 8.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "malla_tau",
            detalle: format!("t_max {t_max} fuera de 0 < t ≤ 8"),
        });
    }
    if !(2..=EDO_MAX_MUESTRAS).contains(&muestras) {
        return Err(SceneError::MobjectInvalido {
            donde: "malla_tau",
            detalle: format!("muestras {muestras} fuera de 2..={EDO_MAX_MUESTRAS}"),
        });
    }
    let mut out = Vec::with_capacity(muestras);
    for i in 0..muestras {
        out.push(t_max * i as f64 / (muestras - 1) as f64);
    }
    Ok(out)
}

/// `(f*g)(t)` por trapecios en `[0, t]` (`t` 0..=8, `muestras` 2..=512).
/// Formas cerradas (pineadas por test): `t ∈ [0,2] → 1-e^{-t}`,
/// `t ≥ 2 → e^{-(t-2)}-e^{-t}`, `h(0) = 0`.
pub fn convolucion_en(t: f64, muestras: usize) -> SceneResult<f64> {
    if !t.is_finite() || t < 0.0 || t > 8.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "convolucion_en",
            detalle: format!("t {t} fuera de 0..=8"),
        });
    }
    if !(2..=EDO_MAX_MUESTRAS).contains(&muestras) {
        return Err(SceneError::MobjectInvalido {
            donde: "convolucion_en",
            detalle: format!("muestras {muestras} fuera de 2..={EDO_MAX_MUESTRAS}"),
        });
    }
    if t == 0.0 {
        return Ok(0.0);
    }
    let h = t / (muestras - 1) as f64;
    let mut acc = 0.0;
    for i in 0..muestras {
        let tau = h * i as f64;
        let v = conv_f(tau) * conv_g_desplazada(t, tau);
        let peso = if i == 0 || i + 1 == muestras {
            0.5
        } else {
            1.0
        };
        acc += peso * v;
    }
    let v = acc * h;
    Ok(if v.is_finite() { v } else { 0.0 })
}

/// Curva `h(t)` en `[0, t_max]` (`muestras` puntos, 2..=512).
pub fn curva_convolucion(t_max: f64, muestras: usize) -> SceneResult<Vec<[f64; 2]>> {
    let taus = malla_tau(t_max, muestras)?;
    let mut pts = Vec::with_capacity(muestras);
    for t in taus {
        pts.push([t, convolucion_en(t, muestras)?]);
    }
    valida_trazo(&pts, "curva_convolucion")?;
    Ok(pts)
}

// ── 3. Laplace como descomposición en exponenciales ──────────────────────

/// `f(t) = 1` (escalón: su `F(s) = 1/s` pinea la cuadratura).
pub fn laplace_f(t: f64) -> f64 {
    if t.is_finite() && t >= 0.0 {
        1.0
    } else {
        0.0
    }
}

/// Núcleo `e^{-st}` (la "dirección" sobre la que se proyecta `f`). Puro.
pub fn nucleo_en(s: f64, t: f64) -> f64 {
    if !s.is_finite() || !t.is_finite() || s < 0.0 || t < 0.0 {
        return 0.0;
    }
    let v = (-s * t).exp();
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Integrando `f(t)·e^{-st}`. Puro.
pub fn laplace_producto(t: f64, s: f64) -> f64 {
    laplace_f(t) * nucleo_en(s, t)
}

/// `F(s)` por trapecios en `[0, 20]` con 512 pasos fijos (con `f = 1` da
/// `1/s` con error < 5e-3 en `s ≥ 0.1`; la cola vale `e^{-20s}/s`).
pub fn laplace_en(s: f64) -> SceneResult<f64> {
    if !s.is_finite() || s < 0.1 || s > 10.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "laplace_en",
            detalle: format!("s {s} fuera de 0.1..=10"),
        });
    }
    let n = 512usize;
    let h = LAPLACE_T_MAX / n as f64;
    let mut acc = 0.0;
    for i in 0..=n {
        let v = laplace_producto(h * i as f64, s);
        let peso = if i == 0 || i == n { 0.5 } else { 1.0 };
        acc += peso * v;
    }
    let v = acc * h;
    Ok(if v.is_finite() { v } else { 0.0 })
}

/// Curva `F(s)` en `[s0, s1]` (`0.05 ≤ s0 < s1 ≤ 20`, `n` 2..=512).
pub fn curva_laplace(s0: f64, s1: f64, n: usize) -> SceneResult<Vec<[f64; 2]>> {
    let ventana_crece = matches!(s0.partial_cmp(&s1), Some(std::cmp::Ordering::Less));
    if ![s0, s1].iter().all(|v| v.is_finite()) || !ventana_crece || s0 < 0.05 || s1 > 20.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "curva_laplace",
            detalle: format!("ventana [{s0}, {s1}] inválida: necesito 0.05 ≤ s0 < s1 ≤ 20"),
        });
    }
    if !(2..=EDO_MAX_MUESTRAS).contains(&n) {
        return Err(SceneError::MobjectInvalido {
            donde: "curva_laplace",
            detalle: format!("n {n} fuera de 2..={EDO_MAX_MUESTRAS}"),
        });
    }
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let s = s0 + (s1 - s0) * i as f64 / (n - 1) as f64;
        let f = if s <= 10.0 { laplace_en(s)? } else { 1.0 / s };
        pts.push([s, f]);
    }
    valida_trazo(&pts, "curva_laplace")?;
    Ok(pts)
}

// ── 4. Fourier con epiciclos ─────────────────────────────────────────────

/// Valida el conteo de armónicos 1..=16.
fn valida_terminos(terms: usize, donde: &'static str) -> SceneResult<()> {
    if !(1..=EDO_MAX_TERMINOS).contains(&terms) {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("terms {terms} fuera de 1..={EDO_MAX_TERMINOS}"),
        });
    }
    Ok(())
}

/// Armónico `j` (0-based) de la onda cuadrada: radio `4/(πk)` con
/// `k = 2j+1` impar. Puro.
pub fn coef_cuadrada(j: usize) -> (f64, f64) {
    let k = 2.0 * j as f64 + 1.0;
    (4.0 / (std::f64::consts::PI * k), k)
}

/// Suma parcial `S_N(t) = Σ 4/(πk)·sin(kt)` (`terms` 1..=16, `t` finito).
pub fn suma_fourier(t: f64, terms: usize) -> SceneResult<f64> {
    valida_terminos(terms, "suma_fourier")?;
    if !t.is_finite() {
        return Err(SceneError::MobjectInvalido {
            donde: "suma_fourier",
            detalle: "t no finito".to_string(),
        });
    }
    let mut acc = 0.0;
    for j in 0..terms {
        let (r, k) = coef_cuadrada(j);
        acc += r * (k * t).sin();
    }
    Ok(if acc.is_finite() { acc } else { 0.0 })
}

/// Centros de la cadena de epiciclos en `t` (`terms+1` puntos: origen +
/// una punta por armónico; el fasor `j` gira a `k·t` con radio `r` y la
/// altura de la punta final ES `S_N(t)`).
pub fn epiciclos_en(t: f64, terms: usize) -> SceneResult<Vec<[f64; 2]>> {
    valida_terminos(terms, "epiciclos_en")?;
    if !t.is_finite() {
        return Err(SceneError::MobjectInvalido {
            donde: "epiciclos_en",
            detalle: "t no finito".to_string(),
        });
    }
    let mut centros = Vec::with_capacity(terms + 1);
    let mut c = [0.0, 0.0];
    centros.push(c);
    for j in 0..terms {
        let (r, k) = coef_cuadrada(j);
        let a = k * t;
        c = [c[0] + r * a.cos(), c[1] + r * a.sin()];
        if !c[0].is_finite() || !c[1].is_finite() {
            return Err(SceneError::MobjectInvalido {
                donde: "epiciclos_en",
                detalle: format!("cadena no finita en el armónico {j}"),
            });
        }
        centros.push(c);
    }
    Ok(centros)
}

/// Mobjects de los epiciclos: un `Circle` + un brazo `Line` por armónico y
/// un `Dot` en la punta (`2·terms+1` objetos planos, sin `Group` a
/// propósito: 16 armónicos darían 33 hijos > `MAX_GROUP_CHILDREN`).
pub fn mobjects_epiciclos(t: f64, terms: usize) -> SceneResult<Vec<Mobject>> {
    let centros = epiciclos_en(t, terms)?;
    let mut out = Vec::with_capacity(2 * terms + 1);
    for j in 0..terms {
        let (r, _) = coef_cuadrada(j);
        let c = centros[j];
        out.push(Mobject::Circle {
            cx: c[0],
            cy: c[1],
            r,
        });
        out.push(Mobject::Line {
            from: c,
            to: centros[j + 1],
        });
    }
    let punta = centros[terms];
    out.push(Mobject::Dot {
        x: punta[0],
        y: punta[1],
    });
    for m in &out {
        m.validate()?;
    }
    Ok(out)
}

/// Traza `[t, S_N(t)]` con `t ∈ [0, 2π]` (`muestras` 2..=512).
pub fn traza_fourier(terms: usize, muestras: usize) -> SceneResult<Vec<[f64; 2]>> {
    valida_terminos(terms, "traza_fourier")?;
    if !(2..=EDO_MAX_MUESTRAS).contains(&muestras) {
        return Err(SceneError::MobjectInvalido {
            donde: "traza_fourier",
            detalle: format!("muestras {muestras} fuera de 2..={EDO_MAX_MUESTRAS}"),
        });
    }
    let mut pts = Vec::with_capacity(muestras);
    for i in 0..muestras {
        let t = 2.0 * std::f64::consts::PI * i as f64 / (muestras - 1) as f64;
        pts.push([t, suma_fourier(t, terms)?]);
    }
    valida_trazo(&pts, "traza_fourier")?;
    Ok(pts)
}

// ── 5. Calor y onda ──────────────────────────────────────────────────────

/// Valida `(x, t)` en `[0, 1] × [0, 5]` (ventana animada honesta).
fn valida_xt(x: f64, t: f64, donde: &'static str) -> SceneResult<()> {
    if !x.is_finite() || !(0.0..=1.0).contains(&x) {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("x {x} fuera de 0..=1"),
        });
    }
    if !t.is_finite() || !(0.0..=5.0).contains(&t) {
        return Err(SceneError::MobjectInvalido {
            donde,
            detalle: format!("t {t} fuera de 0..=5"),
        });
    }
    Ok(())
}

/// Calor `u(x,t) = Σ_{n impar} 4/(πn)·sin(nπx)·e^{-(nπ)²αt}` con `terms`
/// modos impares (`n = 1, 3, …, 2·terms-1`): el pulso inicial se derrite
/// modo a modo (los altos mueren primero ∝ `n²`). Dirichlet `u = 0` en
/// bordes por construcción.
pub fn calor_en(x: f64, t: f64, terms: usize) -> SceneResult<f64> {
    valida_xt(x, t, "calor_en")?;
    valida_terminos(terms, "calor_en")?;
    if x == 0.0 || x == 1.0 {
        return Ok(0.0);
    }
    let mut acc = 0.0;
    for j in 0..terms {
        let n = 2.0 * j as f64 + 1.0;
        let decae = (-(n * std::f64::consts::PI).powi(2) * CALOR_ALFA * t).exp();
        acc += 4.0 / (std::f64::consts::PI * n) * (n * std::f64::consts::PI * x).sin() * decae;
    }
    Ok(if acc.is_finite() { acc } else { 0.0 })
}

/// Pulso inicial de la onda (gaussiana centrada en 0.5). Puro.
fn phi_onda(x: f64) -> f64 {
    let d = (x - 0.5) / ONDA_SIGMA;
    (-d * d).exp()
}

/// Onda por d'Alembert `u = ½[φ(x-ct)+φ(x+ct)]`: el pulso se parte en dos
/// y viaja. Honesto: sin reflexiones en los bordes (la ventana animada
/// `t ≤ 1` con `c = 0.5` apenas los roza; la Piel no debe prometer Dirichlet).
pub fn onda_en(x: f64, t: f64) -> SceneResult<f64> {
    valida_xt(x, t, "onda_en")?;
    let v = 0.5 * (phi_onda(x - ONDA_C * t) + phi_onda(x + ONDA_C * t));
    Ok(if v.is_finite() { v } else { 0.0 })
}

/// Perfil `[x, u]` con `nx` 2..=128 puntos (`terms` solo lo usa el calor).
pub fn perfil_calor_onda(
    onda: bool,
    t: f64,
    nx: usize,
    terms: usize,
) -> SceneResult<Vec<[f64; 2]>> {
    if !(2..=EDO_HEAT_MAX_NX).contains(&nx) {
        return Err(SceneError::MobjectInvalido {
            donde: "perfil_calor_onda",
            detalle: format!("nx {nx} fuera de 2..={EDO_HEAT_MAX_NX}"),
        });
    }
    if !t.is_finite() || !(0.0..=5.0).contains(&t) {
        return Err(SceneError::MobjectInvalido {
            donde: "perfil_calor_onda",
            detalle: format!("t {t} fuera de 0..=5"),
        });
    }
    if onda {
        let mut pts = Vec::with_capacity(nx);
        for i in 0..nx {
            let x = i as f64 / (nx - 1) as f64;
            pts.push([x, onda_en(x, t)?]);
        }
        valida_trazo(&pts, "perfil_calor_onda")?;
        Ok(pts)
    } else {
        valida_terminos(terms, "perfil_calor_onda")?;
        let mut pts = Vec::with_capacity(nx);
        for i in 0..nx {
            let x = i as f64 / (nx - 1) as f64;
            pts.push([x, calor_en(x, t, terms)?]);
        }
        valida_trazo(&pts, "perfil_calor_onda")?;
        Ok(pts)
    }
}

/// Mapea el perfil `[0,1]×ℝ` al viewport `[-3,3]²` (`x → -3+6x`, `y`
/// clamp a `±3` para no mentir fuera de plano). Puro.
pub fn perfil_a_mundo(perfil: &[[f64; 2]]) -> Vec<[f64; 2]> {
    perfil
        .iter()
        .map(|p| {
            let x = -EDO_CAMPO_SEMI + 2.0 * EDO_CAMPO_SEMI * p[0];
            let y = if p[1].is_finite() {
                p[1].clamp(-EDO_CAMPO_SEMI, EDO_CAMPO_SEMI)
            } else {
                0.0
            };
            [x, y]
        })
        .collect()
}

/// Grilla de `nt` perfiles (`nx` 2..=128, `nt` 1..=48, `nx·nt` ≤ 6144,
/// `t_max` finito en 0..=5): el caso que "pide grilla", acotado por los
/// tres topes a la vez. El frente la consume por frames.
pub fn grilla_calor_onda(
    onda: bool,
    nx: usize,
    nt: usize,
    terms: usize,
    t_max: f64,
) -> SceneResult<Vec<Vec<[f64; 2]>>> {
    if !(2..=EDO_HEAT_MAX_NX).contains(&nx) {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla nx {nx} fuera de 2..={EDO_HEAT_MAX_NX}"),
        });
    }
    if !(1..=EDO_HEAT_MAX_NT).contains(&nt) {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla nt {nt} fuera de 1..={EDO_HEAT_MAX_NT}"),
        });
    }
    let celdas = nx.saturating_mul(nt);
    if celdas > EDO_HEAT_MAX_CELDAS {
        return Err(SceneError::PresupuestoExcedido {
            detalle: format!("grilla {nx}×{nt} = {celdas} celdas (tope {EDO_HEAT_MAX_CELDAS})"),
        });
    }
    if !t_max.is_finite() || t_max <= 0.0 || t_max > 5.0 {
        return Err(SceneError::MobjectInvalido {
            donde: "grilla_calor_onda",
            detalle: format!("t_max {t_max} fuera de 0 < t ≤ 5"),
        });
    }
    let mut out = Vec::with_capacity(nt);
    for k in 0..nt {
        let t = if nt == 1 {
            0.0
        } else {
            t_max * k as f64 / (nt - 1) as f64
        };
        out.push(perfil_calor_onda(onda, t, nx, terms)?);
    }
    Ok(out)
}

// ── Escena por id (lo que cablea el dispatcher) ──────────────────────────

/// Fotograma `alpha` 0..1 (no finito → 0) de la plantilla `id` con los
/// params vivos. Desconocido → `Err` honesto (jamás placeholder con curva
/// falsa). El `Scratch` lo provee el llamador (reúso `anims.rs`).
pub fn escena_para(
    id: &str,
    params: &EdoParams,
    alpha: f64,
    scratch: &mut Scratch,
) -> SceneResult<Vec<PlacedMobject>> {
    let a = clamp01(alpha);
    let _ = params.t_reservado()?;
    match id {
        "edo-campo-direcciones" => {
            let cual = params.modo_campo()?;
            let k = params.k_val()?;
            let (nx, ny) = params.reticula()?;
            let mut out = Vec::with_capacity(nx * ny + 3);
            for flecha in campo_flechas(cual, k, nx, ny)? {
                out.push(colocado(flecha, 0.75)?);
            }
            for y0 in [-1.5, 0.0, 1.5] {
                let trazo = integra_solucion(cual, k, -3.0, y0, 3.0, 256)?;
                let anim = anim_solucion(trazo, 24, 2000, RateFunc::Smooth)?;
                out.push(muestra_anim_con(&anim, a, scratch)?);
            }
            Ok(out)
        }
        "edo-convolucion" => {
            let m = params.muestras()?;
            let t = a * CONV_T_MAX;
            let taus = malla_tau(CONV_T_MAX, m)?;
            let f_poly: Vec<[f64; 2]> = taus.iter().map(|tau| [*tau, conv_f(*tau)]).collect();
            let g_poly: Vec<[f64; 2]> = taus
                .iter()
                .map(|tau| [*tau, conv_g_desplazada(t, *tau)])
                .collect();
            let p_poly: Vec<[f64; 2]> = taus
                .iter()
                .map(|tau| [*tau, conv_f(*tau) * conv_g_desplazada(t, *tau)])
                .collect();
            valida_trazo(&f_poly, "escena_convolucion")?;
            let h_total = curva_convolucion(CONV_T_MAX, m)?;
            let hasta = ((a * m as f64).ceil() as usize).min(m).max(1);
            let h_tramo = h_total[..hasta].to_vec();
            let piezas = [
                (Mobject::Polygon { pts: f_poly }, 0.9),
                (Mobject::Polygon { pts: g_poly }, 0.9),
                (Mobject::Polygon { pts: p_poly }, 1.0),
                (Mobject::Polygon { pts: h_tramo }, 1.0),
                (
                    Mobject::Dot {
                        x: t,
                        y: convolucion_en(t, m)?,
                    },
                    1.0,
                ),
            ];
            let mut out = Vec::with_capacity(piezas.len());
            for (mobject, opacidad) in piezas {
                out.push(colocado(mobject, opacidad)?);
            }
            Ok(out)
        }
        "edo-laplace" => {
            let terms = params.terminos()?;
            let m = params.muestras()?;
            // Barrido en s: la sonda viaja 0.5→3.0 con el clip (antes s*
            // fijo de params y la escena solo se revelaba por prefijo:
            // a mitad de clip parecía congelada).
            let s_estrella = 0.5 + smooth(a) * 2.5;
            let mut out = Vec::with_capacity(terms + 3);
            for j in 0..terms {
                let s = 0.5 * (j as f64 + 1.0);
                let mut pts = Vec::with_capacity(m);
                for i in 0..m {
                    let t = LAPLACE_T_VIZ * i as f64 / (m - 1) as f64;
                    pts.push([t, nucleo_en(s, t)]);
                }
                valida_trazo(&pts, "escena_laplace")?;
                let op = if (s - s_estrella).abs() < 0.26 {
                    1.0
                } else {
                    0.45
                };
                out.push(colocado(Mobject::Polygon { pts }, op)?);
            }
            let curva = curva_laplace(LAPLACE_S_MIN, LAPLACE_S_MAX, m)?;
            out.push(colocado(Mobject::Polygon { pts: curva }, 1.0)?);
            out.push(colocado(
                Mobject::Dot {
                    x: s_estrella,
                    y: laplace_en(s_estrella)?,
                },
                1.0,
            )?);
            Ok(out)
        }
        "edo-fourier-epiciclos" => {
            let terms = params.terminos()?;
            let m = params.muestras()?;
            let t = a * 2.0 * std::f64::consts::PI;
            let mut out = Vec::with_capacity(2 * terms + 2);
            let centros = epiciclos_en(t, terms)?;
            for j in 0..terms {
                let (r, _) = coef_cuadrada(j);
                out.push(colocado(
                    Mobject::Circle {
                        cx: centros[j][0],
                        cy: centros[j][1],
                        r,
                    },
                    0.55,
                )?);
                out.push(colocado(
                    Mobject::Line {
                        from: centros[j],
                        to: centros[j + 1],
                    },
                    1.0,
                )?);
            }
            let traza = traza_fourier(terms, m)?;
            out.push(colocado(Mobject::Polygon { pts: traza }, 0.5)?);
            let punta = centros[terms];
            out.push(colocado(
                Mobject::Dot {
                    x: punta[0],
                    y: punta[1],
                },
                1.0,
            )?);
            Ok(out)
        }
        "edo-calor-onda" => {
            let onda = params.es_onda()?;
            let terms = params.terminos()?;
            let nx = params.nx_espacial()?;
            let t = a * HEAT_T_MAX;
            let mundo = perfil_a_mundo(&perfil_calor_onda(onda, t, nx, terms)?);
            let out = vec![
                colocado(Mobject::Axes, 0.5)?,
                colocado(Mobject::Polygon { pts: mundo }, 1.0)?,
            ];
            Ok(out)
        }
        otro => Err(SceneError::EscenaInvalida {
            detalle: format!("plantilla {otro:?} desconocida: elegí una de {TEMPLATE_IDS:?}"),
        }),
    }
}

// ── Tests inline ─────────────────────────────────────────────────────────

#[cfg(test)]
mod edo_tests {
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
            // Ninguna colisiona con las 13 canónicas del protocolo.
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
            ] {
                assert_ne!(*a, canonica);
            }
        }
        assert!(describe("no-existe").is_none());
        // Títulos centrados exactos (reúso textanim).
        let t = debe(titulo_para("edo-laplace", 640, 480));
        let l = t.layout();
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
        assert_eq!(l.offset_y, (480.0 - l.bloque_h) / 2.0);
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(EDO_MAX_TERMINOS, 16);
        assert_eq!(EDO_MAX_MUESTRAS, 512);
        assert_eq!(EDO_HEAT_MAX_NX, 128);
        assert_eq!(EDO_HEAT_MAX_NT, PLAYER_MAX_FRAMES);
        assert_eq!(EDO_HEAT_MAX_CELDAS, 128 * 48);
        assert_eq!((LAPLACE_S_MIN, LAPLACE_S_MAX), (0.25, 6.0));
    }

    #[test]
    fn campo_rhs_y_equilibrios() {
        assert_eq!(CampoEdo::Exponencial.rhs(1.0, 0.0, 2.0), 2.0);
        assert_eq!(CampoEdo::Logistico.rhs(1.0, 0.0, 1.0), 0.0);
        assert_eq!(CampoEdo::Logistico.rhs(1.0, 0.0, 0.0), 0.0);
        assert_eq!(CampoEdo::Suma.rhs(1.0, 2.0, 3.0), 5.0);
        assert!(CampoEdo::desde_modo(0.0).is_ok());
        assert!(CampoEdo::desde_modo(2.0).is_ok());
        assert!(CampoEdo::desde_modo(3.0).is_err());
        assert!(CampoEdo::desde_modo(f64::NAN).is_err());
        // Retícula 13×9 con todo finito + bordes honestos.
        let flechas = debe(campo_flechas(CampoEdo::Exponencial, 1.0, 13, 9));
        assert_eq!(flechas.len(), 117);
        assert!(campo_flechas(CampoEdo::Exponencial, 1.0, 0, 9).is_err());
        assert!(campo_flechas(CampoEdo::Exponencial, 1.0, 65, 9).is_err());
        assert!(campo_flechas(CampoEdo::Exponencial, f64::INFINITY, 4, 4).is_err());
    }

    #[test]
    fn integra_exponencial_da_e() {
        // y' = y, y(0) = 1 → e en x = 1 (RK2 punto medio, h = 1/256).
        let trazo = debe(integra_solucion(
            CampoEdo::Exponencial,
            1.0,
            0.0,
            1.0,
            1.0,
            256,
        ));
        assert_eq!(trazo.len(), 257);
        let y1 = trazo[256][1];
        assert!((y1 - std::f64::consts::E).abs() < 1e-4, "y(1) = {y1}");
        // y' = x + y, y(0) = 0 → e^x - x - 1 en x = 1.
        let trazo = debe(integra_solucion(CampoEdo::Suma, 1.0, 0.0, 0.0, 1.0, 256));
        let y1 = trazo[256][1];
        assert!(
            (y1 - (std::f64::consts::E - 2.0)).abs() < 1e-3,
            "y(1) = {y1}"
        );
        // Equilibrio logístico en y = 1 se queda quieto.
        let trazo = debe(integra_solucion(
            CampoEdo::Logistico,
            1.0,
            0.0,
            1.0,
            2.0,
            64,
        ));
        for p in &trazo {
            assert!((p[1] - 1.0).abs() < 1e-12);
        }
        assert!(integra_solucion(CampoEdo::Exponencial, 1.0, 1.0, 0.0, 1.0, 8).is_err());
        assert!(integra_solucion(CampoEdo::Exponencial, 1.0, 0.0, 1.0, 1.0, 0).is_err());
        assert!(integra_solucion(CampoEdo::Exponencial, 1.0, 0.0, 9.0, 1.0, 8).is_err());
    }

    #[test]
    fn anim_solucion_progresa_con_scratch() {
        let trazo = debe(integra_solucion(
            CampoEdo::Exponencial,
            1.0,
            0.0,
            1.0,
            1.0,
            64,
        ));
        let n = trazo.len();
        let anim = debe(anim_solucion(trazo, 8, 1000, RateFunc::Smooth));
        assert_eq!(anim.frames(), 8);
        let mut scratch = Scratch::nuevo();
        let p0 = debe(muestra_anim_con(&anim, 0.0, &mut scratch));
        let p1 = debe(muestra_anim_con(&anim, 1.0, &mut scratch));
        let len = |p: &PlacedMobject| match &p.mobject {
            Mobject::Polygon { pts } => pts.len(),
            _ => 0,
        };
        assert!(len(&p0) < len(&p1), "revela progresivo");
        assert_eq!(len(&p1), n);
        assert!(anim_solucion(vec![[0.0, 0.0]], 0, 1000, RateFunc::Linear).is_err());
    }

    #[test]
    fn convolucion_pineada_a_forma_cerrada() {
        assert_eq!(debe(convolucion_en(0.0, 64)), 0.0);
        // t ∈ [0,2]: 1 - e^{-t}.
        let h1 = debe(convolucion_en(1.0, 512));
        assert!((h1 - (1.0 - (-1.0f64).exp())).abs() < 1e-3, "h(1) = {h1}");
        // t ≥ 2: e^{-(t-2)} - e^{-t} (el pulso ya salió del todo).
        let h3 = debe(convolucion_en(3.0, 512));
        assert!(
            (h3 - ((-1.0f64).exp() - (-3.0f64).exp())).abs() < 1e-2,
            "h(3) = {h3}"
        );
        // Crece y después cae: el área se acumula y el núcleo la olvida.
        let h2 = debe(convolucion_en(2.0, 512));
        assert!(h2 > h1 && h3 < h2);
        assert!(convolucion_en(-1.0, 64).is_err());
        assert!(convolucion_en(1.0, 1).is_err());
        let curva = debe(curva_convolucion(4.0, 64));
        assert_eq!(curva.len(), 64);
    }

    #[test]
    fn laplace_pineada_a_uno_sobre_s() {
        let f1 = debe(laplace_en(1.0));
        assert!((f1 - 1.0).abs() < 5e-3, "F(1) = {f1}");
        let f2 = debe(laplace_en(2.0));
        assert!((f2 - 0.5).abs() < 5e-3, "F(2) = {f2}");
        assert!(laplace_en(0.05).is_err());
        assert!(laplace_en(11.0).is_err());
        let curva = debe(curva_laplace(0.25, 6.0, 32));
        assert_eq!(curva.len(), 32);
        for par in curva.windows(2) {
            assert!(par[1][1] <= par[0][1], "1/s decrece");
        }
        assert!(curva_laplace(2.0, 1.0, 8).is_err());
    }

    #[test]
    fn laplace_sonda_barre_con_alpha() {
        // La sonda s* viaja 0.5→3.0 con el clip (antes fija: la escena
        // parecía congelada). Se verifica por el Dot sobre F(s).
        let base = EdoParams::por_defecto("edo-laplace");
        let mut scratch = Scratch::nuevo();
        let mut punto = |alpha: f64| {
            let d = debe(escena_para("edo-laplace", &base, alpha, &mut scratch));
            d.iter()
                .filter_map(|o| match o.mobject {
                    Mobject::Dot { x, y } => Some((x, y)),
                    _ => None,
                })
                .next()
        };
        let (x0, y0) = punto(0.0).expect("dot en alpha 0");
        let (x1, y1) = punto(1.0).expect("dot en alpha 1");
        assert!((x0 - 0.5).abs() < 1e-9, "arranca en s=0.5, fue {x0}");
        assert!((x1 - 3.0).abs() < 1e-9, "termina en s=3.0, fue {x1}");
        assert!((y0 - 2.0).abs() < 0.05, "F(0.5)≈2, fue {y0}");
        assert!((y1 - 1.0 / 3.0).abs() < 0.05, "F(3)≈1/3, fue {y1}");
    }

    #[test]
    fn fourier_epiciclos_y_suma_coinciden() {
        // S_1(π/2) = 4/π exacto.
        let s1 = debe(suma_fourier(std::f64::consts::FRAC_PI_2, 1));
        assert!((s1 - 4.0 / std::f64::consts::PI).abs() < 1e-12, "S = {s1}");
        assert_eq!(debe(suma_fourier(0.0, 5)), 0.0);
        assert!(suma_fourier(1.0, 0).is_err());
        assert!(suma_fourier(1.0, 17).is_err());
        assert!(suma_fourier(f64::NAN, 3).is_err());
        // La altura de la punta es la suma (dos caminos, mismo valor).
        for t in [0.0, 0.7, 2.1, 5.0] {
            let centros = debe(epiciclos_en(t, 6));
            assert_eq!(centros.len(), 7);
            let punta = centros[6];
            let s = debe(suma_fourier(t, 6));
            assert!((punta[1] - s).abs() < 1e-12, "t = {t}");
        }
        // Más armónicos acercan el plateau (Gibbs aparte): S_8(π/2) ≈ 1.
        let s8 = debe(suma_fourier(std::f64::consts::FRAC_PI_2, 8));
        assert!((s8 - 1.0).abs() < 0.05, "S8 = {s8}");
        let mob = debe(mobjects_epiciclos(1.0, 4));
        assert_eq!(mob.len(), 9);
        let traza = debe(traza_fourier(3, 64));
        assert_eq!(traza.len(), 64);
    }

    #[test]
    fn calor_decae_y_onda_viaja() {
        // Dirichlet en bordes por construcción.
        assert_eq!(debe(calor_en(0.0, 0.5, 6)), 0.0);
        assert_eq!(debe(calor_en(1.0, 0.5, 6)), 0.0);
        // El centro se derrite: la suma truncada en t = 0 oscila cerca de 1
        // (Gibbs) y el decaimiento ∝ n² la lleva a 0 modo a modo. La energía
        // discreta Σu² decae monótono (cada modo decae por su cuenta).
        let u0 = debe(calor_en(0.5, 0.0, 6));
        assert!((u0 - 1.0).abs() < 0.1, "u(0.5, 0) = {u0}");
        let u1 = debe(calor_en(0.5, 0.5, 6));
        let u2 = debe(calor_en(0.5, 1.0, 6));
        let u3 = debe(calor_en(0.5, 2.0, 6));
        assert!(u1 > u2 && u2 > u3 && u3 > 0.0, "{u1} {u2} {u3}");
        let energia = |t: f64| {
            debe(perfil_calor_onda(false, t, 64, 6))
                .iter()
                .map(|p| p[1] * p[1])
                .sum::<f64>()
        };
        assert!(energia(1.0) < energia(0.5));
        assert!(calor_en(2.0, 0.1, 6).is_err());
        assert!(calor_en(0.5, 0.1, 0).is_err());
        // Onda: pulso unitario que se parte; simetría espejo exacta.
        assert_eq!(debe(onda_en(0.5, 0.0)), 1.0);
        assert!(debe(onda_en(0.5, 0.3)) < 0.9);
        for d in [0.05, 0.1, 0.2] {
            let (a, b) = (debe(onda_en(0.5 - d, 0.3)), debe(onda_en(0.5 + d, 0.3)));
            assert!((a - b).abs() < 1e-12, "simetría en d = {d}: {a} vs {b}");
        }
        assert!(onda_en(0.5, -1.0).is_err());
        // Perfil al mundo: x ∈ [-3, 3], y clamp a ±3.
        let p = debe(perfil_calor_onda(false, 0.2, 32, 4));
        assert_eq!(p.len(), 32);
        let mundo = perfil_a_mundo(&p);
        assert!((mundo[0][0] + 3.0).abs() < 1e-12);
        assert!((mundo[31][0] - 3.0).abs() < 1e-12);
        for q in &mundo {
            assert!(q[1].abs() <= 3.0);
        }
    }

    #[test]
    fn grilla_acotada_por_tres_topes() {
        assert_eq!(EDO_HEAT_MAX_CELDAS, 6144);
        let g = debe(grilla_calor_onda(false, 32, 12, 4, 1.0));
        assert_eq!((g.len(), g[0].len()), (12, 32));
        let g = debe(grilla_calor_onda(true, 128, 48, 1, 1.0));
        assert_eq!(g.len() * g[0].len(), 6144);
        assert!(grilla_calor_onda(false, 129, 12, 4, 1.0).is_err());
        assert!(grilla_calor_onda(false, 32, 49, 4, 1.0).is_err());
        assert!(grilla_calor_onda(false, 32, 12, 4, 0.0).is_err());
    }

    #[test]
    fn params_vivos_mezclan_y_validan() {
        let vacio = BTreeMap::new();
        let base = EdoParams::desde_mapa("edo-fourier-epiciclos", &vacio);
        assert_eq!(debe(base.terminos()), 6);
        let mut mapa = BTreeMap::new();
        mapa.insert(SCENE_PARAM_TERMS.to_string(), 3.0);
        mapa.insert(EDO_PARAM_NX.to_string(), 64.0);
        mapa.insert(EDO_PARAM_S.to_string(), 2.0);
        mapa.insert(EDO_PARAM_K.to_string(), f64::NAN);
        let p = EdoParams::desde_mapa("edo-laplace", &mapa);
        assert_eq!(debe(p.terminos()), 3);
        assert_eq!(debe(p.muestras()), 64);
        assert_eq!(debe(p.s_estrella()), 2.0);
        assert_eq!(debe(p.k_val()), 1.0);
        assert_eq!(debe(p.t_reservado()), 0.0);
        assert!(!debe(p.es_onda()));
        mapa.insert(SCENE_PARAM_TERMS.to_string(), 99.0);
        assert!(EdoParams::desde_mapa("x", &mapa).terminos().is_err());
        let mut mala = EdoParams::por_defecto("edo-calor-onda");
        mala.modo = 2.0;
        assert!(mala.es_onda().is_err());
        mala.modo = 1.0;
        assert!(debe(mala.es_onda()));
        assert_eq!(debe(mala.nt()), 1);
        assert_eq!(debe(mala.nx_espacial()), 64);
        let campo = EdoParams::por_defecto("edo-campo-direcciones");
        assert_eq!(debe(campo.reticula()), (13, 9));
        assert_eq!(debe(campo.modo_campo()), CampoEdo::Exponencial);
    }

    #[test]
    fn escena_para_cubre_las_cinco_y_rechaza_resto() {
        let mut scratch = Scratch::nuevo();
        for id in TEMPLATE_IDS {
            let params = EdoParams::por_defecto(id);
            let frame = debe(escena_para(id, &params, 0.5, &mut scratch));
            assert!(!frame.is_empty(), "{id} sin objetos");
            for colocado in &frame {
                match colocado.mobject.validate() {
                    Ok(()) => {}
                    Err(e) => panic!("{id}: mobject inválido: {e:?}"),
                }
                assert!((0.0..=1.0).contains(&colocado.opacity));
            }
            // alpha no finito no paniquea ni inventa: equivale a 0.
            let cero = debe(escena_para(id, &params, 0.0, &mut scratch));
            let nan = debe(escena_para(id, &params, f64::NAN, &mut scratch));
            assert_eq!(cero.len(), nan.len(), "{id}");
        }
        let params = EdoParams::por_defecto("edo-campo-direcciones");
        assert!(escena_para("no-existe", &params, 0.5, &mut scratch).is_err());
        // Campo con retícula máxima: 4096 flechas + 3 curvas, todo válido.
        let mut grande = EdoParams::por_defecto("edo-campo-direcciones");
        grande.nx = 64.0;
        grande.ny = 64.0;
        let frame = debe(escena_para(
            "edo-campo-direcciones",
            &grande,
            1.0,
            &mut scratch,
        ));
        assert_eq!(frame.len(), 4096 + 3);
    }
}
