//! Director de guion: contrato LLM → player (F2 slice 1).
//!
//! El LLM emite [`GuionTexto`] (todo String/números, serializable); la
//! validación vive en [`Guion::try_new`] y produce [`Guion`] con
//! [`Scene`], [`Resolution`] y [`AnimDuration`] tipados. El lowering a
//! [`PlayItem`] es puro ([`compilar_paso`]) y NUNCA se expone `PlayItem`
//! crudo al LLM.
//!
//! Presupuestos (heredados del crate, no inventados):
//! - actos `1..=GUION_MAX_ACTOS`, pasos por acto `1..=ACTO_MAX_PASOS`
//! - pasos aplanados `<= PLAYLIST_MAX_STEPS` (8, reuso directo)
//! - frames por paso `4..=16`, total `<= 96` (`PLAYLIST_MAX_FRAMES_TOTAL`)
//! - set `w*h*4*total <= 64 MiB` (paridad con `ScenePlayer::try_play`)
//! - `Resolution` único por guion, `run 100..=60000 ms` (P0 long-form),
//!   `wait_after 0..=10000 ms`, concepto `<= 500` chars
//! - template solo de [`CANONICAL_TEMPLATES`] (saneado, alias histórico
//!   `pythagoras → pitagoras`)
//! - `math_expr` inválida → `None` (no `Err`: el paso se conserva); el
//!   CAS-gate real lo hace pedagogía después ([`math_expr_valida_para_cas`]
//!   es solo el hook de forma).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::player::{
    CreateAnim, FadeAnim, GrowFromCenterAnim, IndicateAnim, PlayItem, TrackerMap,
    UpdateFromTracker, WaitAnim, WriteAnim,
};
use crate::protocol::{
    AnimDuration, AnimRequest, AnimationGroup, ExportFormat, Playlist, PlaylistStep, Resolution,
    CANONICAL_TEMPLATES, PLAYLIST_MAX_FRAMES_TOTAL, PLAYLIST_MAX_STEPS, PLAYLIST_MAX_WAIT_MS,
};
use crate::scene::{Mobject, Ortho, RateFunc, Scene, MAX_SCENE_LAYERS};

/// Actos por guion: `1..=GUION_MAX_ACTOS`.
pub const GUION_MAX_ACTOS: usize = 5;
/// Mínimo de actos (un guion vacío no es guion).
pub const GUION_MIN_ACTOS: usize = 1;
/// Pasos por acto: `1..=ACTO_MAX_PASOS`.
pub const ACTO_MAX_PASOS: usize = 3;
/// Frames por paso: `PASO_MIN_FRAMES..=PASO_MAX_FRAMES`.
pub const PASO_MIN_FRAMES: usize = 4;
/// Frames por paso: `PASO_MIN_FRAMES..=PASO_MAX_FRAMES`.
pub const PASO_MAX_FRAMES: usize = 16;
/// Tope de memoria estimada del set (`w*h*4*total`, paridad player).
pub const GUION_MAX_MEMORIA_BYTES: usize = 64 * 1024 * 1024;
/// Título de acto: `<= 80` chars.
pub const TITULO_MAX_CHARS: usize = 80;
/// Concepto del guion y texto del paso: `<= 500` chars.
pub const CONCEPTO_MAX_CHARS: usize = 500;
/// `math_expr`: corta, `<= 200` chars (forma local; el CAS decide después).
pub const MATH_EXPR_MAX_CHARS: usize = 200;
/// Pista de pizarra: `<= 200` chars.
pub const WHITEBOARD_MAX_CHARS: usize = 200;
/// Entradas máximas en `params` (anti-OOM de wire).
pub const PARAMS_MAX_ENTRIES: usize = 16;
/// Clave de `params`: `<= 64` chars.
pub const PARAMS_MAX_KEY_CHARS: usize = 64;
/// `run_ms` de paso animado (paridad `AnimDuration` 0.1..=60 s, P0 long-form).
pub const PASO_MIN_RUN_MS: u64 = 100;
/// `run_ms` de paso animado (paridad `AnimDuration` 0.1..=60 s, P0 long-form).
pub const PASO_MAX_RUN_MS: u64 = 60_000;
/// Fondo por defecto de la base de acto (blanco).
pub const FONDO_POR_DEFECTO: [u8; 3] = [255, 255, 255];

/// Error honesto del director (todo en español, sin pánicos).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GuionError {
    /// Guion/acto/paso mal formado (campo + motivo).
    #[error("guion inválido ({campo}): {motivo}")]
    Invalido {
        /// Campo que falló (`actos`, `pasos`, `template`, …).
        campo: &'static str,
        /// Motivo en español.
        motivo: String,
    },
    /// La bajada a [`PlayItem`] falló (constructor del player).
    #[error("bajada a player falló: {motivo}")]
    Bajada {
        /// Motivo en español (viene del `SceneError`).
        motivo: String,
    },
}

impl From<crate::scene::SceneError> for GuionError {
    fn from(e: crate::scene::SceneError) -> Self {
        Self::Bajada {
            motivo: e.to_string(),
        }
    }
}

impl From<crate::protocol::ProtocolError> for GuionError {
    fn from(e: crate::protocol::ProtocolError) -> Self {
        Self::Invalido {
            campo: "viewport",
            motivo: e.to_string(),
        }
    }
}

fn invalido(campo: &'static str, motivo: String) -> GuionError {
    GuionError::Invalido { campo, motivo }
}

/// Efecto de un paso (lo único que elige el LLM; el `PlayItem` lo arma
/// [`compilar_paso`]). Serializable para el wire del guion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EfectoPaso {
    /// Traza progresiva.
    Create,
    /// Revelado de texto.
    Write,
    /// Aparición por alfa.
    Fade,
    /// Escala 0→1 desde el centroide.
    Grow,
    /// Pulso ×1.2.
    Indicate,
    /// Barrido de tracker.
    Tracker,
    /// Espera silenciosa.
    Wait,
}

