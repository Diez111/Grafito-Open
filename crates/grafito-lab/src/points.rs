//! `points`: parseo de puntos `.pt`/`.vtx` y JSON.
//!
//! Port de `lab/parse874.py` (`parse_pt` + clase `P`: enteros, fracciones,
//! `Sqrt`, `+-*/`, paréntesis) y de `lab/hn_hunt.py` (`load_points_json`).
//! Devuelve `Vec<Point2>` (`grafito-geometry`) con errores honestos.
//!
//! NO portado (a propósito): `KISSAT = ...`, `sys.path.insert("/tmp/opencode")`
//! y el `import parse874` de `hn_hunt.py` — hardcodes de máquina del autor.
//! El runner SAT vive en `grafito-mcp::sat`; este módulo es puro, sin I/O
//! ni subprocess.
//!
//! Divergencias estrictas vs el `.py` (documentadas, no silenciosas):
//! - El `.py` ignora tokens sobrantes (`P(...).expr()` no chequea el resto);
//!   acá sobrantes → `Trailing`.
//! - El `.py` no tokeniza `.` (un `0.5` se evalúa mal en silencio); acá los
//!   decimales con punto están soportados.

use grafito_geometry::Point2;
use thiserror::Error;

/// Errores del parser de puntos, con línea (1-based) cuando aplica.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PointsError {
    /// La línea no está envuelta en `{...}`.
    #[error("puntos línea {line}: se esperaba '{{...}}', vino '{preview}'")]
    BadBraces { line: usize, preview: String },
    /// No hay coma top-level que separe `x` de `y`.
    #[error("puntos línea {line}: sin coma top-level en '{preview}'")]
    NoTopComma { line: usize, preview: String },
    /// Falló el tokenizer o la gramática de una coordenada.
    #[error("puntos línea {line}: expresión inválida ({msg})")]
    Expr { line: usize, msg: String },
    /// Sobraron tokens tras evaluar la coordenada (el `.py` los tragaba).
    #[error("puntos línea {line}: tokens sobrantes tras la expresión")]
    Trailing { line: usize },
    /// Alguna coordenada no es finita (`inf`/`NaN`, p. ej. división por cero).
    #[error("puntos línea {line}: coordenada no finita")]
    NonFinite { line: usize },
    /// El texto no es JSON válido.
    #[error("puntos json: {msg}")]
    Json { msg: String },
    /// El JSON no es una lista de pares `[x, y]`.
    #[error("puntos json[{index}]: {msg}")]
    JsonShape { index: usize, msg: String },
    /// Coordenada JSON no finita.
    #[error("puntos json[{index}]: coordenada no finita")]
    JsonNonFinite { index: usize },
}

/// Token del mini-lenguaje estilo Mathematica (`parse874.P`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Sqrt,
    Num(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    LBrack,
    RBrack,
}

/// Tokeniza una coordenada. Soporta `Sqrt`, números (con o sin punto),
/// `+-*/`, `()` y `[]`. Cualquier otro char → `Err`.
fn tokenize(s: &str, line: usize) -> Result<Vec<Tok>, PointsError> {
    let err = |msg: &str| PointsError::Expr {
        line,
        msg: msg.into(),
    };
    let bytes = s.as_bytes();
    // Hot path: pre-reserva (~1 token cada 4 bytes) para evitar 2-3
    // reallocs por coordenada en fixtures grandes (2000 líneas).
    let mut toks = Vec::with_capacity(bytes.len() / 4 + 4);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c == 'S' && s[i..].starts_with("Sqrt") {
            toks.push(Tok::Sqrt);
            i += 4;
        } else if c.is_ascii_digit() || c == '.' {
            let mut j = i + 1;
            while j < bytes.len() {
                let d = bytes[j];
                if d.is_ascii_digit() || d == b'.' {
                    j += 1;
                } else {
                    break;
                }
            }
            let num: f64 = s[i..j].parse().map_err(|_| err("número inválido"))?;
            toks.push(Tok::Num(num));
            i = j;
        } else {
            let t = match c {
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' => Tok::Star,
                '/' => Tok::Slash,
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                '[' => Tok::LBrack,
                ']' => Tok::RBrack,
                _ => return Err(err("carácter inesperado")),
            };
            toks.push(t);
            i += 1;
        }
    }
    Ok(toks)
}

/// Parser recursivo `expr → term → fact`, espejo de `parse874.P`.
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    line: usize,
}

