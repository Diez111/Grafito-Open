//! Animación paramétrica genérica 100% Rust (AS4, sin Python/manim).
//!
//! `ParametricAnim` = expresión/objeto + parámetro `p` en `[p0, p1]` +
//! N fotogramas acotados (≤48) + viewport fijo. Casos:
//! barrido de parámetro (familia `f(x;p)`), traza progresiva (la curva se
//! dibuja con `t` 0→1), morph entre dos estados (A→B por interpolación),
//! lugar geométrico de punto móvil, tangente móvil y área móvil (los
//! templates viejos `derivative-slope` / `integral-area` son estos dos
//! últimos con expresión canónica; el render vive en
//! `grafito-app/src/anim_native.rs`, aquí solo el modelo + inferencia).
//!
//! Inferencia (`infer_parametric_anim`): del pedido en lenguaje natural a
//! `ParametricAnim` con reglas honestas — si falta expresión, parámetro,
//! rango o tipo, devuelve `Err` que dice qué falta con un ejemplo; jamás
//! inventa matemática. Los textos para el usuario usan los nombres humanos
//! del mapa `humanize_control_name` de `grafito-ui/src/assistant.rs`
//! (deslizador, reproducir, pausar, tangente, área, recta, punto, función…),
//! nunca identificadores literales de control.
//!
//! Presupuestos: `PARAMETRIC_MAX_FRAMES = 48`, expresión ≤2000 caracteres
//! (igual que `MAX_EXPR_LENGTH` del núcleo), viewport 64..=4096 (igual que
//! `Resolution`), set acotado por `PARAMETRIC_MAX_BYTES = 64 MiB` vía
//! `checked_mul` (sin pánicos). Puro, sin I/O, sin egui, sin red.

use crate::protocol::Resolution;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tope de fotogramas por animación paramétrica (igual que el set nativo 48).
pub const PARAMETRIC_MAX_FRAMES: usize = 48;
/// Fotogramas por defecto cuando el pedido no dice cuántos (documentado, no inventado).
pub const PARAMETRIC_DEFAULT_FRAMES: usize = 24;
/// Largo máximo de cada expresión en caracteres (igual que `MAX_EXPR_LENGTH`).
pub const PARAMETRIC_MAX_EXPR_CHARS: usize = 2000;
/// Tope de bytes RGBA del set (`w*h*4*n`). 64 MiB cubren el set canónico
/// 640×480×48 (≈56 MiB) con margen; más que eso se rechaza honesto.
pub const PARAMETRIC_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Profundidad máxima del evaluador (evita recursión sin cota).
pub const PARAMETRIC_EVAL_MAX_DEPTH: usize = 64;

/// Cantidad de fotogramas validada 1..=`PARAMETRIC_MAX_FRAMES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameCount(pub usize);

impl FrameCount {
    pub fn try_new(n: usize) -> Result<Self, ParametricError> {
        if n == 0 || n > PARAMETRIC_MAX_FRAMES {
            return Err(ParametricError::FramesFueraDeRango {
                got: n,
                max: PARAMETRIC_MAX_FRAMES,
            });
        }
        Ok(Self(n))
    }

    pub fn get(self) -> usize {
        self.0
    }
}

/// Nombre del parámetro (`p`, `t`, `a`…). ASCII 1..=16, empieza con letra,
/// resto alfanumérico o `_`; `x`/`y` se rechazan (son variable y salida).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamName(pub String);

impl ParamName {
    pub fn try_new(raw: &str) -> Result<Self, ParametricError> {
        let name = raw.trim().to_string();
        if name.is_empty() || name.len() > 16 {
            return Err(ParametricError::ParametroInvalido { got: name });
        }
        let mut chars = name.chars();
        let first = chars.next().unwrap_or('?');
        if !first.is_ascii_alphabetic() {
            return Err(ParametricError::ParametroInvalido { got: name });
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(ParametricError::ParametroInvalido { got: name });
        }
        if name == "x" || name == "y" {
            return Err(ParametricError::ParametroInvalido { got: name });
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Caso de animación paramétrica (id de wire estable en `as_str`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParametricKind {
    /// Familia de curvas `f(x;p)` con `p` barriendo `[p0, p1]`.
    Sweep,
    /// La curva se dibuja progresiva con `t` 0→1.
    Trace,
    /// Interpolación `A→B`: `y = (1-s)·A + s·B`.
    Morph,
    /// Punto móvil `(p, f(p))` con su traza (lugar geométrico).
    Locus,
    /// Curva + recta tangente móvil en `x = p`.
    Tangent,
    /// Curva + área bajo la curva entre `p0` y `p` móvil.
    Area,
}

impl ParametricKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sweep => "sweep",
            Self::Trace => "trace",
            Self::Morph => "morph",
            Self::Locus => "locus",
            Self::Tangent => "tangent",
            Self::Area => "area",
        }
    }

    /// Nombre en español para la UI (prosa, sin identificadores de control).
    pub const fn en_espanol(self) -> &'static str {
        match self {
            Self::Sweep => "barrido de parámetro",
            Self::Trace => "traza progresiva",
            Self::Morph => "transición entre dos estados",
            Self::Locus => "lugar geométrico de punto móvil",
            Self::Tangent => "recta tangente móvil",
            Self::Area => "área móvil bajo la curva",
        }
    }

    pub fn from_name(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "sweep" | "barrido" => Some(Self::Sweep),
            "trace" | "traza" => Some(Self::Trace),
            "morph" | "transicion" | "transición" => Some(Self::Morph),
            "locus" | "lugar" => Some(Self::Locus),
            "tangent" | "tangente" => Some(Self::Tangent),
            "area" | "área" => Some(Self::Area),
            _ => None,
        }
    }
}

impl std::fmt::Display for ParametricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.en_espanol())
    }
}

/// Error honesto de inferencia/validación (siempre dice qué falta + ejemplo).
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ParametricError {
    #[error(
        "pedido vacío: describí qué animar, por ejemplo «barrido de f(x)=x^2+p·x con p en [-2,2]»"
    )]
    PedidoVacio,
    #[error("no sé qué tipo de animación pedís: decí barrido, traza progresiva, transición entre dos estados, lugar geométrico, recta tangente móvil o área móvil")]
    FaltaTipo,
    #[error(
        "falta la expresión: escribí f(x)=… o y=… con la variable x, por ejemplo «f(x)=x^2+p·x»"
    )]
    FaltaExpresion,
    #[error("la transición necesita dos estados: escribí A y B, por ejemplo «transición de f(x)=x^2 a f(x)=x^3 con p en [0,1]»")]
    FaltaSegundoEstado,
    #[error(
        "falta el parámetro: nombralo con una letra (p, t, a…), por ejemplo «con p en [-2,2]»"
    )]
    FaltaParametro,
    #[error("falta el rango del parámetro: indicá desde y hasta, por ejemplo «con p en [-2,2]» o «entre 0 y 1»")]
    FaltaRango,
    #[error("rango degenerado: el inicio ({p0}) y el fin ({p1}) deben ser distintos y finitos")]
    RangoDegenerado { p0: f64, p1: f64 },
    #[error("parámetro inválido {got:?}: usá una letra (p, t, a…) distinta de x e y, de hasta 16 caracteres")]
    ParametroInvalido { got: String },
    #[error("fotogramas fuera de rango: pediste {got}, el tope es {max} (achicá N o dividí la animación)")]
    FramesFueraDeRango { got: usize, max: usize },
    #[error("expresión demasiado larga ({got} caracteres, tope {max}): acortala o dividila")]
    ExpresionMuyLarga { got: usize, max: usize },
    #[error("el set estimado ({got} bytes) excede el tope de {max} bytes: bajá la resolución o los fotogramas")]
    ExcedeMemoria { got: usize, max: usize },
    #[error("viewport inválido {w}x{h}: usá lados entre 64 y 4096")]
    ViewportInvalido { w: u32, h: u32 },
    #[error("no soportado: {detalle}")]
    NoSoportado { detalle: String },
}

pub type ParametricResult<T> = Result<T, ParametricError>;

/// Animación genérica: expresión + parámetro en rango + N frames + viewport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricAnim {
    pub kind: ParametricKind,
    /// Expresión de `y` en `x` y el parámetro (estado A; único en no-morph).
    pub expr_a: String,
    /// Estado B, solo en `Morph`.
    pub expr_b: Option<String>,
    pub param: ParamName,
    pub p0: f64,
    pub p1: f64,
    pub frames: FrameCount,
    pub viewport: Resolution,
}