impl EfectoPaso {
    /// Parsea el efecto del LLM (minúsculas, con alias en español).
    /// `Err` honesto si no matchea.
    pub fn parsear(raw: &str) -> Result<Self, GuionError> {
        match raw.trim().to_lowercase().as_str() {
            "create" | "crear" | "traza" => Ok(Self::Create),
            "write" | "escribir" | "texto" => Ok(Self::Write),
            "fade" | "aparecer" => Ok(Self::Fade),
            "grow" | "crecer" => Ok(Self::Grow),
            "indicate" | "indicar" | "pulso" => Ok(Self::Indicate),
            "tracker" => Ok(Self::Tracker),
            "wait" | "espera" | "pausa" => Ok(Self::Wait),
            otro => Err(invalido(
                "efecto",
                format!(
                    "efecto desconocido {otro:?}: usá create|write|fade|grow|indicate|tracker|wait"
                ),
            )),
        }
    }

    /// ¿Es espera silenciosa?
    pub fn es_espera(self) -> bool {
        matches!(self, Self::Wait)
    }
}

/// Frontera entre actos (evita saturar las 32 capas):
/// o se conserva lo dibujado o se limpia a la base del acto que arranca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frontera {
    /// Seguir dibujando sobre las capas actuales.
    Conservar,
    /// Limpiar a la base del acto ([`aplicar_frontera`]).
    Limpiar,
}

/// Escena base de un acto: cámara canónica + capas iniciales + fondo.
///
/// La frontera [`Frontera::Limpiar`] restaura exactamente esta escena.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscenaBase {
    /// Escena inicial del acto (1..=32 capas validadas).
    pub escena: Scene,
}

impl EscenaBase {
    /// Constructor validado (re-valida vía `Scene::try_new`: la
    /// deserialización puede bypasear los límites).
    pub fn try_new(escena: Scene) -> Result<Self, GuionError> {
        let escena = Scene::try_new(escena.camera, escena.layers, escena.bg)
            .map_err(|e| invalido("escena", format!("base inválida: {e}")))?;
        Ok(Self { escena })
    }

    /// Base por defecto: cámara 16:9 + ejes + fondo dado.
    pub fn por_defecto(bg: [u8; 3]) -> Self {
        Self {
            escena: Scene {
                camera: Ortho::default_16_9(),
                layers: vec![Mobject::Axes],
                bg,
            },
        }
    }

    /// Cantidad de capas de la base.
    pub fn capas(&self) -> usize {
        self.escena.layers.len()
    }
}

/// Hook de forma para `math_expr`: valida sintaxis local barata.
///
/// Hoy es SOLO forma (no corre el CAS): no vacía, `<= 200` chars, sin
/// `=∫Σ→`, sin controles, paréntesis balanceados y al menos un
/// alfanumérico. El CAS-gate real lo hace pedagogía después.
pub fn math_expr_valida_para_cas(expr: &str) -> bool {
    let t = expr.trim();
    if t.is_empty() || t.chars().count() > MATH_EXPR_MAX_CHARS {
        return false;
    }
    if t.contains(['=', '∫', 'Σ', '→']) {
        return false;
    }
    if t.chars().any(|c| c.is_control()) {
        return false;
    }
    // Balanceo exacto (sin pánicos: `t` está acotado a 200 chars, el
    // `i32` no puede desbordar con ese largo).
    let mut n: i32 = 0;
    for c in t.chars() {
        if c == '(' {
            n += 1;
        } else if c == ')' {
            n -= 1;
            if n < 0 {
                return false;
            }
        }
    }
    if n != 0 {
        return false;
    }
    t.chars().any(|c| c.is_alphanumeric())
}

/// Sanea `math_expr` del LLM: `None` entra → `None` sale; inválida →
/// `None` (NO es `Err`: el paso se conserva sin matemática).
pub fn sanear_math_expr(raw: Option<String>) -> Option<String> {
    raw.and_then(|e| {
        let t = e.trim().to_string();
        if math_expr_valida_para_cas(&t) {
            Some(t)
        } else {
            None
        }
    })
}

/// Sanea `template_hint` a canónico; fuera de la allowlist es `Err`.
fn sanear_template(hint: &str) -> Result<String, GuionError> {
    let t = hint.trim().to_lowercase();
    // Alias histórico (paridad con `sanitize_template` del protocolo).
    let t = if t == "pythagoras" {
        "pitagoras".to_string()
    } else {
        t
    };
    if CANONICAL_TEMPLATES.contains(&t.as_str()) {
        Ok(t)
    } else {
        Err(invalido(
            "template",
            format!("{hint:?} no está en la lista canónica: elegí una de {CANONICAL_TEMPLATES:?}"),
        ))
    }
}

/// Valida `params`: `<= 16` entradas, clave `1..=64` chars, valor finito.
fn sanear_params(raw: BTreeMap<String, f64>) -> Result<BTreeMap<String, f64>, GuionError> {
    if raw.len() > PARAMS_MAX_ENTRIES {
        return Err(invalido(
            "params",
            format!("{} entradas (válido 0..={PARAMS_MAX_ENTRIES})", raw.len()),
        ));
    }
    let mut out = BTreeMap::new();
    for (k, v) in raw {
        let k = k.trim().to_string();
        if k.is_empty() || k.chars().count() > PARAMS_MAX_KEY_CHARS {
            return Err(invalido(
                "params",
                format!("clave {k:?} fuera de 1..={PARAMS_MAX_KEY_CHARS} chars"),
            ));
        }
        if !v.is_finite() {
            return Err(invalido("params", format!("{k} no es finito")));
        }
        out.insert(k, v);
    }
    Ok(out)
}