impl Parser {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).copied()
    }

    fn next(&mut self) -> Result<Tok, PointsError> {
        let t = self.peek().ok_or(PointsError::Expr {
            line: self.line,
            msg: "fin inesperado de la expresión".into(),
        })?;
        self.pos += 1;
        Ok(t)
    }

    fn expr(&mut self) -> Result<f64, PointsError> {
        let mut v = self.term()?;
        while matches!(self.peek(), Some(Tok::Plus | Tok::Minus)) {
            let op = self.next()?;
            let w = self.term()?;
            v = if op == Tok::Plus { v + w } else { v - w };
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<f64, PointsError> {
        let mut v = self.fact()?;
        while matches!(self.peek(), Some(Tok::Star | Tok::Slash)) {
            let op = self.next()?;
            let w = self.fact()?;
            v = if op == Tok::Star { v * w } else { v / w };
        }
        Ok(v)
    }

    fn fact(&mut self) -> Result<f64, PointsError> {
        let line = self.line;
        let bad = |msg: &str| PointsError::Expr {
            line,
            msg: msg.into(),
        };
        match self.next()? {
            Tok::Minus => Ok(-self.fact()?),
            Tok::Plus => self.fact(),
            Tok::Sqrt => {
                if self.next()? != Tok::LBrack {
                    return Err(bad("se esperaba '[' tras Sqrt"));
                }
                let v = self.expr()?;
                if self.next()? != Tok::RBrack {
                    return Err(bad("se esperaba ']'"));
                }
                Ok(v.sqrt())
            }
            Tok::LParen => {
                let v = self.expr()?;
                if self.next()? != Tok::RParen {
                    return Err(bad("se esperaba ')'"));
                }
                Ok(v)
            }
            Tok::Num(v) => Ok(v),
            _ => Err(bad("token inesperado")),
        }
    }
}

/// Evalúa una coordenada y exige consumir todos los tokens.
fn eval_coord(s: &str, line: usize) -> Result<f64, PointsError> {
    let toks = tokenize(s, line)?;
    let mut p = Parser { toks, pos: 0, line };
    let v = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(PointsError::Trailing { line });
    }
    Ok(v)
}

/// Corta la línea en la primera coma top-level (profundidad de `[(`
/// balanceada), espejo de `parse_pt`.
fn split_top_comma(inner: &str) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => return Some((i, i + ch.len_utf8())),
            _ => {}
        }
    }
    None
}

fn preview(s: &str) -> String {
    s.chars().take(60).collect()
}

fn parse_one(line: &str, nline: usize) -> Result<Point2, PointsError> {
    let t = line.trim();
    if !t.starts_with('{') || !t.ends_with('}') {
        return Err(PointsError::BadBraces {
            line: nline,
            preview: preview(t),
        });
    }
    let inner = &t[1..t.len() - 1];
    let Some((a, b)) = split_top_comma(inner) else {
        return Err(PointsError::NoTopComma {
            line: nline,
            preview: preview(t),
        });
    };
    let x = eval_coord(&inner[..a], nline)?;
    let y = eval_coord(&inner[b..], nline)?;
    if !x.is_finite() || !y.is_finite() {
        return Err(PointsError::NonFinite { line: nline });
    }
    Ok(Point2::new(x, y))
}

/// Parsea una línea `{x_expr, y_expr}` estilo `parse874.parse_pt`.
///
/// Los errores reportan línea 1 (es la única línea de este input).
pub fn parse_pt_line(line: &str) -> Result<Point2, PointsError> {
    parse_one(line, 1)
}

/// Parsea un texto `.pt`/`.vtx` (una coordenada por línea).
///
/// Ignora líneas en blanco; cualquier otra línea inválida corta con `Err`
/// (el `.py` asumía 874 líneas vía `assert`, acá el conteo lo valida el
/// llamador).
pub fn parse_pt_text(text: &str) -> Result<Vec<Point2>, PointsError> {
    // Hot path: una pasada O(n) sobre bytes para reservar exacto y evitar
    // reallocs en fixtures de miles de líneas (misma semántica).
    let cap = text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;
    let mut out = Vec::with_capacity(cap);
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(parse_one(line, idx + 1)?);
    }
    Ok(out)
}

/// Parsea el JSON de puntos de `hn_hunt.load_points_json` (`[[x, y], ...]`).
///
/// Valida forma (pares) y finitud con índice honesto; el `.py` devolvía la
/// lista cruda sin validar.
pub fn parse_points_json(text: &str) -> Result<Vec<Point2>, PointsError> {
    let raw: Vec<Vec<f64>> =
        serde_json::from_str(text).map_err(|e| PointsError::Json { msg: e.to_string() })?;
    let mut out = Vec::with_capacity(raw.len());
    for (index, pair) in raw.iter().enumerate() {
        if pair.len() != 2 {
            return Err(PointsError::JsonShape {
                index,
                msg: format!("se esperaba [x, y], vinieron {} elementos", pair.len()),
            });
        }
        let (x, y) = (pair[0], pair[1]);
        if !x.is_finite() || !y.is_finite() {
            return Err(PointsError::JsonNonFinite { index });
        }
        out.push(Point2::new(x, y));
    }
    Ok(out)
}