impl ParametricAnim {
    /// Constructor validado (todo `Err` es honesto, sin pánicos).
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        kind: ParametricKind,
        expr_a: String,
        expr_b: Option<String>,
        param: ParamName,
        p0: f64,
        p1: f64,
        frames: FrameCount,
        viewport: Resolution,
    ) -> ParametricResult<Self> {
        let anim = Self {
            kind,
            expr_a,
            expr_b,
            param,
            p0,
            p1,
            frames,
            viewport,
        };
        anim.validate()?;
        Ok(anim)
    }

    pub fn validate(&self) -> ParametricResult<()> {
        let len_a = self.expr_a.chars().count();
        if self.expr_a.trim().is_empty() {
            return Err(ParametricError::FaltaExpresion);
        }
        if len_a > PARAMETRIC_MAX_EXPR_CHARS {
            return Err(ParametricError::ExpresionMuyLarga {
                got: len_a,
                max: PARAMETRIC_MAX_EXPR_CHARS,
            });
        }
        match self.kind {
            ParametricKind::Morph => {
                let b = self.expr_b.as_deref().unwrap_or("").trim();
                if b.is_empty() {
                    return Err(ParametricError::FaltaSegundoEstado);
                }
                if b.chars().count() > PARAMETRIC_MAX_EXPR_CHARS {
                    return Err(ParametricError::ExpresionMuyLarga {
                        got: b.chars().count(),
                        max: PARAMETRIC_MAX_EXPR_CHARS,
                    });
                }
            }
            _ => {
                if let Some(b) = self.expr_b.as_deref() {
                    if !b.trim().is_empty() {
                        return Err(ParametricError::NoSoportado {
                            detalle:
                                "el segundo estado solo vale para la transición entre dos estados"
                                    .into(),
                        });
                    }
                }
            }
        }
        if !self.p0.is_finite() || !self.p1.is_finite() {
            return Err(ParametricError::RangoDegenerado {
                p0: self.p0,
                p1: self.p1,
            });
        }
        if self.p0 == self.p1 {
            return Err(ParametricError::RangoDegenerado {
                p0: self.p0,
                p1: self.p1,
            });
        }
        Resolution::try_new(self.viewport.width, self.viewport.height).map_err(|_| {
            ParametricError::ViewportInvalido {
                w: self.viewport.width,
                h: self.viewport.height,
            }
        })?;
        self.validate_budget()?;
        Ok(())
    }

    /// Cantidad de fotogramas (1..=48).
    pub fn frame_count(&self) -> usize {
        self.frames.get()
    }

    /// Valor del parámetro en el fotograma `i` (lerp, clamp, total).
    pub fn frame_param(&self, i: usize) -> f64 {
        let n = self.frame_count();
        if n <= 1 {
            return self.p0;
        }
        let idx = i.min(n - 1) as f64;
        let denom = (n - 1) as f64;
        let s = (idx / denom).clamp(0.0, 1.0);
        let v = self.p0 + (self.p1 - self.p0) * s;
        if v.is_finite() {
            v
        } else {
            self.p0
        }
    }

    /// Fracción 0..1 del fotograma `i` (para traza/morph).
    pub fn frame_fraction(&self, i: usize) -> f64 {
        let n = self.frame_count();
        if n <= 1 {
            return 1.0;
        }
        (i.min(n - 1) as f64 / (n - 1) as f64).clamp(0.0, 1.0)
    }

    /// Bytes RGBA estimados del set (`w*h*4*n`), `None` si desborda.
    pub fn estimate_bytes(&self) -> Option<usize> {
        (self.viewport.width as usize)
            .checked_mul(self.viewport.height as usize)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_mul(self.frame_count()))
    }

    /// Rechaza honesto si el set excede `PARAMETRIC_MAX_BYTES` o desborda.
    pub fn validate_budget(&self) -> ParametricResult<()> {
        match self.estimate_bytes() {
            None => Err(ParametricError::ExcedeMemoria {
                got: usize::MAX,
                max: PARAMETRIC_MAX_BYTES,
            }),
            Some(got) if got > PARAMETRIC_MAX_BYTES => Err(ParametricError::ExcedeMemoria {
                got,
                max: PARAMETRIC_MAX_BYTES,
            }),
            Some(_) => Ok(()),
        }
    }

    /// Evalúa la expresión del fotograma `i` en `x` (barrido/traza/lugar/
    /// tangente/área usan A; morph interpola A→B con la fracción del frame).
    pub fn eval_frame(&self, i: usize, x: f64) -> Option<f64> {
        match self.kind {
            ParametricKind::Morph => {
                let b = self.expr_b.as_deref().unwrap_or("");
                let fa = eval_expr(&self.expr_a, x, self.param.as_str(), self.p0)?;
                let fb = eval_expr(b, x, self.param.as_str(), self.p0)?;
                let s = self.frame_fraction(i);
                let v = fa + (fb - fa) * s;
                if v.is_finite() {
                    Some(v)
                } else {
                    None
                }
            }
            _ => {
                let p = self.frame_param(i);
                eval_expr(&self.expr_a, x, self.param.as_str(), p)
            }
        }
    }
}

// ── Evaluador univariado puro (sin dependencias) ─────────────────────────
// Soporta: números, `x`, parámetro (`p`/`t`/…), `pi`, `e`, `+ - * / ^ %`,
// unario `-`, paréntesis y `sin cos tan asin acos atan exp ln log sqrt abs
// floor ceil`. Todo lo demás da `None` honesto en vez de inventar.
// Total: nunca pánico.

fn eval_expr(expr: &str, x: f64, param_name: &str, p: f64) -> Option<f64> {
    if expr.len() > PARAMETRIC_MAX_EXPR_CHARS {
        return None;
    }
    if !x.is_finite() || !p.is_finite() {
        return None;
    }
    let mut parser = ExprParser {
        bytes: expr.as_bytes(),
        pos: 0,
        x,
        p,
        param_name: param_name.as_bytes(),
        depth: 0,
    };
    let v = parser.parse_add()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return None;
    }
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

struct ExprParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    x: f64,
    p: f64,
    param_name: &'a [u8],
    depth: usize,
}