/// Paso tal cual lo emite el LLM (todo String/números, serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasoTexto {
    /// Narración del paso (`1..=500` chars).
    pub texto: String,
    /// Matemática opcional (inválida → `None`, sin `Err`).
    pub math_expr: Option<String>,
    /// Pista de pizarra (`<= 200` chars, puede vacía).
    pub whiteboard_hint: String,
    /// Plantilla (solo canónicas, saneada).
    pub template_hint: String,
    /// Parámetros numéricos finitos.
    pub params: BTreeMap<String, f64>,
    /// Efecto como string (`create|write|fade|grow|indicate|tracker|wait`).
    pub efecto: String,
    /// Frames del paso (`4..=16`).
    pub frames: usize,
    /// Corrida en ms (animado `100..=60000` P0 long-form; espera `1..=10000`).
    pub run_ms: u64,
    /// Silencio posterior (`0..=10000`; la espera sola exige `0`).
    pub wait_after_ms: u64,
}

/// Acto tal cual lo emite el LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActoTexto {
    /// Título (`1..=80` chars).
    pub titulo: String,
    /// Fondo de la base (`None` = blanco).
    pub fondo: Option<[u8; 3]>,
    /// `true` = limpiar a la base al arrancar el acto.
    pub limpiar: bool,
    /// Pasos (`1..=ACTO_MAX_PASOS`).
    pub pasos: Vec<PasoTexto>,
}

/// Guion tal cual lo emite el LLM (serializable, sin tipos del player).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuionTexto {
    /// Concepto (`<= 500` chars; vacío = "matemática").
    pub concepto: String,
    /// Ancho del viewport (único para todo el guion, `64..=4096`).
    pub width: u32,
    /// Alto del viewport (único para todo el guion, `64..=4096`).
    pub height: u32,
    /// Actos (`1..=GUION_MAX_ACTOS`).
    pub actos: Vec<ActoTexto>,
}

/// Paso validado (salida de la validación, entrada de [`compilar_paso`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasoGuion {
    /// Narración saneada.
    pub texto: String,
    /// Matemática saneada (`None` si el LLM mandó inválida: el paso sigue).
    pub math_expr: Option<String>,
    /// Pista de pizarra saneada.
    pub whiteboard_hint: String,
    /// Plantilla canónica.
    pub template: String,
    /// Parámetros finitos.
    pub params: BTreeMap<String, f64>,
    /// Efecto tipado.
    pub efecto: EfectoPaso,
    /// Frames (`4..=16`).
    pub frames: usize,
    /// Corrida en ms.
    pub run_ms: u64,
    /// Silencio posterior en ms.
    pub wait_after_ms: u64,
}

impl PasoGuion {
    /// Valida un paso del LLM (todo `Err` en español; `math_expr`
    /// inválida → `None`, nunca `Err`).
    pub fn try_new(raw: PasoTexto) -> Result<Self, GuionError> {
        let texto = raw.texto.trim().to_string();
        if texto.is_empty() || texto.chars().count() > CONCEPTO_MAX_CHARS {
            return Err(invalido(
                "texto",
                format!(
                    "texto de {} chars (válido 1..={CONCEPTO_MAX_CHARS})",
                    texto.chars().count()
                ),
            ));
        }
        let whiteboard_hint = raw.whiteboard_hint.trim().to_string();
        if whiteboard_hint.chars().count() > WHITEBOARD_MAX_CHARS {
            return Err(invalido(
                "whiteboard_hint",
                format!(
                    "pista de {} chars (válido 0..={WHITEBOARD_MAX_CHARS})",
                    whiteboard_hint.chars().count()
                ),
            ));
        }
        let template = sanear_template(&raw.template_hint)?;
        let params = sanear_params(raw.params)?;
        let efecto = EfectoPaso::parsear(&raw.efecto)?;
        if !(PASO_MIN_FRAMES..=PASO_MAX_FRAMES).contains(&raw.frames) {
            return Err(invalido(
                "frames",
                format!(
                    "pediste {} fotogramas (válido {PASO_MIN_FRAMES}..={PASO_MAX_FRAMES})",
                    raw.frames
                ),
            ));
        }
        if raw.wait_after_ms > PLAYLIST_MAX_WAIT_MS {
            return Err(invalido(
                "wait_after_ms",
                format!(
                    "{} excede el máximo de {PLAYLIST_MAX_WAIT_MS}",
                    raw.wait_after_ms
                ),
            ));
        }
        if efecto.es_espera() {
            // Paridad con `PlaylistStep::pausa`: la espera sola no lleva
            // silencio posterior y su corrida es `1..=10000`.
            if raw.wait_after_ms != 0 {
                return Err(invalido(
                    "wait_after_ms",
                    "la espera sola no lleva silencio posterior: usá otro paso".to_string(),
                ));
            }
            if raw.run_ms == 0 || raw.run_ms > PLAYLIST_MAX_WAIT_MS {
                return Err(invalido(
                    "run_ms",
                    format!(
                        "espera de {} fuera de 1..={PLAYLIST_MAX_WAIT_MS}",
                        raw.run_ms
                    ),
                ));
            }
        } else if !(PASO_MIN_RUN_MS..=PASO_MAX_RUN_MS).contains(&raw.run_ms) {
            return Err(invalido(
                "run_ms",
                format!(
                    "{} fuera de {PASO_MIN_RUN_MS}..={PASO_MAX_RUN_MS}",
                    raw.run_ms
                ),
            ));
        }
        Ok(Self {
            texto,
            math_expr: sanear_math_expr(raw.math_expr),
            whiteboard_hint,
            template,
            params,
            efecto,
            frames: raw.frames,
            run_ms: raw.run_ms,
            wait_after_ms: raw.wait_after_ms,
        })
    }

    /// Hook CAS: ¿la matemática del paso es apta para el gate de
    /// pedagogía? (hoy = forma local; el CAS real decide después).
    pub fn math_expr_valida_para_cas(&self) -> bool {
        self.math_expr
            .as_deref()
            .is_some_and(math_expr_valida_para_cas)
    }
}