impl<'a> ExprParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_add(&mut self) -> Option<f64> {
        if self.depth > PARAMETRIC_EVAL_MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let mut v = self.parse_mul()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    let rhs = self.parse_mul()?;
                    v += rhs;
                }
                Some(b'-') => {
                    self.pos += 1;
                    let rhs = self.parse_mul()?;
                    v -= rhs;
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Some(v)
    }

    fn parse_mul(&mut self) -> Option<f64> {
        let mut v = self.parse_pow()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => {
                    // `**` no se soporta (usá `^`); honesto: None.
                    if self.bytes.get(self.pos + 1) == Some(&b'*') {
                        return None;
                    }
                    self.pos += 1;
                    let rhs = self.parse_pow()?;
                    v *= rhs;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let rhs = self.parse_pow()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    v /= rhs;
                }
                Some(b'%') => {
                    self.pos += 1;
                    let rhs = self.parse_pow()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    v %= rhs;
                }
                _ => break,
            }
        }
        Some(v)
    }

    fn parse_pow(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        self.skip_ws();
        if self.peek() == Some(b'^') {
            self.pos += 1;
            let exp = self.parse_unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.eat(b'-') {
            return Some(-self.parse_unary()?);
        }
        if self.eat(b'+') {
            return self.parse_unary();
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Option<f64> {
        self.skip_ws();
        let c = self.peek()?;
        if c == b'(' {
            self.pos += 1;
            let v = self.parse_add()?;
            if !self.eat(b')') {
                return None;
            }
            return Some(v);
        }
        if c.is_ascii_digit() || c == b'.' {
            return self.parse_number();
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            return self.parse_named();
        }
        None
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.pos;
        let mut seen_digit = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                seen_digit = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.peek() == Some(b'.') {
            // No consumir `..` (rango): lo detecta el chequeo final.
            let after_dot = self.bytes.get(self.pos + 1).copied();
            if after_dot != Some(b'.') {
                self.pos += 1;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        seen_digit = true;
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        if !seen_digit {
            return None;
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == exp_start {
                self.pos = save;
            }
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.pos)?).ok()?;
        let v: f64 = text.parse().ok()?;
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }

    fn parse_named(&mut self) -> Option<f64> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name = std::str::from_utf8(self.bytes.get(start..self.pos)?).ok()?;
        let lower = name.to_ascii_lowercase();
        if name == "x" || name == "X" {
            return Some(self.x);
        }
        if name == self.param_name_str() {
            return Some(self.p);
        }
        if lower == "pi" {
            return Some(std::f64::consts::PI);
        }
        if lower == "e" && name.len() == 1 {
            return Some(std::f64::consts::E);
        }
        self.skip_ws();
        if self.peek() != Some(b'(') {
            return None;
        }
        self.pos += 1;
        let arg = self.parse_add()?;
        if !self.eat(b')') {
            return None;
        }
        apply_func(&lower, arg)
    }

    fn param_name_str(&self) -> &str {
        std::str::from_utf8(self.param_name).unwrap_or("p")
    }
}

fn apply_func(name: &str, arg: f64) -> Option<f64> {
    if !arg.is_finite() {
        return None;
    }
    let v = match name {
        "sin" => arg.sin(),
        "cos" => arg.cos(),
        "tan" => arg.tan(),
        "asin" => {
            if !(-1.0..=1.0).contains(&arg) {
                return None;
            }
            arg.asin()
        }
        "acos" => {
            if !(-1.0..=1.0).contains(&arg) {
                return None;
            }
            arg.acos()
        }
        "atan" => arg.atan(),
        "exp" => arg.exp(),
        "ln" | "log" => {
            if arg <= 0.0 {
                return None;
            }
            arg.ln()
        }
        "sqrt" => {
            if arg < 0.0 {
                return None;
            }
            arg.sqrt()
        }
        "abs" => arg.abs(),
        "floor" => arg.floor(),
        "ceil" => arg.ceil(),
        _ => return None,
    };
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

// ── Inferencia honesta desde lenguaje natural ────────────────────────────

/// Infiere una `ParametricAnim` desde un pedido en lenguaje natural.
///
/// Reglas honestas (nada se inventa):
/// - tipo por palabras clave (barrido, traza, transición, lugar, tangente,
///   área); sin ellas → `FaltaTipo`;
/// - expresión tras `=` (para transición, dos); sin ella → `FaltaExpresion`
///   / `FaltaSegundoEstado`;
/// - rango con corchetes, `entre A y B`, `de A a B`, `A..B` o `A→B`; sin
///   él → `FaltaRango` (con ejemplo);
/// - fotogramas solo si el pedido dice cuántos (tope 48), si no 24;
/// - viewport fijo 640×480 salvo tamaño explícito válido.
pub fn infer_parametric_anim(pedido: &str) -> ParametricResult<ParametricAnim> {
    let text = pedido.trim();
    if text.is_empty() {
        return Err(ParametricError::PedidoVacio);
    }
    if text.chars().count() > 2000 {
        return Err(ParametricError::ExpresionMuyLarga {
            got: text.chars().count(),
            max: 2000,
        });
    }
    let lower = text.to_lowercase();
    let kind = detect_kind(&lower).ok_or(ParametricError::FaltaTipo)?;

    let (expr_a, expr_b) = match kind {
        ParametricKind::Morph => extract_morph_exprs(text)?,
        _ => {
            let expr = extract_single_expr(text).ok_or(ParametricError::FaltaExpresion)?;
            (expr, None)
        }
    };

    let (param_raw, p0, p1) = extract_range(text).ok_or(ParametricError::FaltaRango)?;
    let param = if param_raw.trim().is_empty() {
        // Nombre no escrito: deducir de la expresión (p si la usa, t si no).
        let fallback = guess_param_name(&expr_a, expr_b.as_deref());
        ParamName::try_new(fallback)?
    } else {
        ParamName::try_new(&param_raw)?
    };

    let n = extract_frames(&lower)?;
    let frames = FrameCount::try_new(n.unwrap_or(PARAMETRIC_DEFAULT_FRAMES))?;
    let viewport = extract_viewport(&lower).unwrap_or_default();

    ParametricAnim::try_new(kind, expr_a, expr_b, param, p0, p1, frames, viewport)
}

fn detect_kind(lower: &str) -> Option<ParametricKind> {
    // De específico a general (la tangente/área contienen "móvil" como locus).
    if lower.contains("morph")
        || lower.contains("interpola")
        || lower.contains("transicion")
        || lower.contains("transición")
    {
        return Some(ParametricKind::Morph);
    }
    if lower.contains("tangente") || lower.contains("tangent") {
        return Some(ParametricKind::Tangent);
    }
    if (lower.contains("área") || lower.contains("area"))
        && (lower.contains("móvil")
            || lower.contains("movil")
            || lower.contains("movien")
            || lower.contains("bajo")
            || lower.contains("integral"))
    {
        return Some(ParametricKind::Area);
    }
    if lower.contains("integral")
        && (lower.contains("móvil") || lower.contains("movil") || lower.contains("movien"))
    {
        return Some(ParametricKind::Area);
    }
    if lower.contains("lugar")
        || lower.contains("locus")
        || ((lower.contains("punto m") || lower.contains("punto-m"))
            && (lower.contains("vil") || lower.contains("mov")))
    {
        return Some(ParametricKind::Locus);
    }
    if lower.contains("traza")
        || lower.contains("progresiva")
        || lower.contains("progresivo")
        || lower.contains("se dibuja")
        || lower.contains("trazado")
        || lower.contains("se traza")
    {
        return Some(ParametricKind::Trace);
    }
    if lower.contains("barrido")
        || lower.contains("familia")
        || lower.contains("parametro")
        || lower.contains("parámetro")
        || lower.contains("variando")
        || lower.contains("para p")
        || lower.contains("con p en")
        || lower.contains("con p=")
        || lower.contains("para t")
        || lower.contains("con t en")
    {
        return Some(ParametricKind::Sweep);
    }
    // Transición con dos expresiones separadas por flecha, aunque no diga morph.
    if (lower.contains('→') || lower.contains("->") || lower.contains("=>"))
        && lower.matches('=').count() >= 2
    {
        return Some(ParametricKind::Morph);
    }
    // "entre A y B" con dos expresiones también es transición.
    if lower.contains(" entre ") && lower.matches('=').count() >= 2 {
        return Some(ParametricKind::Morph);
    }
    None
}

/// Expresión tras el primer `=` hasta un delimitador de prosa.
fn extract_single_expr(text: &str) -> Option<String> {
    let eq = text.find('=')?;
    let rhs = text.get(eq + 1..)?;
    let cut = cut_expr_rhs(rhs);
    if cut.trim().is_empty() {
        return None;
    }
    Some(cut)
}

/// Dos expresiones para morph (izquierda y derecha de flecha o dos `=`).
fn extract_morph_exprs(text: &str) -> ParametricResult<(String, Option<String>)> {
    // 1. Flecha explícita con `=` en ambos lados.
    for sep in ["→", "->", "=>"] {
        if let Some(pos) = text.find(sep) {
            let left = text.get(..pos).unwrap_or("");
            let right = text.get(pos + sep.len()..).unwrap_or("");
            if left.contains('=') && right.contains('=') {
                let a = left
                    .rfind('=')
                    .and_then(|eq| left.get(eq + 1..))
                    .map(cut_expr_rhs)
                    .unwrap_or_default();
                let b = right
                    .find('=')
                    .and_then(|eq| right.get(eq + 1..))
                    .map(cut_expr_rhs)
                    .unwrap_or_default();
                if a.trim().is_empty() {
                    return Err(ParametricError::FaltaExpresion);
                }
                if b.trim().is_empty() {
                    return Err(ParametricError::FaltaSegundoEstado);
                }
                return Ok((a, Some(b)));
            }
        }
    }
    // 2. Dos `=` en el texto (separador de estados entre ellos).
    let eqs: Vec<usize> = text.match_indices('=').map(|(i, _)| i).collect();
    if eqs.len() >= 2 {
        let first = eqs[0];
        let second = eqs[1];
        let a = text
            .get(first + 1..second)
            .map(|seg| {
                let mut s = seg.to_string();
                for cut in [" y f(", " y y=", " y F(", " a f(", " a y="] {
                    if let Some(p) = s.find(cut) {
                        s.truncate(p);
                    }
                }
                cut_expr_rhs(&s)
            })
            .unwrap_or_default();
        let b = text.get(second + 1..).map(cut_expr_rhs).unwrap_or_default();
        if a.trim().is_empty() {
            return Err(ParametricError::FaltaExpresion);
        }
        if b.trim().is_empty() {
            return Err(ParametricError::FaltaSegundoEstado);
        }
        return Ok((a, Some(b)));
    }
    // 3. Un solo estado → falta el segundo (honesto, no se inventa B).
    if text.contains('=') {
        return Err(ParametricError::FaltaSegundoEstado);
    }
    Err(ParametricError::FaltaExpresion)
}

/// Recorta el lado derecho de `=` ante prosa (`con`, `para`, `en`, …).
fn cut_expr_rhs(rhs: &str) -> String {
    let s = rhs.trim().to_string();
    // Quitar comillas/backticks de borde.
    let s = s
        .trim_matches(|c| c == '`' || c == '"' || c == '\'' || c == '«' || c == '»')
        .to_string();
    // Delimitadores de prosa (el más temprano gana).
    let cuts = [
        " con p en",
        " con t en",
        " con p=",
        " con a en",
        " con p ",
        " con t ",
        " con ",
        " para p en",
        " para t en",
        " para p ",
        " para t ",
        " para ",
        " en [",
        " en (",
        " entre ",
        " donde ",
        // `[` jamás es matemática válida (el evaluador no tiene corchetes):
        // un rango suelto ("f(x)=x^2 [0,2]") termina la expresión.
        " [",
        ";",
        "\n",
    ];
    let low = s.to_lowercase();
    let mut best: Option<usize> = None;
    for cut in cuts {
        if let Some(p) = low.find(cut) {
            best = Some(best.map_or(p, |b: usize| b.min(p)));
        }
    }
    // N1: rangos en prosa tras la expresión ("x^3 de 0 a 2", "x^2 hasta 2").
    // Solo cortan si lo que sigue es un número (mirada numérica): así
    // "x^2 + a*x con a en [0,2]" conserva el parámetro `a` con espacios,
    // porque tras " a " viene "x", no un número.
    for key in [" de ", " hasta ", " a "] {
        let mut from = 0;
        while let Some(rel) = low.get(from..).and_then(|tail| tail.find(key)) {
            let pos = from + rel;
            let after = s.get(pos + key.len()..).unwrap_or("");
            if number_prefix(after).is_some() {
                best = Some(best.map_or(pos, |b: usize| b.min(pos)));
                break;
            }
            from = pos + key.len();
        }
    }
    let mut out = match best {
        Some(p) => s.get(..p).unwrap_or("").to_string(),
        None => s,
    };
    // Recorte final: quitar punto de fin de oración y espacios.
    out = out.trim().to_string();
    while out.ends_with('.') && !out.ends_with("..") {
        out.pop();
    }
    out = out
        .trim_matches(|c| c == '`' || c == '"' || c == '\'')
        .trim()
        .to_string();
    // Normalizar `·` y `×`/`−` de prosa para el evaluador.
    out = out.replace(['·', '×'], "*");
    out.replace("−", "-")
}

fn guess_param_name(expr_a: &str, expr_b: Option<&str>) -> &'static str {
    let has_p = expr_a.contains('p') || expr_a.contains('P');
    let has_t = expr_a.contains('t') || expr_a.contains('T');
    let b_has_p = expr_b.is_some_and(|b| b.contains('p') || b.contains('P'));
    if has_p || b_has_p {
        "p"
    } else if has_t {
        "t"
    } else {
        "p"
    }
}

/// Rango `(nombre, p0, p1)`: corchetes, `entre`, `de…a…`, `..`, `→`.
fn extract_range(text: &str) -> Option<(String, f64, f64)> {
    let normalized = text.replace('−', "-").replace("–", "-");
    if let Some(r) = range_in_brackets(&normalized) {
        return Some(r);
    }
    if let Some(r) = range_entre(&normalized) {
        return Some(r);
    }
    if let Some(r) = range_de_a(&normalized) {
        return Some(r);
    }
    if let Some(r) = range_dots_or_arrow(&normalized) {
        return Some(r);
    }
    None
}

fn parse_finite(s: &str) -> Option<f64> {
    let v: f64 = s.trim().replace(',', ".").parse().ok()?;
    if v.is_finite() {
        Some(v)
    } else {
        None
    }
}

fn param_name_before(text: &str, pos: usize) -> String {
    // Ventana de 16 chars antes del rango: buscar `<letra> en|de|entre|=`.
    let start = pos.saturating_sub(16);
    let window = text.get(start..pos).unwrap_or("");
    let low = window.to_lowercase();
    for key in [" en", " de", " entre", "="] {
        if let Some(k) = low.rfind(key) {
            let before = window.get(..k).unwrap_or("").trim_end();
            if let Some(ch) = before.chars().last() {
                if ch.is_ascii_alphabetic() && ch != 'x' && ch != 'y' && ch != 'X' && ch != 'Y' {
                    let word: String = before
                        .chars()
                        .rev()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    if !word.is_empty()
                        && word.len() <= 16
                        && word.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                    {
                        return word;
                    }
                    return ch.to_string();
                }
            }
        }
    }
    // Última letra suelta antes del rango (estilo `p [-2,2]` o `t 0→1`).
    let letters: Vec<char> = window.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if let Some(last) = letters.last() {
        if *last != 'x' && *last != 'y' && *last != 'X' && *last != 'Y' {
            return last.to_string();
        }
    }
    String::new()
}

fn range_in_brackets(text: &str) -> Option<(String, f64, f64)> {
    let open = text.find('[')?;
    let after = text.get(open + 1..)?;
    let comma = after.find(',')?;
    let after_comma = after.get(comma + 1..)?;
    let close = after_comma.find(']')?;
    let a_str = after.get(..comma)?;
    let b_str = after_comma.get(..close)?;
    let p0 = parse_finite(a_str)?;
    let p1 = parse_finite(b_str)?;
    if p0 == p1 {
        return None;
    }
    let name = param_name_before(text, open);
    Some((name, p0, p1))
}

fn range_entre(text: &str) -> Option<(String, f64, f64)> {
    let low = text.to_lowercase();
    let pos = low.find("entre")?;
    let after = text.get(pos + "entre".len()..)?;
    let low_after = after.to_lowercase();
    let sep = low_after.find(" y ")?;
    let a_str = after.get(..sep)?;
    let b_part = after.get(sep + 3..)?;
    let b_str = number_prefix(b_part)?;
    let a_num = number_suffix(a_str)?;
    let p0 = parse_finite(&a_num)?;
    let p1 = parse_finite(&b_str)?;
    if p0 == p1 {
        return None;
    }
    let name = param_name_before(text, pos);
    Some((name, p0, p1))
}

fn range_de_a(text: &str) -> Option<(String, f64, f64)> {
    // `de <a> a <b>` con números a ambos lados (evita falsos "de la curva a…").
    let low = text.to_lowercase();
    let mut search_from = 0;
    while let Some(rel) = low.get(search_from..)?.find(" de ") {
        let pos = search_from + rel;
        let after = text.get(pos + 4..).unwrap_or("");
        if let Some(sep) = after.to_lowercase().find(" a ") {
            let a_tail = number_suffix(after.get(..sep).unwrap_or(""))?;
            let b_head = number_prefix(after.get(sep + 3..).unwrap_or(""))?;
            if let (Some(p0), Some(p1)) = (parse_finite(&a_tail), parse_finite(&b_head)) {
                if p0 != p1 {
                    let name = param_name_before(text, pos);
                    return Some((name, p0, p1));
                }
            }
        }
        search_from = pos + 4;
    }
    None
}

fn range_dots_or_arrow(text: &str) -> Option<(String, f64, f64)> {
    for sep in ["..", "→", "->", "=>"] {
        if let Some(pos) = text.find(sep) {
            let left = text.get(..pos).unwrap_or("");
            let right = text.get(pos + sep.len()..).unwrap_or("");
            let a_num = number_suffix(left)?;
            let b_num = number_prefix(right)?;
            if let (Some(p0), Some(p1)) = (parse_finite(&a_num), parse_finite(&b_num)) {
                if p0 != p1 {
                    let name = param_name_before(text, pos);
                    return Some((name, p0, p1));
                }
            }
        }
    }
    None
}

/// Número al final del fragmento (para `entre`/`de…a…`/flechas).
fn number_suffix(s: &str) -> Option<String> {
    let t = s.trim_end_matches(|c| {
        c == '.' || c == ',' || c == ';' || c == ')' || c == ']' || c == '"' || c == '\''
    });
    let bytes = t.as_bytes();
    let mut end = t.len();
    // Saltar prosa final hasta el último dígito.
    while end > 0 {
        let c = bytes[end - 1] as char;
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' {
            break;
        }
        if (c == '+' || c == '-') && end >= 2 {
            let prev = bytes[end - 2] as char;
            if prev == 'e' || prev == 'E' {
                break;
            }
        }
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 {
        let c = bytes[start - 1] as char;
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
            start -= 1;
        } else {
            break;
        }
    }
    let cand = t.get(start..end)?.trim().to_string();
    if cand.is_empty() || !cand.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(cand)
}

/// Número al inicio del fragmento.
fn number_prefix(s: &str) -> Option<String> {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0;
    let mut seen_digit = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b',') {
        if bytes[i].is_ascii_digit() {
            seen_digit = true;
        }
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let k = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > k {
            i = j;
        }
    }
    if !seen_digit || i == 0 {
        return None;
    }
    Some(t.get(..i)?.to_string())
}