/// Acto validado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acto {
    /// Título saneado.
    pub titulo: String,
    /// Base de escena del acto.
    pub escena: EscenaBase,
    /// Pasos validados (`1..=ACTO_MAX_PASOS`).
    pub pasos: Vec<PasoGuion>,
    /// Frontera al arrancar el acto.
    pub frontera: Frontera,
}

impl Acto {
    /// Valida un acto del LLM.
    pub fn try_new(raw: ActoTexto) -> Result<Self, GuionError> {
        let titulo = raw.titulo.trim().to_string();
        if titulo.is_empty() || titulo.chars().count() > TITULO_MAX_CHARS {
            return Err(invalido(
                "titulo",
                format!(
                    "título de {} chars (válido 1..={TITULO_MAX_CHARS})",
                    titulo.chars().count()
                ),
            ));
        }
        if raw.pasos.is_empty() || raw.pasos.len() > ACTO_MAX_PASOS {
            return Err(invalido(
                "pasos",
                format!(
                    "acto con {} pasos (válido 1..={ACTO_MAX_PASOS})",
                    raw.pasos.len()
                ),
            ));
        }
        let mut pasos = Vec::with_capacity(raw.pasos.len());
        for p in raw.pasos {
            pasos.push(PasoGuion::try_new(p)?);
        }
        let escena = EscenaBase::por_defecto(raw.fondo.unwrap_or(FONDO_POR_DEFECTO));
        let frontera = if raw.limpiar {
            Frontera::Limpiar
        } else {
            Frontera::Conservar
        };
        Ok(Self {
            titulo,
            escena,
            pasos,
            frontera,
        })
    }
}

/// Comprueba que todas las resoluciones son la misma (viewport único) y la
/// devuelve tipada. Mezcla → `Err` honesto.
///
/// B4.4: el [`Guion`] NO la usa en `try_new` — su `GuionTexto` trae un solo
/// par (width, height) por construcción y el mixto es imposible ahí. Esta
/// función vive para la composición multi-fuente (varias resoluciones de
/// pasos/actos sueltos reunidos por el llamador), donde la divergencia sí
/// puede existir.
pub fn comprobar_viewport_unico(resoluciones: &[(u32, u32)]) -> Result<Resolution, GuionError> {
    let Some(primera) = resoluciones.first() else {
        return Err(invalido(
            "viewport",
            "sin resoluciones: pasame al menos 1".to_string(),
        ));
    };
    for r in resoluciones {
        if r != primera {
            return Err(invalido(
                "viewport",
                format!(
                    "viewport mixto: {}x{} vs {}x{} (el guion usa una sola resolución)",
                    r.0, r.1, primera.0, primera.1
                ),
            ));
        }
    }
    Ok(Resolution::try_new(primera.0, primera.1)?)
}

/// Guion validado: concepto + actos + [`Scene`] + [`Resolution`] +
/// [`AnimDuration`] tipados. Solo se construye vía [`Guion::try_new`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guion {
    /// Concepto normalizado (`<= 500` chars).
    pub concepto: String,
    /// Actos validados.
    pub actos: Vec<Acto>,
    /// Escena inicial (base del primer acto).
    pub escena: Scene,
    /// Resolución única del guion.
    pub resolution: Resolution,
    /// Duración total (suma `run + wait_after`, `0.1..=60 s` P0 long-form).
    pub duracion: AnimDuration,
}

impl Guion {
    /// Valida el guion del LLM (todo `Err` en español, sin pánicos).
    pub fn try_new(raw: GuionTexto) -> Result<Self, GuionError> {
        if raw.actos.len() < GUION_MIN_ACTOS || raw.actos.len() > GUION_MAX_ACTOS {
            return Err(invalido(
                "actos",
                format!(
                    "guion con {} actos (válido {GUION_MIN_ACTOS}..={GUION_MAX_ACTOS})",
                    raw.actos.len()
                ),
            ));
        }
        // B4.4: viewport único POR CONSTRUCCIÓN — `GuionTexto` trae un solo
        // par (width, height) para todo el guion, así que el viewport mixto
        // es imposible acá (el chequeo con 1 elemento siempre pasaba y era
        // código muerto). La detección multi-viewport vive donde sí puede
        // divergir: `comprobar_viewport_unico` (composición multi-fuente) y
        // `AnimationGroup::plan_remuestreo` (viewport mixto → `Err`).
        let resolution = Resolution::try_new(raw.width, raw.height)?;
        let mut actos = Vec::with_capacity(raw.actos.len());
        for a in raw.actos {
            actos.push(Acto::try_new(a)?);
        }
        let total_pasos: usize = actos.iter().map(|a| a.pasos.len()).sum();
        if total_pasos > PLAYLIST_MAX_STEPS {
            return Err(invalido(
                "pasos",
                format!(
                    "{total_pasos} pasos exceden el tope de {PLAYLIST_MAX_STEPS}: partí el guion en dos"
                ),
            ));
        }
        let total_frames: usize = actos.iter().flat_map(|a| &a.pasos).map(|p| p.frames).sum();
        if total_frames > PLAYLIST_MAX_FRAMES_TOTAL {
            return Err(invalido(
                "frames",
                format!(
                    "el total {total_frames} excede {PLAYLIST_MAX_FRAMES_TOTAL}: bajá pasos o fotogramas"
                ),
            ));
        }
        // Presupuesto de memoria del set (paridad con el player: 64 MiB).
        // `u32 → usize` vía `try_from` (sin `as`, sin truncar en 16-bit).
        let pixeles = usize::try_from(resolution.width)
            .ok()
            .and_then(|w| usize::try_from(resolution.height).ok().map(|h| (w, h)))
            .and_then(|(w, h)| w.checked_mul(h))
            .and_then(|p| p.checked_mul(4))
            .and_then(|p| p.checked_mul(total_frames));
        match pixeles {
            Some(bytes) if bytes <= GUION_MAX_MEMORIA_BYTES => {}
            _ => {
                return Err(invalido(
                    "memoria",
                    format!(
                        "el set {}x{}x4x{total_frames} excede 64 MiB: bajá resolución o fotogramas",
                        resolution.width, resolution.height
                    ),
                ));
            }
        }
        let total_ms: u64 = actos
            .iter()
            .flat_map(|a| &a.pasos)
            .map(|p| p.run_ms.saturating_add(p.wait_after_ms))
            .fold(0, |acc, d| acc.saturating_add(d));
        let secs = total_ms as f64 / 1000.0;
        let duracion = AnimDuration::try_new(secs)
            .map_err(|e| invalido("duracion", format!("total {total_ms} ms inválido: {e}")))?;
        let concepto = crate::protocol::normalize_concept(&raw.concepto);
        // La escena inicial es la base del primer acto (hay ≥1 por el
        // chequeo de actos de arriba).
        let escena = actos
            .first()
            .map(|a| a.escena.escena.clone())
            .ok_or_else(|| {
                invalido(
                    "actos",
                    "sin actos tras validar: no debería pasar".to_string(),
                )
            })?;
        Ok(Self {
            concepto,
            actos,
            escena,
            resolution,
            duracion,
        })
    }

    /// Concepto normalizado.
    pub fn concepto(&self) -> &str {
        &self.concepto
    }

    /// Actos validados.
    pub fn actos(&self) -> &[Acto] {
        &self.actos
    }

    /// Resolución única.
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    /// Duración total.
    pub fn duracion(&self) -> AnimDuration {
        self.duracion
    }

    /// Escena inicial (base del primer acto).
    pub fn escena(&self) -> &Scene {
        &self.escena
    }

    /// Pasos aplanados totales.
    pub fn total_pasos(&self) -> usize {
        self.actos.iter().map(|a| a.pasos.len()).sum()
    }

    /// Frames totales del guion.
    pub fn total_frames(&self) -> usize {
        self.actos
            .iter()
            .flat_map(|a| &a.pasos)
            .map(|p| p.frames)
            .sum()
    }

    /// Duración total en ms (`run + wait_after` saturada).
    pub fn duracion_total_ms(&self) -> u64 {
        self.actos
            .iter()
            .flat_map(|a| &a.pasos)
            .map(|p| p.run_ms.saturating_add(p.wait_after_ms))
            .fold(0, |acc, d| acc.saturating_add(d))
    }

    /// Baja todo el guion a items del player (puro, en orden de actos).
    /// La frontera la aplica el llamador con [`aplicar_frontera`].
    ///
    /// B4.5: paridad con [`Guion::a_playlist`] — si los pasos animados
    /// traen N distinto se valida el remuestreo al máximo vía
    /// [`AnimationGroup::plan_remuestreo`] (vecino más cercano, viewport
    /// mixto → `Err`, jamás reescaleo silencioso). Las esperas no tienen
    /// set que componer y se excluyen igual que en `a_playlist`.
    pub fn items_para(&self, escena: &Scene) -> Result<Vec<PlayItem>, GuionError> {
        let animados: Vec<&PasoGuion> = self
            .actos
            .iter()
            .flat_map(|a| a.pasos.iter())
            .filter(|p| !p.efecto.es_espera())
            .collect();
        if animados.len() >= 2
            && animados
                .iter()
                .map(|p| p.frames)
                .collect::<Vec<_>>()
                .windows(2)
                .any(|w| w[0] != w[1])
        {
            let (ancho, alto) = self.resolution.as_tuple();
            let frames: Vec<usize> = animados.iter().map(|p| p.frames).collect();
            let tamanos = vec![(ancho as usize, alto as usize); animados.len()];
            let indices: Vec<usize> = (0..animados.len()).collect();
            let grupo = AnimationGroup::try_new(indices, 0.0)
                .map_err(|e| invalido("frames", format!("remuestreo imposible: {e}")))?;
            grupo
                .plan_remuestreo(&frames, &tamanos)
                .map_err(|e| invalido("frames", format!("remuestreo imposible: {e}")))?;
        }
        let mut out = Vec::with_capacity(self.total_pasos());
        for acto in &self.actos {
            for paso in &acto.pasos {
                out.push(compilar_paso(paso, escena)?);
            }
        }
        Ok(out)
    }