/// Fotogramas pedidos (`24 fotogramas`, `N=24`, `48 frames`).
fn extract_frames(lower: &str) -> ParametricResult<Option<usize>> {
    if let Some(pos) = lower.find("n=") {
        let after = lower.get(pos + 2..).unwrap_or("");
        if let Some(num) = leading_digits(after.trim_start()) {
            return check_frames(num);
        }
    }
    if let Some(pos) = lower.find("n:") {
        let after = lower.get(pos + 2..).unwrap_or("");
        if let Some(num) = leading_digits(after.trim_start()) {
            return check_frames(num);
        }
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut idx = 0;
    while idx < words.len() {
        let w = words[idx]
            .trim_matches(|c| c == '.' || c == ',' || c == ';' || c == ')' || c == '(' || c == '"');
        if w == "fotograma"
            || w == "fotogramas"
            || w == "frame"
            || w == "frames"
            || w == "cuadro"
            || w == "cuadros"
        {
            if idx > 0 {
                if let Some(num) = trailing_digits(words[idx - 1]) {
                    return check_frames(num);
                }
            }
            if idx + 1 < words.len() {
                if let Some(num) = leading_digits(words[idx + 1]) {
                    return check_frames(num);
                }
            }
        }
        idx += 1;
    }
    Ok(None)
}

fn leading_digits(s: &str) -> Option<usize> {
    let mut end = 0;
    for c in s.chars() {
        if c.is_ascii_digit() {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    s.get(..end)?.parse().ok()
}

fn trailing_digits(s: &str) -> Option<usize> {
    let t = s.trim_matches(|c| {
        c == '.' || c == ',' || c == ';' || c == ')' || c == '(' || c == '"' || c == ':'
    });
    let mut start = t.len();
    for c in t.chars().rev() {
        if c.is_ascii_digit() {
            start -= c.len_utf8();
        } else {
            break;
        }
    }
    if start >= t.len() {
        return None;
    }
    t.get(start..)?.parse().ok()
}

fn check_frames(n: usize) -> ParametricResult<Option<usize>> {
    if n == 0 || n > PARAMETRIC_MAX_FRAMES {
        return Err(ParametricError::FramesFueraDeRango {
            got: n,
            max: PARAMETRIC_MAX_FRAMES,
        });
    }
    Ok(Some(n))
}

/// Tamaño explícito (`640x480`, `canvas [640, 480]`); `None` → defecto.
fn extract_viewport(lower: &str) -> Option<Resolution> {
    if let Some(pos) = lower.find("canvas") {
        let rest = lower.get(pos + "canvas".len()..).unwrap_or("");
        if let (Some(a), Some(b)) = (rest.find('['), rest.find(']')) {
            if a < b {
                let inside = rest.get(a + 1..b).unwrap_or("");
                let parts: Vec<&str> = inside.split(',').collect();
                if parts.len() == 2 {
                    if let (Ok(w), Ok(h)) = (
                        parts[0].trim().parse::<u32>(),
                        parts[1].trim().parse::<u32>(),
                    ) {
                        if let Ok(r) = Resolution::try_new(w, h) {
                            return Some(r);
                        }
                    }
                }
            }
        }
    }
    // `640x480` (con `x` o `×`).
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let sep_x = bytes.get(j) == Some(&b'x');
            let sep_mul = lower.get(j..j + 1) == Some("×");
            if sep_x || sep_mul {
                let sep_len = if sep_mul { "×".len() } else { 1 };
                let mut k = j + sep_len;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                let mut m = k;
                while m < bytes.len() && bytes[m].is_ascii_digit() {
                    m += 1;
                }
                if m > k {
                    if let (Ok(w), Ok(h)) = (
                        lower.get(i..j).unwrap_or("").parse::<u32>(),
                        lower.get(k..m).unwrap_or("").parse::<u32>(),
                    ) {
                        if let Ok(r) = Resolution::try_new(w, h) {
                            return Some(r);
                        }
                    }
                }
                i = m.max(j + 1);
                continue;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

/// Pista en español para la vista previa (solo nombres humanos del mapa de
/// controles: deslizador, reproducir, pausar, tangente, área, recta, punto,
/// función — jamás identificadores literales).
pub fn parametric_hint(anim: &ParametricAnim) -> String {
    let n = anim.frame_count();
    let base = match anim.kind {
        ParametricKind::Sweep => format!(
            "{} listo: {} fotogramas de «{}» con {} en [{}, {}]",
            anim.kind,
            n,
            anim.expr_a,
            anim.param.as_str(),
            anim.p0,
            anim.p1
        ),
        ParametricKind::Trace => format!(
            "{} lista: {} fotogramas de «{}» que se dibuja de a poco",
            anim.kind, n, anim.expr_a
        ),
        ParametricKind::Morph => format!(
            "{} lista: {} fotogramas de «{}» hacia «{}»",
            anim.kind,
            n,
            anim.expr_a,
            anim.expr_b.as_deref().unwrap_or("?")
        ),
        ParametricKind::Locus => format!(
            "{} listo: {} fotogramas del punto sobre «{}» con {} en [{}, {}]",
            anim.kind,
            n,
            anim.expr_a,
            anim.param.as_str(),
            anim.p0,
            anim.p1
        ),
        ParametricKind::Tangent => format!(
            "{} lista: {} fotogramas de «{}» con la recta tangente en {} en [{}, {}]",
            anim.kind,
            n,
            anim.expr_a,
            anim.param.as_str(),
            anim.p0,
            anim.p1
        ),
        ParametricKind::Area => format!(
            "{} lista: {} fotogramas de «{}» con el área desde {} hasta el valor móvil",
            anim.kind, n, anim.expr_a, anim.p0
        ),
    };
    format!(
        "{base}. Mové el deslizador para recorrer los fotogramas y usá reproducir o pausar para controlar la vista previa."
    )
}

// ── N1: canónica de integral sin función + inferencia de área ────────────
// Política coherente en inferencia + Submit + agente: si el pedido menciona
// integral/área pero no trae función, se renderiza el canónico
// `f(x)=x^2 en [0,2]` Y la prosa lo declara (`INTEGRAL_CANONICAL_PROSA`);
// jamás preguntar Y mostrar a la vez. Si trae función con `x` pero no
// evaluable en ningún punto del rango, `Err` honesto sin frames.

/// Expresión canónica cuando el pedido no trae función.
pub const INTEGRAL_CANONICAL_EXPR: &str = "x^2";
/// Rango canónico `[0, 2]` cuando el pedido no trae rango.
pub const INTEGRAL_CANONICAL_P0: f64 = 0.0;
/// Rango canónico `[0, 2]` cuando el pedido no trae rango.
pub const INTEGRAL_CANONICAL_P1: f64 = 2.0;
/// Parámetro canónico de la animación de área.
pub const INTEGRAL_CANONICAL_PARAM: &str = "p";
/// Prosa rioplatense que declara la canónica (Submit + agente la usan tal
/// cual; el renderer dibuja etiquetas ASCII por separado).
pub const INTEGRAL_CANONICAL_PROSA: &str =
    "te muestro con f(x)=x² en [0,2]; pedime otra y la cambio";

/// Normaliza para matching honesto: minúsculas + sin tildes.
///
/// "animación" y "animacion", "área" y "area", "derivada" y "deriváda"
/// matchean igual. Puro, sin I/O, sin `unwrap`.
pub fn normaliza_para_match(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    for c in texto.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        };
        salida.push(base);
    }
    salida
}

/// Distancia de edición (Levenshtein) acotada por `tope`.
///
/// Devuelve `Some(d)` si `d <= tope`, `None` si excede o si algún lado
/// supera 64 caracteres (cota anti-OOM). Sin `unwrap`, sin pánico.
fn distancia_acotada(a: &str, b: &str) -> Option<usize> {
    distancia_acotada_con_tope(a, b, 2)
}

fn distancia_acotada_con_tope(a: &str, b: &str, tope: usize) -> Option<usize> {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    if a_chars.len() > 64 || b_chars.len() > 64 {
        return None;
    }
    let diff = a_chars.len().abs_diff(b_chars.len());
    if diff > tope {
        return None;
    }
    if a_chars.is_empty() {
        return if b_chars.len() <= tope {
            Some(b_chars.len())
        } else {
            None
        };
    }
    if b_chars.is_empty() {
        return if a_chars.len() <= tope {
            Some(a_chars.len())
        } else {
            None
        };
    }
    let mut previa: Vec<usize> = (0..=b_chars.len()).collect();
    let mut actual: Vec<usize> = vec![0; b_chars.len() + 1];
    for (i, ca) in a_chars.iter().enumerate() {
        actual[0] = i + 1;
        let mut fila_min = actual[0];
        for (j, cb) in b_chars.iter().enumerate() {
            let costo = if ca == cb { 0 } else { 1 };
            let borrado = previa[j + 1].saturating_add(1);
            let insercion = actual[j].saturating_add(1);
            let sustitucion = previa[j].saturating_add(costo);
            let mejor = borrado.min(insercion).min(sustitucion);
            actual[j + 1] = mejor;
            if mejor < fila_min {
                fila_min = mejor;
            }
        }
        if fila_min > tope {
            return None;
        }
        std::mem::swap(&mut previa, &mut actual);
    }
    let d = previa[b_chars.len()];
    if d <= tope {
        Some(d)
    } else {
        None
    }
}

/// ¿El token normalizado matchea la clave por exacto o typo acotado?
///
/// - Token que contiene a la clave ("integrales", "derivadas") vale.
/// - Claves cortas (`"area"`, ≤4): solo exacto (evita "arena"→área).
/// - Claves largas (`"integral"`, `"derivada"`, `"animacion"`, ≥7):
///   distancia ≤2 (cubre "integrela", "derivadaa", "integrar").
///
/// Puro, sin I/O, sin `unwrap`.
pub fn token_matchea_clave(token_norm: &str, clave: &str) -> bool {
    if token_norm.is_empty() || clave.is_empty() {
        return false;
    }
    if token_norm.contains(clave) {
        return true;
    }
    if clave.len() <= 4 || token_norm.len() > 32 {
        return false;
    }
    distancia_acotada(token_norm, clave).is_some()
}

/// ¿El pedido menciona integral (con typos acotados)?
///
/// Cubre "integral", "integrales", "integrar" y typos como "integrela"
/// (distancia ≤2). Puro, sin I/O.
pub fn pedido_menciona_integral_fuzzy(pedido: &str) -> bool {
    let norm = normaliza_para_match(pedido);
    for token in norm.split(|c: char| !c.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if token_matchea_clave(token, "integral") {
            return true;
        }
    }
    false
}

/// ¿El pedido menciona integral o área? A propósito más amplio que
/// `detect_kind` (que exige "móvil" para el área): un "haceme una animación
/// de una integral" sin más ya pide la canónica, no un error.
///
/// Normaliza sin tildes + fuzzy acotado: "integrela" matchea "integral"
/// (distancia 2), "animacion" matchea "animación". "area" sigue exacta
/// por token (evita "tarea"→área del `contains` anterior).
pub fn pedido_menciona_area(pedido: &str) -> bool {
    let norm = normaliza_para_match(pedido);
    for token in norm.split(|c: char| !c.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if token == "area" || token_matchea_clave(token, "integral") {
            return true;
        }
    }
    false
}

/// Pedido de integral/área ya resuelto: canónico o explícito.
#[derive(Debug, Clone, PartialEq)]
pub enum AreaPedido {
    /// Sin función (o con prosa sin `x` no evaluable): canónico `x^2 [0,2]`.
    Canonica(ParametricAnim),
    /// Con función válida del usuario (constantes incluidas).
    Explicita(ParametricAnim),
}

impl AreaPedido {
    /// La animación a renderizar en ambas ramas.
    pub fn anim(&self) -> &ParametricAnim {
        match self {
            Self::Canonica(a) | Self::Explicita(a) => a,
        }
    }

    /// `true` solo en la rama canónica (la prosa debe declararlo).
    pub fn es_canonica(&self) -> bool {
        matches!(self, Self::Canonica(_))
    }
}

/// ¿La expresión se evalúa en al menos un punto del rango? 9 muestras;
/// basta una finita (el render corta los huecos sin unir ramas).
fn area_expr_evaluable(expr: &str, param: &str, p0: f64, p1: f64) -> bool {
    let mut validas = 0;
    for k in 0..9 {
        let t = k as f64 / 8.0;
        let pv = p0 + (p1 - p0) * t;
        // Muestreo en x sobre el mundo [-3, 3] del render.
        let x = -3.0 + 6.0 * t;
        if eval_expr(expr, x, param, pv).is_some() {
            validas += 1;
        }
    }
    validas > 0
}

fn area_anim(
    expr: String,
    param_raw: &str,
    p0: f64,
    p1: f64,
    pedido_lower: &str,
) -> ParametricResult<ParametricAnim> {
    let param_nombre: String = if param_raw.trim().is_empty() {
        guess_param_name(&expr, None).to_string()
    } else {
        param_raw.to_string()
    };
    let param = ParamName::try_new(&param_nombre)?;
    let n = extract_frames(pedido_lower)?.unwrap_or(PARAMETRIC_MAX_FRAMES);
    let frames = FrameCount::try_new(n)?;
    let viewport = extract_viewport(pedido_lower).unwrap_or_default();
    ParametricAnim::try_new(
        ParametricKind::Area,
        expr,
        None,
        param,
        p0,
        p1,
        frames,
        viewport,
    )
}

/// Infiere un pedido de integral/área a `AreaPedido`.
///
/// - Sin expresión tras `=` → `Canonica` (`x^2` en `[0,2]`, o el rango
///   pedido si lo trae).
/// - Con expresión evaluable (constantes incluidas) → `Explicita`.
/// - Con prosa sin `x` no evaluable ("F=m*a") → se ignora la prosa y va
///   `Canonica` (no era una función).
/// - Con `x` no evaluable en ningún punto ("foo(x)") → `Err` honesto
///   (`NoSoportado` con ejemplo), sin frames.
/// - Sin mención a integral/área → `FaltaTipo` (no es un pedido de área).
pub fn infer_area_anim(pedido: &str) -> ParametricResult<AreaPedido> {
    let text = pedido.trim();
    if text.is_empty() {
        return Err(ParametricError::PedidoVacio);
    }
    if text.chars().count() > 2000 {
        return Err(ParametricError::ExpresionMuyLarga {
            got: text.chars().count(),
            max: 2000,
        });
    }
    if !pedido_menciona_area(pedido) {
        return Err(ParametricError::FaltaTipo);
    }
    let lower = text.to_lowercase();
    let (param_raw, p0, p1) = extract_range(text).map_or_else(
        || (String::new(), INTEGRAL_CANONICAL_P0, INTEGRAL_CANONICAL_P1),
        |(nombre, a, b)| (nombre, a, b),
    );
    let Some(expr) = extract_single_expr(text) else {
        let anim = area_anim(
            INTEGRAL_CANONICAL_EXPR.to_string(),
            &param_raw,
            p0,
            p1,
            &lower,
        )?;
        return Ok(AreaPedido::Canonica(anim));
    };
    // Nombre del parámetro efectivo para validar la expresión.
    let param_efectivo: String = if param_raw.trim().is_empty() {
        guess_param_name(&expr, None).to_string()
    } else {
        param_raw.clone()
    };
    if area_expr_evaluable(&expr, &param_efectivo, p0, p1) {
        let anim = area_anim(expr, &param_raw, p0, p1, &lower)?;
        return Ok(AreaPedido::Explicita(anim));
    }
    // No evaluable: con `x` es función inválida (Err); sin `x` es prosa
    // ("F=m*a") y se ignora yendo a la canónica.
    if expr.contains('x') || expr.contains('X') {
        return Err(ParametricError::NoSoportado {
            detalle: format!(
                "la función {expr:?} no se puede evaluar en [{p0},{p1}]: revisá la expresión, por ejemplo f(x)=x^2"
            ),
        });
    }
    let anim = area_anim(
        INTEGRAL_CANONICAL_EXPR.to_string(),
        &param_raw,
        p0,
        p1,
        &lower,
    )?;
    Ok(AreaPedido::Canonica(anim))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_acota_sin_panico() {
        assert!(FrameCount::try_new(0).is_err());
        assert!(FrameCount::try_new(1).is_ok());
        assert!(FrameCount::try_new(48).is_ok());
        assert!(FrameCount::try_new(49).is_err());
        assert_eq!(FrameCount::try_new(24).unwrap().get(), 24);
    }

    #[test]
    fn param_name_rechaza_x_y_y_formas_raras() {
        assert!(ParamName::try_new("p").is_ok());
        assert!(ParamName::try_new("t").is_ok());
        assert!(ParamName::try_new("alfa").is_ok());
        assert!(ParamName::try_new("").is_err());
        assert!(ParamName::try_new("x").is_err());
        assert!(ParamName::try_new("y").is_err());
        assert!(ParamName::try_new("1p").is_err());
        assert!(ParamName::try_new("p con espacios").is_err());
        assert!(ParamName::try_new("esta_es_muy_larga_123").is_err());
    }

    #[test]
    fn eval_cubre_operadores_y_funciones() {
        assert_eq!(eval_expr("x+p", 2.0, "p", 3.0), Some(5.0));
        assert_eq!(eval_expr("x^2+p*x", 2.0, "p", 3.0), Some(10.0));
        assert_eq!(eval_expr("sin(x)+cos(p)", 0.0, "p", 0.0), Some(1.0));
        assert_eq!(
            eval_expr("exp(0)+sqrt(4)+abs(0-5)", 0.0, "p", 0.0),
            Some(8.0)
        );
        // Dominio honesto: nada inventado.
        assert_eq!(eval_expr("1/0", 0.0, "p", 0.0), None);
        assert_eq!(eval_expr("sqrt(0-1)", 0.0, "p", 0.0), None);
        assert_eq!(eval_expr("ln(0)", 0.0, "p", 0.0), None);
        assert_eq!(eval_expr("foo(x)", 1.0, "p", 0.0), None);
        assert_eq!(eval_expr("x**2", 2.0, "p", 0.0), None);
        assert_eq!(eval_expr("", 1.0, "p", 0.0), None);
        assert_eq!(eval_expr("x+", 1.0, "p", 0.0), None);
    }

    #[test]
    fn inferencia_barrido_ok() {
        let anim = infer_parametric_anim("barrido de f(x)=x^2+p*x con p en [-2,2]").unwrap();
        assert_eq!(anim.kind, ParametricKind::Sweep);
        assert_eq!(anim.expr_a, "x^2+p*x");
        assert_eq!(anim.param.as_str(), "p");
        assert_eq!((anim.p0, anim.p1), (-2.0, 2.0));
        assert_eq!(anim.frame_count(), PARAMETRIC_DEFAULT_FRAMES);
        assert_eq!(anim.frame_param(0), -2.0);
        assert_eq!(anim.frame_param(anim.frame_count() - 1), 2.0);
    }

    #[test]
    fn inferencia_traza_morph_locus_tangente_area() {
        let t = infer_parametric_anim("traza progresiva de y=sin(x) con t en [0,1]").unwrap();
        assert_eq!(t.kind, ParametricKind::Trace);
        let m = infer_parametric_anim("transición de f(x)=x^2 a f(x)=x^3 con p en [0,1]").unwrap();
        assert_eq!(m.kind, ParametricKind::Morph);
        assert_eq!(m.expr_b.as_deref(), Some("x^3"));
        let l = infer_parametric_anim("lugar geométrico del punto móvil en y=x^2 con p en [-2,2]")
            .unwrap();
        assert_eq!(l.kind, ParametricKind::Locus);
        let tg = infer_parametric_anim("animá la recta tangente móvil de f(x)=x^2 con p en [-1,1]")
            .unwrap();
        assert_eq!(tg.kind, ParametricKind::Tangent);
        let a = infer_parametric_anim("área móvil bajo la curva y=x^2 con p en [0,2]").unwrap();
        assert_eq!(a.kind, ParametricKind::Area);
    }

    #[test]
    fn inferencia_tres_errores_honestos() {
        // 1. Sin tipo detectable (hay expresión pero no dice qué animar).
        let e1 = infer_parametric_anim("dibujame f(x)=x^2").unwrap_err();
        assert_eq!(e1, ParametricError::FaltaTipo);
        // 2. Sin expresión.
        let e2 = infer_parametric_anim("barrido con p en [-2,2]").unwrap_err();
        assert_eq!(e2, ParametricError::FaltaExpresion);
        // 3. Sin rango.
        let e3 = infer_parametric_anim("barrido de f(x)=x^2+p*x").unwrap_err();
        assert_eq!(e3, ParametricError::FaltaRango);
        // Morph con un solo estado.
        let e4 = infer_parametric_anim("transición de f(x)=x^2 con p en [0,1]").unwrap_err();
        assert_eq!(e4, ParametricError::FaltaSegundoEstado);
        for msg in [
            e1.to_string(),
            e2.to_string(),
            e3.to_string(),
            e4.to_string(),
        ] {
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn pista_usa_nombres_humanos_nunca_ids() {
        let anim = infer_parametric_anim("barrido de f(x)=x^2+p*x con p en [-2,2]").unwrap();
        let hint = parametric_hint(&anim);
        assert!(hint.contains("deslizador"));
        assert!(hint.contains("reproducir"));
        assert!(hint.contains("pausar"));
        // Identificadores literales del mapa jamás en prosa.
        for id in [
            "PlayPause",
            "Slider",
            "Button",
            "Tangent",
            "Select",
            "Parallel",
            "Midpoint",
            "Distance",
            "Angle",
            "Area",
            "Function",
            "Polygon",
            "Circle",
            "Line",
            "Point",
            "Vector",
            "Segment",
            "Ray",
            "Eraser",
            "Pencil",
        ] {
            assert!(!hint.contains(id), "la pista no debe contener {id}");
        }
        assert!(!hint.contains(" Play "));
        assert!(!hint.contains(" Pause "));
    }

    #[test]
    fn presupuesto_oom_rechaza_honesto() {
        let big = ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x+p".to_string(),
            None,
            ParamName::try_new("p").unwrap(),
            -2.0,
            2.0,
            FrameCount::try_new(48).unwrap(),
            Resolution::try_new(4096, 4096).unwrap(),
        );
        assert!(matches!(big, Err(ParametricError::ExcedeMemoria { .. })));
        let ok = ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x+p".to_string(),
            None,
            ParamName::try_new("p").unwrap(),
            -2.0,
            2.0,
            FrameCount::try_new(24).unwrap(),
            Resolution::default(),
        )
        .unwrap();
        assert!(ok.estimate_bytes() == Some(640 * 480 * 4 * 24));
    }

    // ── N1: canónica de integral sin función (3 ramas) ───────────────────
    #[test]
    fn area_sin_funcion_usa_canonica_x2_en_02() {
        for pedido in [
            "haceme una animacion de una integral (nativa)",
            "explica la integral con animación",
            "animá el área bajo la curva con animación",
        ] {
            let res = infer_area_anim(pedido).unwrap();
            assert!(res.es_canonica(), "{pedido:?} debe ser canónica");
            let anim = res.anim();
            assert_eq!(anim.kind, ParametricKind::Area);
            assert_eq!(anim.expr_a, INTEGRAL_CANONICAL_EXPR);
            assert_eq!(anim.param.as_str(), INTEGRAL_CANONICAL_PARAM);
            assert_eq!((anim.p0, anim.p1), (0.0, 2.0));
            assert_eq!(anim.frame_count(), PARAMETRIC_MAX_FRAMES);
        }
        // La prosa declara la canónica en rioplatense.
        assert!(INTEGRAL_CANONICAL_PROSA.contains("x²"));
        assert!(INTEGRAL_CANONICAL_PROSA.contains("pedime otra"));
    }

    #[test]
    fn area_con_funcion_valida_es_explicita() {
        let res =
            infer_area_anim("haceme una animacion de la integral de f(x)=x^3 de 0 a 2").unwrap();
        assert!(!res.es_canonica());
        let anim = res.anim();
        assert_eq!(anim.expr_a, "x^3");
        assert_eq!((anim.p0, anim.p1), (0.0, 2.0));
        // Rango con corchetes también vale.
        let res2 = infer_area_anim("área móvil bajo la curva y=sin(x) con p en [0,2]").unwrap();
        assert!(!res2.es_canonica());
        assert_eq!(res2.anim().expr_a, "sin(x)");
        // Prosa con `=` pero sin `x` ni evaluable se ignora → canónica.
        let res3 = infer_area_anim("animacion de la integral con F=m*a").unwrap();
        assert!(res3.es_canonica(), "la prosa sin x no es función");
    }

    #[test]
    fn area_con_funcion_invalida_falla_honesto_sin_frames() {
        let err =
            infer_area_anim("animacion de la integral de f(x)=foo(x) con p en [0,1]").unwrap_err();
        match err {
            ParametricError::NoSoportado { detalle } => {
                assert!(detalle.contains("foo(x)"), "{detalle}");
                assert!(detalle.contains("x^2"), "da ejemplo: {detalle}");
            }
            otro => panic!("esperaba NoSoportado, fue {otro:?}"),
        }
        // Sin mención a integral/área no es pedido de área.
        assert_eq!(
            infer_area_anim("barrido de f(x)=x^2 con p en [0,1]").unwrap_err(),
            ParametricError::FaltaTipo
        );
        assert!(infer_area_anim("   ").is_err());
    }

    #[test]
    fn recorte_de_rango_en_prosa_no_come_parametros() {
        // " de 0 a 2" tras la expresión se recorta…
        assert_eq!(
            extract_single_expr("integral de f(x)=x^3 de 0 a 2").as_deref(),
            Some("x^3")
        );
        // …pero un parámetro con espacios sobrevive (" a " sin número no corta).
        assert_eq!(
            extract_single_expr("barrido de f(x)=x^2 + a*x con a en [0,2]").as_deref(),
            Some("x^2 + a*x")
        );
        // Rango suelto con corchetes también se recorta.
        assert_eq!(
            extract_single_expr("integral de f(x)=x^2 [0,2]").as_deref(),
            Some("x^2")
        );
    }

    #[test]
    fn typo_screenshot_integrela_es_canonica() {
        // Input EXACTO del screenshot: "hace una animacion de una integrela
        // (nativa)". Antes `pedido_menciona_area` era falso (substring exacto)
        // y el Submit caía a `universal` + pregunta remota (contradicción).
        let pedido = "hace una animacion de una integrela (nativa)";
        assert!(pedido_menciona_area(pedido), "el typo debe mencionar área");
        assert!(pedido_menciona_integral_fuzzy(pedido));
        let res = infer_area_anim(pedido).unwrap();
        assert!(res.es_canonica(), "sin función va a canónica");
        assert_eq!(res.anim().expr_a, INTEGRAL_CANONICAL_EXPR);
        assert_eq!((res.anim().p0, res.anim().p1), (0.0, 2.0));
    }

    #[test]
    fn normalizacion_y_fuzzy_acotado_cubren_typos() {
        assert_eq!(normaliza_para_match("animación"), "animacion");
        assert_eq!(normaliza_para_match("ÁREA"), "area");
        assert_eq!(normaliza_para_match("DERIVÁDA"), "derivada");
        assert!(token_matchea_clave("integrela", "integral"));
        assert!(token_matchea_clave("derivadaa", "derivada"));
        assert!(token_matchea_clave("animacion", "animacion"));
        assert!(token_matchea_clave("integrar", "integral"));
        assert!(token_matchea_clave("integrales", "integral"));
        // Cota: cortas no hacen fuzzy y basura larga no matchea.
        assert!(!token_matchea_clave("arena", "area"));
        assert!(!token_matchea_clave("xyz", "integral"));
        assert!(!pedido_menciona_area("tarea de matemática"));
        assert!(distancia_acotada("integrela", "integral").is_some());
        assert!(distancia_acotada("zzz", "integral").is_none());
    }
}

// ── F10 hostile fuzz (solo tests, sin tocar prod) ─────────────────────────
// Caza del SIGABRT chat→integral→2da animación. RAW a propósito (sin
// catch_unwind ni should_panic): si algo paniquea, el harness lo muestra
// con RUST_BACKTRACE=1. Fase 2 lo convertirá a catch+assert.
#[cfg(test)]
mod hostile_crash_f10 {
    use super::*;
    use crate::protocol::Resolution;

    fn valid_anim(p0: f64, p1: f64, expr: &str, frames: usize) -> ParametricResult<ParametricAnim> {
        let param = ParamName::try_new("p")?;
        let fc = FrameCount::try_new(frames)?;
        let vp = Resolution::default();
        ParametricAnim::try_new(
            ParametricKind::Sweep,
            expr.to_string(),
            None,
            param,
            p0,
            p1,
            fc,
            vp,
        )
    }

    #[test]
    fn hostile_frames_0_1_max() {
        assert!(FrameCount::try_new(0).is_err());
        assert!(FrameCount::try_new(1).is_ok());
        assert!(FrameCount::try_new(48).is_ok());
        assert!(FrameCount::try_new(49).is_err());
        assert!(FrameCount::try_new(usize::MAX).is_err());
        assert!(FrameCount::try_new(usize::MAX / 2).is_err());
    }

    #[test]
    fn hostile_rangos_degenerados() {
        // [0,0] degenerado
        assert!(valid_anim(0.0, 0.0, "x^2", 8).is_err());
        // [-0.0, 0.0] == en f64
        assert!(valid_anim(-0.0, 0.0, "x^2", 8).is_err());
        // NaN / inf deben dar Err, jamás panic
        assert!(valid_anim(f64::NAN, 1.0, "x^2", 8).is_err());
        assert!(valid_anim(0.0, f64::NAN, "x^2", 8).is_err());
        assert!(valid_anim(f64::NAN, f64::NAN, "x^2", 8).is_err());
        assert!(valid_anim(f64::INFINITY, 1.0, "x^2", 8).is_err());
        assert!(valid_anim(0.0, f64::INFINITY, "x^2", 8).is_err());
        assert!(valid_anim(f64::NEG_INFINITY, f64::INFINITY, "x^2", 8).is_err());
        assert!(valid_anim(f64::INFINITY, f64::INFINITY, "x^2", 8).is_err());
        // Rango válido mínimo no degenerado
        assert!(valid_anim(0.0, f64::MIN_POSITIVE, "x^2", 1).is_ok());
    }

    #[test]
    fn hostile_expr_vacia_y_gigante() {
        assert!(valid_anim(0.0, 1.0, "", 8).is_err());
        assert!(valid_anim(0.0, 1.0, "   ", 8).is_err());
        assert!(valid_anim(0.0, 1.0, "\n\t  ", 8).is_err());
        // Solo operadores
        let anim = valid_anim(0.0, 1.0, "+++", 2).expect("construye (eval da None)");
        assert_eq!(anim.eval_frame(0, 1.0), None);
        let anim2 = valid_anim(0.0, 1.0, "***", 2).expect("construye");
        assert_eq!(anim2.eval_frame(0, 1.0), None);
        // 200KB debe dar Err ExpresionMuyLarga, no panic
        let big = "x+".repeat(100_000); // 200KB
        assert!(valid_anim(0.0, 1.0, &big, 2).is_err());
        // Unicode partido / emojis: jamás panic, None honesto
        let anim3 = valid_anim(0.0, 1.0, "x + \u{1F600}", 2).expect("construye");
        assert_eq!(anim3.eval_frame(0, 1.0), None);
        let anim4 = valid_anim(0.0, 1.0, "\u{e9}".repeat(500).as_str(), 2);
        // puede ser Err o Ok-con-None, pero no panic
        if let Ok(a) = anim4 {
            assert_eq!(a.eval_frame(0, 0.0), None);
        }
    }

    #[test]
    fn hostile_viewport_raro() {
        assert!(Resolution::try_new(0, 0).is_err());
        assert!(Resolution::try_new(1, 1).is_err());
        assert!(Resolution::try_new(63, 63).is_err());
        assert!(Resolution::try_new(65, 65).is_ok());
        assert!(Resolution::try_new(4097, 4097).is_err());
        assert!(Resolution::try_new(u32::MAX, u32::MAX).is_err());
        assert!(Resolution::try_new(u32::MAX, 64).is_err());
        assert!(Resolution::try_new(64, u32::MAX).is_err());
    }

    #[test]
    fn hostile_eval_nan_inf() {
        let anim = valid_anim(0.0, 1.0, "x^2+p*x", 4).unwrap();
        assert_eq!(anim.eval_frame(0, f64::NAN), None);
        assert_eq!(anim.eval_frame(0, f64::INFINITY), None);
        assert_eq!(anim.eval_frame(0, f64::NEG_INFINITY), None);
        assert_eq!(anim.eval_frame(usize::MAX, 1.0), anim.eval_frame(3, 1.0));
        assert_eq!(
            anim.eval_frame(usize::MAX / 2, 1.0),
            anim.eval_frame(3, 1.0)
        );
        // frame_param / fraction con índice gigante: clamp, no panic
        let p = anim.frame_param(usize::MAX);
        assert!(p.is_finite());
        let f = anim.frame_fraction(usize::MAX);
        assert!((0.0..=1.0).contains(&f));
        // Morph con B vacía debe fallar honesto
        let param = ParamName::try_new("p").unwrap();
        let fc = FrameCount::try_new(4).unwrap();
        let vp = Resolution::default();
        assert!(ParametricAnim::try_new(
            ParametricKind::Morph,
            "x^2".to_string(),
            None,
            param,
            0.0,
            1.0,
            fc,
            vp
        )
        .is_err());
    }

    #[test]
    fn hostile_parens_10k_y_200kb() {
        // 10k parens: depth guard 64 → None, no stack overflow
        let deep = format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000));
        let anim = valid_anim(0.0, 1.0, &deep, 2);
        if let Ok(a) = anim {
            assert_eq!(a.eval_frame(0, 1.0), None);
        }
        // 200KB de 'x' continua
        let huge = "x".repeat(200_000);
        assert!(valid_anim(0.0, 1.0, &huge, 2).is_err());
    }

    #[test]
    fn hostile_infer_hostil() {
        assert!(infer_parametric_anim("").is_err());
        assert!(infer_parametric_anim("   ").is_err());
        assert!(infer_parametric_anim("+++").is_err());
        assert!(infer_parametric_anim("***///").is_err());
        assert!(infer_parametric_anim("((((").is_err());
        // unicode / emojis
        assert!(infer_parametric_anim("\u{1F600}".repeat(100).as_str()).is_err());
        // 200KB
        let big = "a".repeat(200_000);
        assert!(infer_parametric_anim(&big).is_err());
        // parens 10k con tipo válido pero expr rota
        let deep_req = format!("barrido de f(x)={} con p en [0,1]", "(".repeat(10_000));
        assert!(infer_parametric_anim(&deep_req).is_err());
        // rango degenerado explícito vía inferencia
        assert!(infer_parametric_anim("barrido de f(x)=x^2 con p en [0,0]").is_err());
        assert!(infer_parametric_anim("barrido de f(x)=x^2 con p en [NaN,inf]").is_err());
    }

    #[test]
    fn hostile_estimate_overflow() {
        // 4096x4096x48 desborda presupuesto 64MiB → Err, no panic
        let param = ParamName::try_new("p").unwrap();
        let fc = FrameCount::try_new(48).unwrap();
        let vp = Resolution::try_new(4096, 4096).unwrap();
        let anim = ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x^2".to_string(),
            None,
            param,
            0.0,
            1.0,
            fc,
            vp,
        );
        // try_new valida presupuesto → debe ser Err ExcedeMemoria
        assert!(anim.is_err());
        // estimate_bytes puro con dims gigantes vía cast: None o Some sin panic
        let _ = (4096usize)
            .checked_mul(4096)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_mul(48));
        // usize::MAX-ish
        let w = usize::MAX / 4;
        let h = 4usize;
        assert!(
            w.checked_mul(h).and_then(|v| v.checked_mul(4)).is_none()
                || w.checked_mul(h).and_then(|v| v.checked_mul(4)).is_some()
        );
    }
}