    /// Baja el guion a [`Playlist`] estilo Manim (compilador acto→playlist).
    ///
    /// Aplana los actos en orden (`<= PLAYLIST_MAX_STEPS` steps): cada paso
    /// animado arma su [`AnimRequest`] (`template` canónico + `params` del
    /// paso + `canvas` único + `duration_ms = run_ms`); cada espera (`Wait`)
    /// baja a [`PlaylistStep::pausa`] (silencio puro, 0 frames propios — por
    /// eso se construye paso a paso en vez de `build_animations_with_timings`,
    /// que no expresa pausas; la construcción es equivalente: mismos rangos
    /// `run 100..=60000` (P0) / `wait 0..=10000` y `Playlist::try_new` final).
    /// Si dos pasos animados traen N distinto se valida el remuestreo al
    /// máximo vía [`AnimationGroup::plan_remuestreo`] (vecino más cercano,
    /// sin inventar píxeles; viewport mixto → `Err`, jamás reescaleo
    /// silencioso). Presupuesto: `total <= 96` (`validate_frame_counts`) y
    /// `estimate_set_bytes <= 64 MiB`, si no `Err` honesto en español.
    /// Puro, sin I/O, sin pánicos (todo `Err` es `String` en español porque
    /// este compilador cruza al adaptador de pedagogía sin acoplar errores
    /// tipados).
    pub fn a_playlist(&self) -> Result<Playlist, String> {
        let pasos: Vec<&PasoGuion> = self.actos.iter().flat_map(|a| a.pasos.iter()).collect();
        if pasos.is_empty() {
            return Err("guion sin pasos: pasame al menos 1 paso".to_string());
        }
        if pasos.len() > PLAYLIST_MAX_STEPS {
            return Err(format!(
                "{} pasos exceden el tope de {PLAYLIST_MAX_STEPS}: partí el guion en dos",
                pasos.len()
            ));
        }
        let (ancho, alto) = self.resolution.as_tuple();
        let mut steps = Vec::with_capacity(pasos.len());
        for paso in &pasos {
            if paso.efecto.es_espera() {
                steps.push(
                    PlaylistStep::pausa(paso.run_ms).map_err(|e| format!("pausa inválida: {e}"))?,
                );
            } else {
                let request = AnimRequest {
                    template: paso.template.clone(),
                    concept: self.concepto.clone(),
                    params: paso.params.clone(),
                    spec: None,
                    export: ExportFormat::Gif,
                    canvas: (ancho, alto),
                    duration_ms: paso.run_ms,
                };
                steps.push(
                    PlaylistStep::anim(request, paso.run_ms, paso.wait_after_ms)
                        .map_err(|e| format!("step inválido: {e}"))?,
                );
            }
        }
        let lista = Playlist::try_new(steps).map_err(|e| format!("playlist inválida: {e}"))?;
        // La pausa aporta 0 frames propios (congela el último); el animado
        // aporta los suyos. Así `validate_frame_counts` ve pausas en 0.
        let frames: Vec<usize> = pasos
            .iter()
            .map(|p| if p.efecto.es_espera() { 0 } else { p.frames })
            .collect();
        let tamanos: Vec<(usize, usize)> = vec![(ancho as usize, alto as usize); pasos.len()];
        // N distinto entre animados → remuestreo al máximo (vecino más
        // cercano). Solo sobre animados (la pausa no tiene set que componer).
        let animados: Vec<usize> = frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| if *f > 0 { Some(i) } else { None })
            .collect();
        let n_distinto = animados
            .iter()
            .map(|i| frames[*i])
            .collect::<Vec<_>>()
            .windows(2)
            .any(|w| w[0] != w[1]);
        if n_distinto && animados.len() >= 2 {
            let grupo = AnimationGroup::try_new(animados, 0.0)
                .map_err(|e| format!("grupo inválido: {e}"))?;
            grupo
                .plan_remuestreo(&frames, &tamanos)
                .map_err(|e| format!("remuestreo imposible: {e}"))?;
        }
        let total = lista
            .validate_frame_counts(&frames)
            .map_err(|e| format!("frames inválidos: {e}"))?;
        match Playlist::estimate_set_bytes(ancho as usize, alto as usize, total) {
            Some(bytes) if bytes <= GUION_MAX_MEMORIA_BYTES => Ok(lista),
            Some(bytes) => Err(format!(
                "el set ({bytes} bytes) excede 64 MiB: bajá resolución o fotogramas"
            )),
            None => Err(
                "el set desborda el contador (tope 64 MiB): bajá resolución o fotogramas"
                    .to_string(),
            ),
        }
    }
}

/// Aplica la frontera del acto a la escena viva (puro, sin I/O):
/// `Conservar` no toca nada; `Limpiar` restaura la base del acto.
pub fn aplicar_frontera(escena: &mut Scene, acto: &Acto) -> Result<(), GuionError> {
    match acto.frontera {
        Frontera::Conservar => {
            if escena.layers.is_empty() || escena.layers.len() > MAX_SCENE_LAYERS {
                return Err(invalido(
                    "escena",
                    format!(
                        "{} capas (válido 1..={MAX_SCENE_LAYERS})",
                        escena.layers.len()
                    ),
                ));
            }
            Ok(())
        }
        Frontera::Limpiar => {
            escena.camera = acto.escena.escena.camera;
            escena.layers = acto.escena.escena.layers.clone();
            escena.bg = acto.escena.escena.bg;
            if escena.layers.is_empty() || escena.layers.len() > MAX_SCENE_LAYERS {
                return Err(invalido(
                    "escena",
                    format!(
                        "base con {} capas (válido 1..={MAX_SCENE_LAYERS})",
                        escena.layers.len()
                    ),
                ));
            }
            Ok(())
        }
    }
}

/// Escapa texto para el SVG opaco del `Write` (sin parser de trazos).
fn escapar_svg(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Mobject base para el lowering: la matemática válida manda
/// (`FunctionGraph` tal cual, nunca inventada); si no, la primera capa
/// de la escena viva; si no hay, un punto honesto en el origen.
fn mobject_base(paso: &PasoGuion, escena: &Scene) -> Mobject {
    if let Some(expr) = paso.math_expr.as_deref() {
        return Mobject::FunctionGraph {
            expr: expr.to_string(),
        };
    }
    escena
        .layers
        .first()
        .cloned()
        .unwrap_or(Mobject::Dot { x: 0.0, y: 0.0 })
}

/// Polilínea para `Create`: la del mobject si es polígono, si no un
/// cuadrado canónico (traza visible garantizada).
fn poly_para(base: &Mobject) -> Vec<[f64; 2]> {
    if let Mobject::Polygon { pts } = base {
        if pts.len() >= 2 {
            return pts.clone();
        }
    }
    vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, -1.0]]
}

/// Texto opaco para `Write`: el `Tex` de la escena si hay uno, si no un
/// SVG mínimo con el texto del paso escapado.
fn tex_para(paso: &PasoGuion, escena: &Scene) -> Mobject {
    for capa in &escena.layers {
        if matches!(capa, Mobject::Tex { .. }) {
            return capa.clone();
        }
    }
    Mobject::Tex {
        svg: format!("<svg><text>{}</text></svg>", escapar_svg(&paso.texto)),
    }
}

/// Baja un paso validado a un item del player (puro, sin I/O).
///
/// NO se expone `PlayItem` crudo al LLM: el LLM solo elige [`EfectoPaso`]
/// y este lowering lo materializa con la escena viva como contexto.
pub fn compilar_paso(paso: &PasoGuion, escena: &Scene) -> Result<PlayItem, GuionError> {
    let tasa = RateFunc::Smooth;
    let base = mobject_base(paso, escena);
    let item = match paso.efecto {
        EfectoPaso::Create => {
            let poly = poly_para(&base);
            PlayItem::Create(CreateAnim::try_new(
                poly,
                paso.frames,
                paso.run_ms,
                tasa,
                false,
            )?)
        }
        EfectoPaso::Write => {
            let tex = tex_para(paso, escena);
            PlayItem::Write(WriteAnim::try_new(tex, paso.frames, paso.run_ms, tasa)?)
        }
        EfectoPaso::Fade => PlayItem::Fade(FadeAnim::try_new(
            base,
            true,
            paso.frames,
            paso.run_ms,
            tasa,
        )?),
        EfectoPaso::Grow => PlayItem::GrowFromCenter(GrowFromCenterAnim::try_new(
            base,
            paso.frames,
            paso.run_ms,
            tasa,
        )?),
        EfectoPaso::Indicate => {
            PlayItem::Indicate(IndicateAnim::try_new(base, paso.frames, paso.run_ms, tasa)?)
        }
        EfectoPaso::Tracker => PlayItem::Tracker(UpdateFromTracker::try_new(
            base,
            0.0,
            1.0,
            paso.frames,
            paso.run_ms,
            tasa,
            TrackerMap::Opacity { lo: 0.0, hi: 1.0 },
        )?),
        EfectoPaso::Wait => PlayItem::Wait(WaitAnim::try_new(paso.frames)?),
    };
    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paso_texto(efecto: &str, frames: usize) -> PasoTexto {
        PasoTexto {
            texto: "derivada paso a paso".to_string(),
            math_expr: Some("x^2 + 1".to_string()),
            whiteboard_hint: "ejes".to_string(),
            template_hint: "derivative-slope".to_string(),
            params: BTreeMap::new(),
            efecto: efecto.to_string(),
            frames,
            run_ms: 1000,
            wait_after_ms: 200,
        }
    }

    fn acto_texto(titulo: &str, pasos: Vec<PasoTexto>) -> ActoTexto {
        ActoTexto {
            titulo: titulo.to_string(),
            fondo: None,
            limpiar: false,
            pasos,
        }
    }

    fn guion_texto(actos: Vec<ActoTexto>) -> GuionTexto {
        GuionTexto {
            concepto: "derivada".to_string(),
            width: 640,
            height: 480,
            actos,
        }
    }

    #[test]
    fn rechaza_guion_sin_actos() {
        let r = Guion::try_new(guion_texto(vec![]));
        assert!(r.is_err());
    }

    #[test]
    fn rechaza_guion_con_seis_actos() {
        let actos: Vec<ActoTexto> = (0..6)
            .map(|i| acto_texto(&format!("acto {i}"), vec![paso_texto("create", 4)]))
            .collect();
        let r = Guion::try_new(guion_texto(actos));
        assert!(r.is_err());
    }

    #[test]
    fn rechaza_acto_sin_pasos() {
        let r = Guion::try_new(guion_texto(vec![acto_texto("vacío", vec![])]));
        assert!(r.is_err());
    }

    #[test]
    fn rechaza_acto_con_cuatro_pasos() {
        let pasos: Vec<PasoTexto> = (0..4).map(|_| paso_texto("fade", 4)).collect();
        let r = Guion::try_new(guion_texto(vec![acto_texto("lleno", pasos)]));
        assert!(r.is_err());
    }

    #[test]
    fn rechaza_frames_sobre_96() {
        // 8 pasos (tope) × 16 frames = 128 > 96.
        let actos = vec![
            acto_texto(
                "a",
                vec![
                    paso_texto("create", 16),
                    paso_texto("fade", 16),
                    paso_texto("grow", 16),
                ],
            ),
            acto_texto(
                "b",
                vec![
                    paso_texto("write", 16),
                    paso_texto("indicate", 16),
                    paso_texto("tracker", 16),
                ],
            ),
            acto_texto("c", vec![paso_texto("create", 16), paso_texto("fade", 16)]),
        ];
        let r = Guion::try_new(guion_texto(actos));
        let e = r.expect_err("128 frames deben fallar");
        assert!(e.to_string().contains("96"));
    }

    #[test]
    fn rechaza_viewport_mixto() {
        let r = comprobar_viewport_unico(&[(640, 480), (800, 600)]);
        assert!(r.is_err());
        let ok = comprobar_viewport_unico(&[(640, 480), (640, 480)]);
        assert!(ok.is_ok());
    }

    #[test]
    fn viewport_unico_por_construccion() {
        // B4.4: `GuionTexto` solo admite UN par (width, height) para todo el
        // guion — no hay campo por paso/acto donde el viewport pueda
        // divergir, así que el mixto es imposible por construcción. El
        // `Guion` validado expone exactamente ese par, y la resolución
        // repetida por paso pasa `comprobar_viewport_unico` (la función
        // queda para composición multi-fuente, ver su doc).
        let g = Guion::try_new(guion_texto(vec![
            acto_texto("a", vec![paso_texto("create", 8)]),
            acto_texto("b", vec![paso_texto("fade", 12)]),
        ]))
        .expect("guion válido");
        assert_eq!(g.resolution.as_tuple(), (640, 480));
        let por_paso: Vec<(u32, u32)> = g
            .actos
            .iter()
            .flat_map(|a| a.pasos.iter())
            .map(|_| g.resolution.as_tuple())
            .collect();
        assert_eq!(por_paso.len(), g.total_pasos());
        let unica = comprobar_viewport_unico(&por_paso).expect("único por construcción");
        assert_eq!(unica.as_tuple(), (640, 480));
    }

    #[test]
    fn items_para_valida_remuestreo_con_n_distinto() {
        // B4.5: paridad con `a_playlist` — N distinto (8 vs 12) valida el
        // plan de remuestreo al máximo en `items_para`, no solo en playlist.
        let mut p12 = paso_texto("fade", 12);
        p12.template_hint = "integral-area".to_string();
        let g = Guion::try_new(guion_texto(vec![
            acto_texto("a", vec![paso_texto("create", 8), p12]),
            acto_texto("b", vec![paso_texto("grow", 8), paso_texto("write", 12)]),
        ]))
        .expect("guion válido");
        let escena = EscenaBase::por_defecto(FONDO_POR_DEFECTO).escena;
        let items = g.items_para(&escena).expect("items con N distinto");
        assert_eq!(items.len(), 4);
        let frames: Vec<usize> = items.iter().map(PlayItem::frames).collect();
        assert_eq!(frames, vec![8, 12, 8, 12]);
    }

    #[test]
    fn math_invalida_se_conserva_como_none() {
        let mut p = paso_texto("write", 4);
        p.math_expr = Some("x = ∫ f → Σ".to_string());
        let paso = PasoGuion::try_new(p).expect("el paso se conserva");
        assert_eq!(paso.math_expr, None);
        assert!(!paso.math_expr_valida_para_cas());
    }

    #[test]
    fn rechaza_template_fuera_de_allowlist() {
        let mut p = paso_texto("create", 4);
        p.template_hint = "hollywood-3d".to_string();
        let r = PasoGuion::try_new(p);
        assert!(r.is_err());
    }

    #[test]
    fn camino_feliz_y_lowering_puro() {
        let g = Guion::try_new(guion_texto(vec![acto_texto(
            "apertura",
            vec![paso_texto("create", 8)],
        )]))
        .expect("guion mínimo válido");
        assert_eq!(g.total_pasos(), 1);
        assert_eq!(g.total_frames(), 8);
        assert_eq!(g.resolution.as_tuple(), (640, 480));
        let escena = EscenaBase::por_defecto(FONDO_POR_DEFECTO).escena;
        let item = compilar_paso(&g.actos[0].pasos[0], &escena).expect("lowering puro");
        assert_eq!(item.frames(), 8);
        let items = g.items_para(&escena).expect("guion compila");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn frontera_limpiar_restaura_la_base() {
        let g = Guion::try_new(guion_texto(vec![acto_texto(
            "apertura",
            vec![paso_texto("fade", 4)],
        )]))
        .expect("guion válido");
        let mut viva = EscenaBase::por_defecto([10, 20, 30]).escena;
        viva.layers.push(Mobject::Dot { x: 1.0, y: 2.0 });
        viva.layers.push(Mobject::Dot { x: 3.0, y: 4.0 });
        assert_eq!(viva.layers.len(), 3);
        let mut acto = g.actos[0].clone();
        acto.frontera = Frontera::Limpiar;
        aplicar_frontera(&mut viva, &acto).expect("limpia a la base");
        assert_eq!(viva.layers.len(), acto.escena.capas());
        assert_eq!(viva.bg, FONDO_POR_DEFECTO);
    }

    #[test]
    fn hook_cas_solo_forma() {
        assert!(math_expr_valida_para_cas("x^2 + 1"));
        assert!(!math_expr_valida_para_cas("x = 1"));
        assert!(!math_expr_valida_para_cas("∫ f"));
        assert!(!math_expr_valida_para_cas("((x)"));
        assert!(!math_expr_valida_para_cas(""));
    }

    #[test]
    fn a_playlist_aplana_con_n_distinto_y_presupuesta() {
        // 2 actos × 2 pasos = 4 steps, N distinto (8 vs 12) → remuestreo.
        let mut p12 = paso_texto("fade", 12);
        p12.template_hint = "integral-area".to_string();
        let g = Guion::try_new(guion_texto(vec![
            acto_texto("a", vec![paso_texto("create", 8), p12]),
            acto_texto("b", vec![paso_texto("grow", 8), paso_texto("write", 12)]),
        ]))
        .expect("guion válido");
        let lista = g.a_playlist().expect("playlist válida");
        assert_eq!(lista.len(), 4);
        assert_eq!(lista.total_duration_ms(), g.duracion_total_ms());
        assert_eq!(lista.total_duration_ms(), 4 * (1000 + 200));
        // Orden de actos: templates en orden de emisión.
        let templates: Vec<&str> = lista
            .steps
            .iter()
            .filter_map(|s| s.request.as_ref().map(|r| r.template.as_str()))
            .collect();
        assert_eq!(
            templates,
            vec![
                "derivative-slope",
                "integral-area",
                "derivative-slope",
                "derivative-slope"
            ]
        );
        // Presupuesto: 40 frames en 640×480 dentro de 64 MiB.
        let bytes = Playlist::estimate_set_bytes(640, 480, 40).expect("sin desborde");
        assert!(bytes <= GUION_MAX_MEMORIA_BYTES);
    }

    #[test]
    fn a_playlist_espera_baja_a_pausa_con_cero_frames() {
        let mut espera = paso_texto("wait", 4);
        espera.run_ms = 1000;
        espera.wait_after_ms = 0;
        let g = Guion::try_new(guion_texto(vec![acto_texto(
            "a",
            vec![paso_texto("create", 8), espera],
        )]))
        .expect("guion válido");
        let lista = g.a_playlist().expect("playlist válida");
        assert_eq!(lista.len(), 2);
        assert!(lista.steps[1].is_wait());
        assert_eq!(lista.validate_frame_counts(&[8, 0]).expect("frames"), 8);
    }
}
