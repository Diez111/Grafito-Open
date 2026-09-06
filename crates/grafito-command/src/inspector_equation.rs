//! Ecuación editable del Inspector (frente D3).
//!
//! Pipeline completo testeable headless: objeto→texto canónico→parse→mismo
//! objeto, con validación (sintaxis + dominio + presupuesto) y update
//! validado que preserva `id` y deja el undo intacto.
//! No toca UI (`panels.rs` prohibido en D3): el widget vive en
//! `grafito-app::inspector_edit` y llama a este módulo.
//!
//! Formatos canónicos (los produce `GeoObject::canonical_equation_text`):
//! función `y = …`, paramétrica `(x, y[, z])`, implícita `lhs op rhs`
//! (`= < > <= >=`; sin operador → `F = 0`), recta `ax+by=c` (segmento o
//! semirrecta agregan extremos `[(x1,y1) - (x2,y2)]` o `[… -> …]`),
//! círculo `(x-h)^2+(y-k)^2=R`, cónicas estructuradas
//! (`elipse`/`parabola`/`hiperbola`), punto `(x, y)`,
//! polígono `[(x1,y1), …]`, texto `"…" @ (x, y)`, polar `r = …`,
//! superficie `z = …` / `(x,y,z)` / `|…|`. Resto → no editable honesto.

use grafito_core::{Document, GeoObject, ObjectId, RelationOperator};
use grafito_geometry::Point2;
use std::collections::HashMap;
use std::fmt;

/// Presupuesto de texto editable (igual que `validation::MAX_EXPR_LENGTH`).
pub const MAX_EQUATION_CHARS: usize = grafito_core::validation::MAX_EXPR_LENGTH;

/// Texto de ecuación ya validado en presupuesto. Newtype: no se construye
/// sin pasar por [`EquationText::try_new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquationText(String);

impl EquationText {
    /// Valida presupuesto (largo) y NUL. Sintaxis y dominio los valida el
    /// parse por tipo ([`parse_inspector_equation`]).
    pub fn try_new(raw: &str) -> Result<Self, EquationError> {
        if raw.len() > MAX_EQUATION_CHARS {
            return Err(EquationError::budget(raw.len()));
        }
        if let Some(byte_idx) = raw.find('\0') {
            let col = char_column(raw, byte_idx);
            return Err(EquationError::syntax(
                "hay un carácter NUL que no va".to_string(),
                Some(col),
            ));
        }
        Ok(Self(raw.to_string()))
    }

    /// Constructor para tests que ya garantizan presupuesto.
    #[cfg(test)]
    pub fn from_test(raw: &str) -> Self {
        Self(raw.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error honesto del pipeline D3, en español rioplatense y con columna
/// aproximada (1-based) cuando se conoce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquationError {
    /// Mensaje listo para mostrar en el Inspector.
    pub message: String,
    /// Columna aproximada (1-based, en caracteres) dentro del borrador.
    pub position: Option<usize>,
}

impl EquationError {
    fn syntax(detail: String, position: Option<usize>) -> Self {
        let message = match position {
            Some(col) => format!(
                "Che, no pude entender la ecuación cerca de la columna {col}: {detail}. Revisá paréntesis y operadores."
            ),
            None => format!(
                "Che, no pude entender la ecuación: {detail}. Revisá paréntesis y operadores."
            ),
        };
        Self { message, position }
    }

    fn domain(detail: String, position: Option<usize>) -> Self {
        let message = match position {
            Some(col) => format!("Ese valor no va (columna {col}): {detail}."),
            None => format!("Ese valor no va: {detail}."),
        };
        Self { message, position }
    }

    /// Presupuesto superado (largo del borrador).
    pub fn budget(provided: usize) -> Self {
        Self {
            message: format!(
                "Te pasaste del presupuesto: el texto tiene {provided} caracteres y el máximo es {MAX_EQUATION_CHARS}. Acortá la fórmula."
            ),
            position: None,
        }
    }

    /// Tipo sin edición por ecuación (honesto, jamás inventa).
    pub fn not_editable(kind: &str) -> Self {
        Self {
            message: format!(
                "Este objeto ({kind}) no se puede editar por ecuación todavía. Probá con función, recta, círculo, cónica, punto, polígono, texto, paramétrica, implícita, polar o superficie."
            ),
            position: None,
        }
    }

    fn not_found() -> Self {
        Self {
            message: "No encontré ese objeto, quizás se borró. Probá seleccionar de nuevo."
                .to_string(),
            position: None,
        }
    }
}

impl fmt::Display for EquationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EquationError {}

/// `true` si el tipo tiene ecuación canónica editable.
pub fn is_equation_editable(obj: &GeoObject) -> bool {
    obj.canonical_equation_text().is_some()
}

/// Parse inverso puro: borrador → objeto actualizado con la misma `id`.
/// No toca el documento: el llamador aplica con [`apply_inspector_equation`].
/// Jamás pánico, jamás aplica a medias (o `Ok` completo o `Err` sin tocar nada).
pub fn parse_inspector_equation(
    original: &GeoObject,
    text: &str,
) -> Result<GeoObject, EquationError> {
    let _checked = EquationText::try_new(text)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(EquationError::domain(
            "la ecuación quedó vacía, escribí una fórmula o cancelá".to_string(),
            None,
        ));
    }
    match original {
        GeoObject::Function(_) => parse_function(original, trimmed),
        GeoObject::ParametricCurve2D(_) => parse_parametric_2d(original, trimmed),
        GeoObject::ParametricCurve3D(_) => parse_parametric_3d(original, trimmed),
        GeoObject::ImplicitCurve(_) => parse_implicit(original, trimmed),
        GeoObject::Line(_) => parse_line(original, trimmed),
        GeoObject::Circle(_) => parse_circle(original, trimmed),
        GeoObject::Ellipse(_) => parse_ellipse(original, trimmed),
        GeoObject::Parabola(_) => parse_parabola(original, trimmed),
        GeoObject::Hyperbola(_) => parse_hyperbola(original, trimmed),
        GeoObject::Point(_) => parse_point_obj(original, trimmed),
        GeoObject::Polygon(_) => parse_polygon(original, trimmed),
        GeoObject::Text(_) => parse_text_obj(original, trimmed),
        GeoObject::PolarCurve(_) => parse_polar(original, trimmed),
        GeoObject::Surface3D(_) => parse_surface(original, trimmed),
        other => Err(EquationError::not_editable(other.name())),
    }
}

/// Aplica el borrador al documento preservando `id`.
/// Usa `try_replace_object_with_previous` (validación atómica sobre copia):
/// si falla, el documento queda intacto. Devuelve `true` si cambió.
pub fn apply_inspector_equation(
    doc: &mut Document,
    id: ObjectId,
    text: &str,
) -> Result<bool, EquationError> {
    Ok(apply_inspector_equation_with_previous(doc, id, text)?.is_some())
}

/// Como [`apply_inspector_equation`] pero devuelve el documento previo para
/// que el llamador lo empuje al undo (`push_history_snapshot` en D1-bis).
/// `Ok(None)` = no-op (misma ecuación, nada que deshacer).
pub fn apply_inspector_equation_with_previous(
    doc: &mut Document,
    id: ObjectId,
    text: &str,
) -> Result<Option<Document>, EquationError> {
    let original = doc
        .get_object(id)
        .cloned()
        .ok_or_else(EquationError::not_found)?;
    let updated = parse_inspector_equation(&original, text)?;
    if updated.id() != id {
        return Err(EquationError::domain(
            "el objeto actualizado cambió de id, no lo aplico".to_string(),
            None,
        ));
    }
    doc.try_replace_object_with_previous(id, updated)
        .map_err(|detail| EquationError::domain(format!("no lo pude aplicar: {detail}"), None))
}

// ── Utilidades puras ────────────────────────────────────────────────

/// Columna 1-based (en caracteres) para un byte índice. Nunca pánico.
fn char_column(text: &str, byte_idx: usize) -> usize {
    let len = text.len();
    let mut safe = if byte_idx > len { len } else { byte_idx };
    while safe > 0 && text.get(..safe).is_none() {
        safe -= 1;
    }
    match text.get(..safe) {
        Some(prefix) => prefix.chars().count() + 1,
        None => 1,
    }
}

/// Extrae `byte offset N` de un mensaje del parser AST, si viene.
fn extract_offset(detail: &str) -> Option<usize> {
    let lower = detail.to_ascii_lowercase();
    let pos = lower.find("offset")?;
    let after = lower.get(pos + "offset".len()..)?;
    let mut num = String::new();
    let mut in_num = false;
    for c in after.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            in_num = true;
        } else if in_num {
            break;
        }
    }
    if num.is_empty() {
        None
    } else {
        num.parse::<usize>().ok()
    }
}

/// Primera columna con carácter fuera del alfabeto de expresiones.
fn approx_bad_char(expr: &str) -> Option<usize> {
    for (idx, c) in expr.char_indices() {
        let ok = c.is_whitespace()
            || c.is_alphanumeric()
            || c == '_'
            || c == '.'
            || "+-*/^(),<>=!|\"".contains(c);
        if !ok {
            return Some(char_column(expr, idx));
        }
    }
    None
}

/// Valida sintaxis de una subexpresión con las variables del tipo.
/// `vars` son las libres (`["x"]`, `["t"]`, `["x","y"]`, …).
fn validate_expr_syntax(expr: &str, vars: &[&str], what: &str) -> Result<(), EquationError> {
    if expr.len() > MAX_EQUATION_CHARS {
        return Err(EquationError::budget(expr.len()));
    }
    if expr.trim().is_empty() {
        return Err(EquationError::domain(format!("{what} quedó vacía"), None));
    }
    if let Some(byte_idx) = expr.find('\0') {
        return Err(EquationError::syntax(
            format!("{what} tiene un NUL"),
            Some(char_column(expr, byte_idx)),
        ));
    }
    let empty: HashMap<String, f64> = HashMap::new();
    match grafito_geometry::expr::prepare_function_ast(expr, &empty, vars) {
        Ok(_) => Ok(()),
        Err(reason) => {
            let col = extract_offset(&reason)
                .map(|b| char_column(expr, b))
                .or_else(|| approx_bad_char(expr));
            Err(EquationError::syntax(format!("{what}: {reason}"), col))
        }
    }
}

fn validate_complex_syntax(expr: &str, what: &str) -> Result<(), EquationError> {
    if expr.len() > MAX_EQUATION_CHARS {
        return Err(EquationError::budget(expr.len()));
    }
    if expr.trim().is_empty() {
        return Err(EquationError::domain(format!("{what} quedó vacía"), None));
    }
    match grafito_complex::complex_expr::parse(expr) {
        Ok(_) => Ok(()),
        Err(reason) => Err(EquationError::syntax(
            format!("{what}: {reason}"),
            approx_bad_char(expr),
        )),
    }
}

/// Número finito con posición aproximada dentro del borrador original.
fn parse_finite_number(part: &str, original: &str, what: &str) -> Result<f64, EquationError> {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        return Err(EquationError::domain(format!("{what} quedó vacío"), None));
    }
    let col = original.find(trimmed).map(|b| char_column(original, b));
    match trimmed.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        Ok(_) => Err(EquationError::domain(
            format!("{what} tiene que ser un número finito, llegó `{trimmed}`"),
            col,
        )),
        Err(_) => Err(EquationError::syntax(
            format!("{what} no es un número: `{trimmed}`"),
            col,
        )),
    }
}

/// Booleano `true/false` con posición.
fn parse_bool_flag(part: &str, original: &str, what: &str) -> Result<bool, EquationError> {
    let trimmed = part.trim().to_ascii_lowercase();
    let col = original.find(part.trim()).map(|b| char_column(original, b));
    match trimmed.as_str() {
        "true" | "1" | "si" | "sí" | "v" => Ok(true),
        "false" | "0" | "no" | "f" => Ok(false),
        _ => Err(EquationError::syntax(
            format!("{what} espera true/false, llegó `{}`", part.trim()),
            col,
        )),
    }
}

/// Quita un par externo `(…)` solo si cierra al final. `None` si no hay.
fn strip_outer_parens(s: &str) -> Option<String> {
    let t = s.trim();
    if !(t.starts_with('(') && t.ends_with(')')) || t.len() < 2 {
        return None;
    }
    let mut depth = 0_usize;
    let chars: Vec<(usize, char)> = t.char_indices().collect();
    if chars.is_empty() {
        return None;
    }
    for (i, (byte_idx, c)) in chars.iter().enumerate() {
        if *c == '(' {
            depth += 1;
        } else if *c == ')' {
            if depth == 0 {
                return None;
            }
            depth -= 1;
            if depth == 0 && i + 1 != chars.len() {
                return None;
            }
        }
        let _ = byte_idx;
    }
    if depth != 0 {
        return None;
    }
    t.get(1..t.len() - 1).map(|inner| inner.to_string())
}

/// Parte por comas de nivel 0 (respeta `()[]` y comillas con escape).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth_paren = 0_i32;
    let mut depth_brack = 0_i32;
    let mut in_quote = false;
    let mut escaped = false;
    let mut start = 0_usize;
    let bytes = s.as_bytes();
    let mut i = 0_usize;
    while i < s.len() {
        let Some(c) = s.get(i..).and_then(|rest| rest.chars().next()) else {
            break;
        };
        let clen = c.len_utf8();
        if in_quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quote = false;
            }
        } else if c == '"' {
            in_quote = true;
        } else if c == '(' {
            depth_paren += 1;
        } else if c == ')' {
            depth_paren = depth_paren.saturating_sub(1);
        } else if c == '[' {
            depth_brack += 1;
        } else if c == ']' {
            depth_brack = depth_brack.saturating_sub(1);
        } else if c == ',' && depth_paren == 0 && depth_brack == 0 {
            if let Some(part) = s.get(start..i) {
                out.push(part.to_string());
            }
            start = i + clen;
        }
        let _ = bytes;
        i += clen;
    }
    if let Some(part) = s.get(start..) {
        out.push(part.to_string());
    }
    out
}

/// Punto `(x, y)` (acepta con o sin paréntesis externos).
fn parse_point_text(s: &str, original: &str) -> Result<Point2, EquationError> {
    let inner = strip_outer_parens(s).unwrap_or_else(|| s.trim().to_string());
    let parts = split_top_level_commas(&inner);
    if parts.len() != 2 {
        let col = original.find(s.trim()).map(|b| char_column(original, b));
        return Err(EquationError::syntax(
            format!("se esperaba un punto (x, y), llegó `{}`", s.trim()),
            col,
        ));
    }
    let x = parse_finite_number(
        parts.first().map(String::as_str).unwrap_or(""),
        original,
        "x",
    )?;
    let y = parse_finite_number(
        parts.get(1).map(String::as_str).unwrap_or(""),
        original,
        "y",
    )?;
    Ok(Point2::new(x, y))
}

/// Des-escapa contenido de texto (`\"` → `"`, `\\` → `\`).
fn unescape_text_content(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Parse por tipo ──────────────────────────────────────────────────

fn parse_function(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Function(f) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let expr = if let Some(eq_idx) = text.find('=') {
        let left = text.get(..eq_idx).map(str::trim).unwrap_or("");
        let right = text.get(eq_idx + 1..).map(str::trim).unwrap_or("");
        if left.eq_ignore_ascii_case("y") {
            if right.is_empty() {
                return Err(EquationError::domain(
                    "después de `y =` falta la fórmula en x".to_string(),
                    Some(char_column(text, eq_idx)),
                ));
            }
            right.to_string()
        } else {
            return Err(EquationError::syntax(
                format!("se esperaba `y = …`, llegó `{left} = …`"),
                Some(char_column(text, 0)),
            ));
        }
    } else {
        text.to_string()
    };
    validate_expr_syntax(&expr, &["x"], "la fórmula en x")?;
    let mut next = f.clone();
    next.expr = expr.trim().to_string();
    next.invalidate_cache();
    let mut obj = GeoObject::Function(next);
    // Preserva id/label/estilo del original (el clon ya los trae).
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_parametric_2d(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::ParametricCurve2D(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let inner = strip_outer_parens(text).ok_or_else(|| {
        EquationError::syntax(
            format!("se esperaba `(x(t), y(t))`, llegó `{}`", text.trim()),
            Some(1),
        )
    })?;
    let parts = split_top_level_commas(&inner);
    if parts.len() != 2 {
        return Err(EquationError::syntax(
            format!(
                "la paramétrica 2D lleva 2 partes `(x(t), y(t))`, llegaron {}",
                parts.len()
            ),
            Some(1),
        ));
    }
    let ex = parts
        .first()
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let ey = parts
        .get(1)
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let param = "t";
    validate_expr_syntax(&ex, &[param], "x(t)")?;
    validate_expr_syntax(&ey, &[param], "y(t)")?;
    let mut next = c.clone();
    next.expr_x = ex;
    next.expr_y = ey;
    next.invalidate_cache();
    let mut obj = GeoObject::ParametricCurve2D(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_parametric_3d(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::ParametricCurve3D(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let inner = strip_outer_parens(text).ok_or_else(|| {
        EquationError::syntax(
            format!("se esperaba `(x(t), y(t), z(t))`, llegó `{}`", text.trim()),
            Some(1),
        )
    })?;
    let parts = split_top_level_commas(&inner);
    if parts.len() != 3 {
        return Err(EquationError::syntax(
            format!(
                "la paramétrica 3D lleva 3 partes `(x, y, z)`, llegaron {}",
                parts.len()
            ),
            Some(1),
        ));
    }
    let param = if c.parameter.trim().is_empty() {
        "t".to_string()
    } else {
        c.parameter.clone()
    };
    let ex = parts
        .first()
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let ey = parts
        .get(1)
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let ez = parts
        .get(2)
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    validate_expr_syntax(&ex, &[param.as_str()], "x(t)")?;
    validate_expr_syntax(&ey, &[param.as_str()], "y(t)")?;
    validate_expr_syntax(&ez, &[param.as_str()], "z(t)")?;
    let mut next = c.clone();
    next.expr_x = ex;
    next.expr_y = ey;
    next.expr_z = ez;
    next.invalidate_cache();
    let mut obj = GeoObject::ParametricCurve3D(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

/// Operador implícito de nivel 0. Devuelve (byte_inicio, byte_largo, op).
fn find_implicit_operator(s: &str) -> Option<(usize, usize, RelationOperator)> {
    let mut depth = 0_i32;
    let mut in_quote = false;
    let mut escaped = false;
    let bytes_len = s.len();
    let mut i = 0_usize;
    while i < bytes_len {
        let rest = s.get(i..)?;
        let c = rest.chars().next()?;
        let clen = c.len_utf8();
        if in_quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_quote = false;
            }
        } else if c == '"' {
            in_quote = true;
        } else if c == '(' || c == '[' {
            depth += 1;
        } else if c == ')' || c == ']' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 {
            if rest.starts_with("<=") {
                return Some((i, 2, RelationOperator::LessEq));
            }
            if rest.starts_with(">=") {
                return Some((i, 2, RelationOperator::GreaterEq));
            }
            if rest.starts_with("==") {
                return Some((i, 2, RelationOperator::Eq));
            }
            if rest.starts_with('≤') {
                return Some((i, c.len_utf8(), RelationOperator::LessEq));
            }
            if rest.starts_with('≥') {
                return Some((i, c.len_utf8(), RelationOperator::GreaterEq));
            }
            if c == '=' {
                return Some((i, 1, RelationOperator::Eq));
            }
            if c == '<' {
                return Some((i, 1, RelationOperator::Less));
            }
            if c == '>' {
                return Some((i, 1, RelationOperator::Greater));
            }
        }
        i += clen;
    }
    None
}

fn parse_implicit(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::ImplicitCurve(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let (lhs, rhs, op) = match find_implicit_operator(text) {
        Some((start, len, op)) => {
            let lhs = text.get(..start).map(str::trim).unwrap_or("").to_string();
            let rhs = text
                .get(start + len..)
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            if lhs.is_empty() || rhs.is_empty() {
                return Err(EquationError::syntax(
                    "la implícita necesita los dos lados, p. ej. `x^2 + y^2 = 1`".to_string(),
                    Some(char_column(text, start)),
                ));
            }
            (lhs, rhs, op)
        }
        None => {
            // Sin operador → `F(x,y) = 0` honesto.
            (
                text.trim().to_string(),
                "0".to_string(),
                RelationOperator::Eq,
            )
        }
    };
    validate_expr_syntax(&lhs, &["x", "y"], "el lado izquierdo")?;
    validate_expr_syntax(&rhs, &["x", "y"], "el lado derecho")?;
    let mut next = c.clone();
    next.expr_lhs = lhs;
    next.expr_rhs = rhs;
    next.operator = op;
    next.invalidate_cache();
    let mut obj = GeoObject::ImplicitCurve(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

// ── Recta ax+by=c ───────────────────────────────────────────────────

/// Separa `ecuación [extremos]` si hay corchetes finales.
fn split_line_bracket(text: &str) -> Result<(String, Option<String>), EquationError> {
    let trimmed = text.trim();
    let Some(brack_start) = trimmed.find('[') else {
        return Ok((trimmed.to_string(), None));
    };
    if !trimmed.ends_with(']') {
        return Err(EquationError::syntax(
            "el corchete de extremos quedó sin cerrar `]`".to_string(),
            Some(char_column(text, brack_start)),
        ));
    }
    let eq = trimmed
        .get(..brack_start)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let inner = trimmed
        .get(brack_start + 1..trimmed.len().saturating_sub(1))
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if eq.is_empty() {
        return Err(EquationError::syntax(
            "falta la ecuación `ax+by=c` antes de los extremos".to_string(),
            Some(1),
        ));
    }
    if inner.is_empty() {
        return Err(EquationError::syntax(
            "los extremos `[(x1,y1) - (x2,y2)]` quedaron vacíos".to_string(),
            Some(char_column(text, brack_start)),
        ));
    }
    Ok((eq, Some(inner)))
}

/// Términos `ax+by` → (a, b). Acepta `2x`, `2*x`, `x`, `-y`, espacios.
fn parse_line_lhs(left: &str, original: &str) -> Result<(f64, f64), EquationError> {
    let nospace: String = left.chars().filter(|c| !c.is_whitespace()).collect();
    let no_star = nospace.replace('*', "");
    if no_star.is_empty() {
        return Err(EquationError::syntax(
            "la izquierda quedó vacía, p. ej. `2x - 3y`".to_string(),
            None,
        ));
    }
    // Parte por signo conservando el signo en cada término.
    let mut terms: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in no_star.chars() {
        if (c == '+' || c == '-') && !cur.is_empty() {
            terms.push(std::mem::take(&mut cur));
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        terms.push(cur);
    }
    let mut a_opt: Option<f64> = None;
    let mut b_opt: Option<f64> = None;
    for term in &terms {
        let low = term.to_ascii_lowercase();
        let has_x = low.contains('x');
        let has_y = low.contains('y');
        if has_x && has_y {
            return Err(EquationError::syntax(
                format!("el término `{term}` mezcla x e y, separalos"),
                original
                    .find(term.as_str())
                    .map(|b| char_column(original, b)),
            ));
        }
        if !has_x && !has_y {
            return Err(EquationError::syntax(
                format!(
                    "el término `{term}` no tiene x ni y; la constante va a la derecha del `=`"
                ),
                original
                    .find(term.as_str())
                    .map(|b| char_column(original, b)),
            ));
        }
        let var = if has_x { 'x' } else { 'y' };
        // Coeficiente = término sin la letra.
        let coeff_raw = low.replace(['x', 'y'], "");
        let coeff = if coeff_raw.is_empty() || coeff_raw == "+" {
            1.0
        } else if coeff_raw == "-" {
            -1.0
        } else {
            match coeff_raw.parse::<f64>() {
                Ok(v) if v.is_finite() => v,
                _ => {
                    return Err(EquationError::syntax(
                        format!("coeficiente raro en `{term}`"),
                        original
                            .find(term.as_str())
                            .map(|b| char_column(original, b)),
                    ));
                }
            }
        };
        if var == 'x' {
            if a_opt.is_some() {
                return Err(EquationError::syntax(
                    "la x aparece dos veces, dejá un solo término en x".to_string(),
                    original
                        .find(term.as_str())
                        .map(|b| char_column(original, b)),
                ));
            }
            a_opt = Some(coeff);
        } else {
            if b_opt.is_some() {
                return Err(EquationError::syntax(
                    "la y aparece dos veces, dejá un solo término en y".to_string(),
                    original
                        .find(term.as_str())
                        .map(|b| char_column(original, b)),
                ));
            }
            b_opt = Some(coeff);
        }
    }
    Ok((a_opt.unwrap_or(0.0), b_opt.unwrap_or(0.0)))
}

/// Extrae los dos puntos de `[(x1,y1) - (x2,y2)]` o `[… -> …]`.
fn parse_line_bracket_points(
    inner: &str,
    original: &str,
) -> Result<(Point2, Point2, grafito_geometry::LineKind), EquationError> {
    let kind = if inner.contains("->") {
        grafito_geometry::LineKind::Ray
    } else {
        grafito_geometry::LineKind::Segment
    };
    // Busca los dos grupos `(…)` balanceados.
    let mut groups: Vec<String> = Vec::new();
    let mut depth = 0_i32;
    let mut start: Option<usize> = None;
    for (idx, c) in inner.char_indices() {
        if c == '(' {
            if depth == 0 {
                start = Some(idx);
            }
            depth += 1;
        } else if c == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if let Some(s) = start {
                    if let Some(g) = inner.get(s..idx + c.len_utf8()) {
                        groups.push(g.to_string());
                    }
                    start = None;
                }
            }
        }
    }
    if groups.len() != 2 {
        return Err(EquationError::syntax(
            format!(
                "los extremos necesitan dos puntos `[(x1,y1) - (x2,y2)]`, llegaron {}",
                groups.len()
            ),
            Some(1),
        ));
    }
    let p1 = parse_point_text(groups.first().map(String::as_str).unwrap_or(""), original)?;
    let p2 = parse_point_text(groups.get(1).map(String::as_str).unwrap_or(""), original)?;
    Ok((p1, p2, kind))
}

fn line_points_for_coeffs(a: f64, b: f64, c: f64) -> Result<(Point2, Point2), EquationError> {
    // Dos puntos canónicos sobre la recta infinita.
    if a.abs() >= b.abs() {
        if a.abs() <= 1e-12 {
            return Err(EquationError::domain(
                "la recta necesita al menos un coeficiente no nulo en x o y".to_string(),
                None,
            ));
        }
        let x1 = c / a;
        let x2 = (c - b) / a;
        let (x1, x2) = (x1, x2);
        if !x1.is_finite() || !x2.is_finite() {
            return Err(EquationError::domain(
                "los puntos de la recta no son finitos con esos coeficientes".to_string(),
                None,
            ));
        }
        Ok((Point2::new(x1, 0.0), Point2::new(x2, 1.0)))
    } else {
        if b.abs() <= 1e-12 {
            return Err(EquationError::domain(
                "la recta necesita al menos un coeficiente no nulo en x o y".to_string(),
                None,
            ));
        }
        let y1 = c / b;
        let y2 = (c - a) / b;
        if !y1.is_finite() || !y2.is_finite() {
            return Err(EquationError::domain(
                "los puntos de la recta no son finitos con esos coeficientes".to_string(),
                None,
            ));
        }
        Ok((Point2::new(0.0, y1), Point2::new(1.0, y2)))
    }
}

fn parse_line(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Line(l) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let (eq_part, bracket) = split_line_bracket(text)?;
    let eq_idx = eq_part.find('=').ok_or_else(|| {
        EquationError::syntax(
            format!("se esperaba `ax + by = c`, llegó `{}`", eq_part.trim()),
            Some(1),
        )
    })?;
    let left = eq_part.get(..eq_idx).map(str::trim).unwrap_or("");
    let right = eq_part.get(eq_idx + 1..).map(str::trim).unwrap_or("");
    if left.is_empty() || right.is_empty() {
        return Err(EquationError::syntax(
            "la recta necesita los dos lados, p. ej. `2x - 3y = 5`".to_string(),
            Some(char_column(text, eq_idx)),
        ));
    }
    let (a, b) = parse_line_lhs(left, text)?;
    let c = parse_finite_number(right, text, "c")?;
    if a.abs() <= 1e-12 && b.abs() <= 1e-12 {
        return Err(EquationError::domain(
            "con a=b=0 no hay recta (¿quisiste decir otra cosa?)".to_string(),
            None,
        ));
    }
    let mut next = l.clone();
    match bracket {
        Some(inner) => {
            let (p1, p2, kind) = parse_line_bracket_points(&inner, text)?;
            // Consistencia: los extremos tienen que estar sobre la recta.
            for (i, p) in [p1, p2].iter().enumerate() {
                let v = a * p.x + b * p.y - c;
                let scale = a.abs() + b.abs() + c.abs() + 1.0;
                if !v.is_finite() || v.abs() > 1e-6 * scale {
                    return Err(EquationError::domain(
                        format!(
                            "el extremo {} ({}, {}) no cae sobre `{}`, revisalo",
                            i + 1,
                            p.x,
                            p.y,
                            eq_part.trim()
                        ),
                        None,
                    ));
                }
            }
            if kind == grafito_geometry::LineKind::Ray {
                let dx = p2.x - p1.x;
                let dy = p2.y - p1.y;
                if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) <= 1e-12 {
                    return Err(EquationError::domain(
                        "la semirrecta necesita dos puntos distintos".to_string(),
                        None,
                    ));
                }
            }
            next.start = p1;
            next.end = p2;
            next.kind = kind;
        }
        None => {
            // Sin corchetes: infinita deriva puntos; segmento/semirrecta exige extremos.
            match l.kind {
                grafito_geometry::LineKind::Line => {
                    let (p1, p2) = line_points_for_coeffs(a, b, c)?;
                    next.start = p1;
                    next.end = p2;
                }
                grafito_geometry::LineKind::Segment | grafito_geometry::LineKind::Ray => {
                    return Err(EquationError::domain(
                        "para segmentos y semirrectas incluí los extremos: `ax+by=c [(x1,y1) - (x2,y2)]`".to_string(),
                        None,
                    ));
                }
            }
        }
    }
    let mut obj = GeoObject::Line(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

// ── Círculo ─────────────────────────────────────────────────────────

fn parse_circle_structured(
    original: &GeoObject,
    text: &str,
    lower: &str,
) -> Result<Option<GeoObject>, EquationError> {
    // Alias honesto `centro=(h,k) radio=r` (también `r=`).
    let has_centro = lower.contains("centro");
    let has_radio = lower.contains("radio") || lower.contains("r=");
    if !has_centro && !has_radio {
        return Ok(None);
    }
    let GeoObject::Circle(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    // Centro: primer `(…)` del texto.
    let mut center_opt: Option<Point2> = None;
    {
        let mut depth = 0_i32;
        let mut start: Option<usize> = None;
        for (idx, ch) in text.char_indices() {
            if ch == '(' {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            } else if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let (Some(s), true) = (start, center_opt.is_none()) {
                        if let Some(g) = text.get(s..idx + ch.len_utf8()) {
                            if let Ok(p) = parse_point_text(g, text) {
                                center_opt = Some(p);
                            }
                        }
                    }
                    start = None;
                }
            }
        }
    }
    let center = center_opt.ok_or_else(|| {
        EquationError::syntax(
            "no encontré el centro `(h, k)` en `centro=(h, k) radio=r`".to_string(),
            Some(1),
        )
    })?;
    // Radio: después de `radio=` o `r=`.
    let key_pos = lower.find("radio=").or_else(|| lower.find("r="));
    let Some(kpos) = key_pos else {
        return Err(EquationError::syntax(
            "falta `radio=r`, p. ej. `centro=(1, 2) radio=3`".to_string(),
            Some(1),
        ));
    };
    let key_len = if lower
        .get(kpos..)
        .map(|r| r.starts_with("radio="))
        .unwrap_or(false)
    {
        "radio=".len()
    } else {
        "r=".len()
    };
    let after = text.get(kpos + key_len..).unwrap_or("").trim();
    // El radio termina en coma, espacio o fin.
    let end = after
        .char_indices()
        .find(|(_, ch)| *ch == ',' || *ch == ' ' || *ch == ')')
        .map(|(i, _)| i)
        .unwrap_or(after.len());
    let num_str = after.get(..end).map(str::trim).unwrap_or("");
    let r = parse_finite_number(num_str, text, "radio")?;
    if r <= 0.0 {
        return Err(EquationError::domain(
            format!("el radio tiene que ser positivo, llegó {r}"),
            text.find(num_str).map(|b| char_column(text, b)),
        ));
    }
    let mut next = c.clone();
    next.center = center;
    next.radius = r;
    let mut obj = GeoObject::Circle(next);
    obj.set_label(original.label().to_string());
    Ok(Some(obj))
}

fn parse_circle_equation(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Circle(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    // Normaliza `²` y `**2` a `^2` sin romper unicode (solo reemplazo seguro).
    let norm = text.replace('²', "^2").replace("**2", "^2");
    let eq_idx = norm.find('=').ok_or_else(|| {
        EquationError::syntax(
            format!(
                "se esperaba `(x-h)^2 + (y-k)^2 = R`, llegó `{}`",
                text.trim()
            ),
            Some(1),
        )
    })?;
    let left = norm.get(..eq_idx).map(str::trim).unwrap_or("").to_string();
    let right = norm
        .get(eq_idx + 1..)
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if left.is_empty() || right.is_empty() {
        return Err(EquationError::syntax(
            "al círculo le faltan lados, p. ej. `(x - 1)^2 + (y - 2)^2 = 9`".to_string(),
            Some(char_column(text, eq_idx)),
        ));
    }
    let r2 = parse_finite_number(&right, text, "R")?;
    if r2 <= 0.0 {
        return Err(EquationError::domain(
            format!("R tiene que ser positivo (r²), llegó {r2}"),
            text.find(right.trim()).map(|b| char_column(text, b)),
        ));
    }
    let r = r2.sqrt();
    if !r.is_finite() || r <= 0.0 {
        return Err(EquationError::domain(
            "el radio no es finito con ese R".to_string(),
            None,
        ));
    }
    // Izquierda: `(x ± h)^2 + (y ± k)^2` (tolera espacios y `*`).
    let compact: String = left.chars().filter(|ch| !ch.is_whitespace()).collect();
    let compact = compact.replace('*', "");
    // Busca `x`, operador, número, `^2`, `+`, `y`, operador, número, `^2`.
    let (h, k) = parse_circle_lhs(&compact, text)?;
    let mut next = c.clone();
    next.center = Point2::new(h, k);
    next.radius = r;
    let mut obj = GeoObject::Circle(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

/// `(x-h)^2+(y-k)^2` compacto → (h, k).
fn parse_circle_lhs(compact: &str, original: &str) -> Result<(f64, f64), EquationError> {
    // Encuentra `x` y su número.
    let x_pos = compact.find('x').ok_or_else(|| {
        EquationError::syntax(
            "en el círculo falta la parte en x `(x - h)^2`".to_string(),
            Some(1),
        )
    })?;
    let y_pos = compact.find('y').ok_or_else(|| {
        EquationError::syntax(
            "en el círculo falta la parte en y `(y - k)^2`".to_string(),
            Some(1),
        )
    })?;
    if y_pos < x_pos {
        return Err(EquationError::syntax(
            "el círculo va `(x …)^2 + (y …)^2`, la x va primero".to_string(),
            Some(1),
        ));
    }
    let h = parse_circle_axis(compact, x_pos, 'x', original)?;
    let k = parse_circle_axis(compact, y_pos, 'y', original)?;
    // Exige `^2` después de cada paréntesis (honesto, no inventa).
    let after_x = compact.get(x_pos..).unwrap_or("");
    let after_y = compact.get(y_pos..).unwrap_or("");
    if !after_x.contains("^2") || !after_y.contains("^2") {
        return Err(EquationError::syntax(
            "al círculo le falta `^2` en algún término".to_string(),
            Some(1),
        ));
    }
    Ok((h, k))
}

/// Número tras `x`/`y` con su signo (`(x-1)` → 1, `(x+1)` → -1).
fn parse_circle_axis(
    compact: &str,
    var_pos: usize,
    _var: char,
    original: &str,
) -> Result<f64, EquationError> {
    let after = compact.get(var_pos + 1..).unwrap_or("");
    let mut chars = after.char_indices();
    let Some((op_off, op)) = chars.next() else {
        return Err(EquationError::syntax(
            "después de x/y va `- h` o `+ h`".to_string(),
            Some(1),
        ));
    };
    if op != '-' && op != '+' {
        return Err(EquationError::syntax(
            format!("después de x/y va `-` o `+`, llegó `{op}`"),
            Some(char_column(original, 0)),
        ));
    }
    let num_start = var_pos + 1 + op_off + op.len_utf8();
    let rest = compact.get(num_start..).unwrap_or("");
    // Número hasta `)` o `^`.
    let mut end = rest.len();
    for (i, ch) in rest.char_indices() {
        if ch == ')' || ch == '^' {
            end = i;
            break;
        }
        if !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E') {
            end = i;
            break;
        }
    }
    let num_str = rest.get(..end).map(str::trim).unwrap_or("");
    if num_str.is_empty() {
        return Err(EquationError::syntax(
            "falta el número del centro, p. ej. `(x - 1)`".to_string(),
            Some(1),
        ));
    }
    let mag = parse_finite_number(num_str, original, "centro")?;
    Ok(if op == '-' { mag } else { -mag })
}

fn parse_circle(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let lower = text.to_ascii_lowercase();
    if let Some(obj) = parse_circle_structured(original, text, &lower)? {
        return Ok(obj);
    }
    parse_circle_equation(original, text)
}

// ── Cónicas estructuradas ───────────────────────────────────────────

/// Busca `clave=` (ascii, insensible a mayúsculas) y devuelve el valor crudo
/// hasta `,` o fin (respeta paréntesis).
fn find_named_value<'a>(text: &'a str, lower: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let pos = lower.find(needle.as_str())?;
    let after = text.get(pos + needle.len()..)?.trim();
    // Si el valor arranca con `(`, toma hasta el `)` que cierra.
    if after.starts_with('(') {
        let mut depth = 0_i32;
        for (idx, c) in after.char_indices() {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    return after.get(..idx + c.len_utf8());
                }
            }
        }
        return None;
    }
    // Si no, hasta la próxima coma o espacio de nivel 0, o fin.
    let mut depth = 0_i32;
    for (idx, c) in after.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && (c == ',' || c.is_whitespace()) {
            let piece = after.get(..idx).map(str::trim).unwrap_or("");
            if !piece.is_empty() {
                return after.get(..idx);
            }
            // Espacios líderes ya recortados; un espacio acá cierra el valor.
            return after.get(..idx);
        }
    }
    Some(after.trim())
}

fn parse_ellipse(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Ellipse(e) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let lower = text.to_ascii_lowercase();
    if !lower.trim_start().starts_with("elipse") {
        return Err(EquationError::syntax(
            format!(
                "se esperaba `elipse centro=(h, k) rx=… ry=… rot=…`, llegó `{}`",
                text.trim()
            ),
            Some(1),
        ));
    }
    let centro_raw = find_named_value(text, &lower, "centro")
        .ok_or_else(|| EquationError::syntax("falta `centro=(h, k)`".to_string(), Some(1)))?;
    let center = parse_point_text(centro_raw, text)?;
    let rx_raw = find_named_value(text, &lower, "rx")
        .or_else(|| find_named_value(text, &lower, "rx "))
        .ok_or_else(|| EquationError::syntax("falta `rx=…`".to_string(), Some(1)))?;
    let ry_raw = find_named_value(text, &lower, "ry")
        .ok_or_else(|| EquationError::syntax("falta `ry=…`".to_string(), Some(1)))?;
    let rot_raw = find_named_value(text, &lower, "rot").unwrap_or("0");
    let rx = parse_finite_number(rx_raw, text, "rx")?;
    let ry = parse_finite_number(ry_raw, text, "ry")?;
    let rot = parse_finite_number(rot_raw, text, "rot")?;
    if rx <= 0.0 || ry <= 0.0 {
        return Err(EquationError::domain(
            format!("rx y ry tienen que ser positivos (rx={rx}, ry={ry})"),
            None,
        ));
    }
    if !rot.is_finite() {
        return Err(EquationError::domain(
            "rot tiene que ser finito".to_string(),
            None,
        ));
    }
    let mut next = e.clone();
    next.center = center;
    next.rx = rx;
    next.ry = ry;
    next.angle = rot;
    let mut obj = GeoObject::Ellipse(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_parabola(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Parabola(p) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let lower = text.to_ascii_lowercase();
    if !lower.trim_start().starts_with("parabola") && !lower.trim_start().starts_with("parábola") {
        return Err(EquationError::syntax(
            format!(
                "se esperaba `parabola vertice=(h, k) p=… vertical=… rot=…`, llegó `{}`",
                text.trim()
            ),
            Some(1),
        ));
    }
    let vert_raw = find_named_value(text, &lower, "vertice")
        .or_else(|| {
            // `vértice` con tilde ocupa 2 bytes en `é`; el lower ascii no lo
            // trae, así que se busca directo en el texto original.
            let tl = text.to_ascii_lowercase();
            let _ = tl;
            None
        })
        .ok_or_else(|| EquationError::syntax("falta `vertice=(h, k)`".to_string(), Some(1)))?;
    let vertex = parse_point_text(vert_raw, text)?;
    let p_raw = find_named_value(text, &lower, "p").ok_or_else(|| {
        EquationError::syntax("falta `p=…` (distancia focal)".to_string(), Some(1))
    })?;
    // `vertical=` contiene una `p`? No: se busca `vertical=` antes que `p=` para
    // no confundir. Como ya se extrajo con `find_named_value`, el orden no
    // importa: `p=` matchea `vertical=`? `vertical=` contiene `l=`? No `p=`.
    // Pero `p=` sí matchea dentro de `...`? `find("p=")` encuentra la `p` de
    // `vertice=`? No, `vertice=` es `e=` al final. Seguro.
    let vert_raw_flag = find_named_value(text, &lower, "vertical").unwrap_or("true");
    let rot_raw = find_named_value(text, &lower, "rot").unwrap_or("0");
    let focal = parse_finite_number(p_raw, text, "p")?;
    if focal == 0.0 || !focal.is_finite() {
        return Err(EquationError::domain(
            format!("p tiene que ser no nulo y finito, llegó {focal}"),
            None,
        ));
    }
    let vertical = parse_bool_flag(vert_raw_flag, text, "vertical")?;
    let rot = parse_finite_number(rot_raw, text, "rot")?;
    let mut next = p.clone();
    next.vertex = vertex;
    next.p = focal;
    next.vertical = vertical;
    next.angle = rot;
    let mut obj = GeoObject::Parabola(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_hyperbola(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Hyperbola(h) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let lower = text.to_ascii_lowercase();
    if !lower.trim_start().starts_with("hiperbola") && !lower.trim_start().starts_with("hipérbola")
    {
        return Err(EquationError::syntax(
            format!(
                "se esperaba `hiperbola centro=(h, k) a=… b=… horizontal=… rot=…`, llegó `{}`",
                text.trim()
            ),
            Some(1),
        ));
    }
    let centro_raw = find_named_value(text, &lower, "centro")
        .ok_or_else(|| EquationError::syntax("falta `centro=(h, k)`".to_string(), Some(1)))?;
    let center = parse_point_text(centro_raw, text)?;
    // `a=` aparece dentro de `horizontal=`? No (`horizontal=` termina en `l=`).
    // Pero `a=` sí aparece en `parabola`? Acá el texto arranca con hiperbola,
    // así que el primer `a=` es el semieje. Para no confundir con otras
    // claves, se exige que el valor de `a` no contenga `(`.
    let a_raw = find_named_value(text, &lower, "a")
        .ok_or_else(|| EquationError::syntax("falta `a=…` (semieje)".to_string(), Some(1)))?;
    let b_raw = find_named_value(text, &lower, "b")
        .ok_or_else(|| EquationError::syntax("falta `b=…` (semieje)".to_string(), Some(1)))?;
    // Si `a=` capturó `centro=(…)` por error (contiene paréntesis), es que
    // matcheó la `a` de otra palabra: se rechaza honesto.
    if a_raw.contains('(') || b_raw.contains('(') {
        return Err(EquationError::syntax(
            "revisá `a=… b=…` (semiejes sin paréntesis)".to_string(),
            Some(1),
        ));
    }
    let horiz_raw = find_named_value(text, &lower, "horizontal").unwrap_or("true");
    let rot_raw = find_named_value(text, &lower, "rot").unwrap_or("0");
    let a = parse_finite_number(a_raw, text, "a")?;
    let b = parse_finite_number(b_raw, text, "b")?;
    if a <= 0.0 || b <= 0.0 {
        return Err(EquationError::domain(
            format!("a y b tienen que ser positivos (a={a}, b={b})"),
            None,
        ));
    }
    let horizontal = parse_bool_flag(horiz_raw, text, "horizontal")?;
    let rot = parse_finite_number(rot_raw, text, "rot")?;
    let mut next = h.clone();
    next.center = center;
    next.a = a;
    next.b = b;
    next.horizontal = horizontal;
    next.angle = rot;
    let mut obj = GeoObject::Hyperbola(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

// ── Punto / polígono / texto ────────────────────────────────────────

fn parse_point_obj(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Point(p) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let pos = parse_point_text(text, text)?;
    let mut next = p.clone();
    next.position = pos;
    let mut obj = GeoObject::Point(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_polygon(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Polygon(poly) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let trimmed = text.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Err(EquationError::syntax(
            format!(
                "el polígono va `[(x1,y1), (x2,y2), …]`, llegó `{}`",
                trimmed
            ),
            Some(1),
        ));
    }
    let inner = trimmed
        .get(1..trimmed.len().saturating_sub(1))
        .unwrap_or("");
    // Extrae grupos `(…)` balanceados.
    let mut groups: Vec<String> = Vec::new();
    let mut depth = 0_i32;
    let mut start: Option<usize> = None;
    for (idx, c) in inner.char_indices() {
        if c == '(' {
            if depth == 0 {
                start = Some(idx);
            }
            depth += 1;
        } else if c == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if let Some(s) = start {
                    if let Some(g) = inner.get(s..idx + c.len_utf8()) {
                        groups.push(g.to_string());
                    }
                    start = None;
                }
            }
        }
    }
    if groups.len() < 3 {
        return Err(EquationError::domain(
            format!(
                "el polígono necesita al menos 3 vértices, llegaron {}",
                groups.len()
            ),
            None,
        ));
    }
    if groups.len() > grafito_core::validation::MAX_POLYGON_VERTICES {
        return Err(EquationError::budget(groups.len()));
    }
    let mut vertices: Vec<Point2> = Vec::with_capacity(groups.len());
    for g in &groups {
        vertices.push(parse_point_text(g, text)?);
    }
    let mut next = poly.clone();
    next.vertices = vertices;
    next.x_exprs = Vec::new();
    next.y_exprs = Vec::new();
    let mut obj = GeoObject::Polygon(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_text_obj(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Text(t) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    // `"contenido" @ (x, y)` — el `@posición` es opcional (conserva la actual).
    let first_quote = text.find('"').ok_or_else(|| {
        EquationError::syntax(
            "el texto va entre comillas, p. ej. `\"hola\" @ (1, 2)`".to_string(),
            Some(1),
        )
    })?;
    // Cierre no escapado.
    let bytes = text.as_bytes();
    let mut close: Option<usize> = None;
    let mut backslashes = 0_usize;
    let mut idx = first_quote + 1;
    while idx < text.len() {
        let Some(rest) = text.get(idx..) else {
            break;
        };
        let Some(c) = rest.chars().next() else {
            break;
        };
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' && backslashes.is_multiple_of(2) {
                close = Some(idx);
                break;
            }
            backslashes = 0;
        }
        idx += c.len_utf8();
        let _ = bytes;
    }
    let Some(close_idx) = close else {
        return Err(EquationError::syntax(
            "al texto le falta la comilla de cierre".to_string(),
            Some(char_column(text, first_quote)),
        ));
    };
    let raw_content = text.get(first_quote + 1..close_idx).unwrap_or("");
    let content = unescape_text_content(raw_content);
    if content.len() > grafito_core::validation::MAX_STRING_LENGTH {
        return Err(EquationError::budget(content.len()));
    }
    if content.contains('\0') || content.contains('\u{FEFF}') {
        return Err(EquationError::domain(
            "el texto no admite NUL ni BOM".to_string(),
            Some(char_column(text, first_quote)),
        ));
    }
    let after = text.get(close_idx + 1..).map(str::trim).unwrap_or("");
    let position = if after.is_empty() {
        t.position
    } else {
        let at_stripped = after.strip_prefix('@').map(str::trim).unwrap_or(after);
        parse_point_text(at_stripped, text)?
    };
    let mut next = t.clone();
    next.content = content;
    next.position = position;
    let mut obj = GeoObject::Text(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

// ── Polar / superficie ──────────────────────────────────────────────

fn split_name_equals(text: &str, name: &str) -> Option<String> {
    let eq_idx = text.find('=')?;
    let left = text.get(..eq_idx).map(str::trim).unwrap_or("");
    if !left.eq_ignore_ascii_case(name) {
        return None;
    }
    text.get(eq_idx + 1..).map(|r| r.trim().to_string())
}

fn parse_polar(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::PolarCurve(c) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    let expr = if text.contains('=') {
        split_name_equals(text, "r").ok_or_else(|| {
            EquationError::syntax(
                format!("se esperaba `r = …`, llegó `{}`", text.trim()),
                Some(1),
            )
        })?
    } else {
        text.trim().to_string()
    };
    validate_expr_syntax(&expr, &["t"], "r(t)")?;
    let mut next = c.clone();
    next.expr_r = expr.trim().to_string();
    next.invalidate_cache();
    let mut obj = GeoObject::PolarCurve(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

fn parse_surface(original: &GeoObject, text: &str) -> Result<GeoObject, EquationError> {
    let GeoObject::Surface3D(s) = original else {
        return Err(EquationError::not_editable("desconocido"));
    };
    if s.is_parametric {
        let inner = strip_outer_parens(text).ok_or_else(|| {
            EquationError::syntax(
                format!(
                    "se esperaba `(x(u,v), y(u,v), z(u,v))`, llegó `{}`",
                    text.trim()
                ),
                Some(1),
            )
        })?;
        let parts = split_top_level_commas(&inner);
        if parts.len() != 3 {
            return Err(EquationError::syntax(
                format!(
                    "la superficie paramétrica lleva 3 partes, llegaron {}",
                    parts.len()
                ),
                Some(1),
            ));
        }
        let ex = parts
            .first()
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let ey = parts
            .get(1)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let ez = parts
            .get(2)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        validate_expr_syntax(&ex, &["u", "v"], "x(u,v)")?;
        validate_expr_syntax(&ey, &["u", "v"], "y(u,v)")?;
        validate_expr_syntax(&ez, &["u", "v"], "z(u,v)")?;
        let mut next = s.clone();
        next.expr_x = ex;
        next.expr_y = ey;
        next.expr_z = ez;
        let mut obj = GeoObject::Surface3D(next);
        obj.set_label(original.label().to_string());
        return Ok(obj);
    }
    if s.is_complex {
        let t = text.trim();
        let inner = if t.starts_with('|') && t.ends_with('|') && t.len() >= 2 {
            t.get(1..t.len() - 1)
                .map(str::trim)
                .unwrap_or("")
                .to_string()
        } else {
            t.to_string()
        };
        validate_complex_syntax(&inner, "f(z)")?;
        let mut next = s.clone();
        next.expr = inner.trim().to_string();
        let mut obj = GeoObject::Surface3D(next);
        obj.set_label(original.label().to_string());
        return Ok(obj);
    }
    let expr = if text.contains('=') {
        split_name_equals(text, "z").ok_or_else(|| {
            EquationError::syntax(
                format!("se esperaba `z = …`, llegó `{}`", text.trim()),
                Some(1),
            )
        })?
    } else {
        text.trim().to_string()
    };
    validate_expr_syntax(&expr, &["x", "y"], "z(x,y)")?;
    let mut next = s.clone();
    next.expr = expr.trim().to_string();
    let mut obj = GeoObject::Surface3D(next);
    obj.set_label(original.label().to_string());
    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{
        CircleObj, EllipseObj, FunctionObj, HyperbolaObj, ImplicitCurveObj, LineObj, ParabolaObj,
        ParametricCurve2DObj, ParametricCurve3DObj, PointObj, PolarCurveObj, PolygonObj,
        Surface3DObj, TextObj,
    };

    fn doc_with(obj: GeoObject) -> (Document, ObjectId) {
        let mut doc = Document::new();
        let id = match doc.try_add_object(obj) {
            Ok(id) => id,
            Err(_) => ObjectId::new(),
        };
        (doc, id)
    }

    #[test]
    fn roundtrip_function() {
        let obj = GeoObject::Function(FunctionObj::new("x^2 + 1"));
        let text = obj.canonical_equation_text().expect("canónica");
        assert_eq!(text, "y = x^2 + 1");
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        assert_eq!(
            back.canonical_equation_text().as_deref(),
            Some(text.as_str())
        );
        if let GeoObject::Function(f) = back {
            assert_eq!(f.expr, "x^2 + 1");
        } else {
            panic!("tipo cambiado");
        }
    }

    #[test]
    fn roundtrip_parametric_2d() {
        let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
            "cos(t)",
            "sin(t)",
            0.0,
            std::f64::consts::TAU,
        ));
        let text = obj.canonical_equation_text().expect("canónica");
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        assert_eq!(
            back.canonical_equation_text().as_deref(),
            Some(text.as_str())
        );
    }

    #[test]
    fn roundtrip_parametric_3d() {
        let obj =
            GeoObject::ParametricCurve3D(ParametricCurve3DObj::new("t", "t^2", "t^3", 0.0, 1.0));
        let text = obj.canonical_equation_text().expect("canónica");
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        assert_eq!(
            back.canonical_equation_text().as_deref(),
            Some(text.as_str())
        );
    }

    #[test]
    fn roundtrip_implicit_bare_becomes_zero() {
        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x^2 + y^2 - 1",
            "0",
            RelationOperator::Eq,
        ));
        let text = obj.canonical_equation_text().expect("canónica");
        assert!(text.contains('='));
        // Sin operador → F = 0 honesto.
        let bare = parse_inspector_equation(&obj, "x^2 + y^2 - 1").expect("bare");
        if let GeoObject::ImplicitCurve(ic) = bare {
            assert_eq!(ic.expr_rhs, "0");
        } else {
            panic!("tipo cambiado");
        }
    }

    #[test]
    fn roundtrip_line_infinite() {
        use grafito_geometry::LineKind;
        let mut l = LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0));
        l.kind = LineKind::Line;
        let obj = GeoObject::Line(l);
        let text = obj.canonical_equation_text().expect("canónica");
        assert!(text.contains('='));
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        // Misma recta (proporcional), misma id.
        assert_eq!(back.id(), obj.id());
        if let (GeoObject::Line(a), GeoObject::Line(b)) = (&obj, &back) {
            let (aa, ab, ac) = (a.end.y - a.start.y, a.start.x - a.end.x, 0.0);
            let _ = (aa, ab, ac);
            // Verifica que los puntos nuevos caen sobre la recta original.
            for p in [b.start, b.end] {
                let v = (p.x - p.y).abs();
                assert!(v < 1e-9, "punto {p:?} fuera de y=x");
            }
        } else {
            panic!("tipo cambiado");
        }
    }

    #[test]
    fn roundtrip_line_segment_with_bracket() {
        let obj = GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)));
        let text = obj.canonical_equation_text().expect("canónica");
        assert!(text.contains('['));
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        assert_eq!(back.id(), obj.id());
        assert_eq!(
            back.canonical_equation_text().as_deref(),
            Some(text.as_str())
        );
    }

    #[test]
    fn roundtrip_circle() {
        let obj = GeoObject::Circle(CircleObj::new(Point2::new(1.0, 2.0), 3.0));
        let text = obj.canonical_equation_text().expect("canónica");
        let back = parse_inspector_equation(&obj, &text).expect("parse");
        if let GeoObject::Circle(c) = back {
            assert!((c.center.x - 1.0).abs() < 1e-12);
            assert!((c.center.y - 2.0).abs() < 1e-12);
            assert!((c.radius - 3.0).abs() < 1e-9);
        } else {
            panic!("tipo cambiado");
        }
    }

    #[test]
    fn roundtrip_conics() {
        let e = GeoObject::Ellipse(EllipseObj::new(Point2::new(1.0, 2.0), 3.0, 2.0));
        let t = e.canonical_equation_text().expect("elipse");
        let b = parse_inspector_equation(&e, &t).expect("parse elipse");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));

        let p = GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, 1.0), 2.0));
        let t = p.canonical_equation_text().expect("parabola");
        let b = parse_inspector_equation(&p, &t).expect("parse parabola");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));

        let h = GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 3.0, 2.0));
        let t = h.canonical_equation_text().expect("hiperbola");
        let b = parse_inspector_equation(&h, &t).expect("parse hiperbola");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));
    }

    #[test]
    fn roundtrip_point_polygon_text() {
        let pt = GeoObject::Point(PointObj::new(Point2::new(1.5, -2.0)));
        let t = pt.canonical_equation_text().expect("punto");
        let b = parse_inspector_equation(&pt, &t).expect("parse punto");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));

        let poly = GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ]));
        let t = poly.canonical_equation_text().expect("polígono");
        let b = parse_inspector_equation(&poly, &t).expect("parse polígono");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));

        let tx = GeoObject::Text(TextObj::new("hola", Point2::new(1.0, 2.0)));
        let t = tx.canonical_equation_text().expect("texto");
        let b = parse_inspector_equation(&tx, &t).expect("parse texto");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));
    }

    #[test]
    fn roundtrip_polar_surface() {
        let pol =
            GeoObject::PolarCurve(PolarCurveObj::new("1 + cos(t)", 0.0, std::f64::consts::TAU));
        let t = pol.canonical_equation_text().expect("polar");
        let b = parse_inspector_equation(&pol, &t).expect("parse polar");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));

        let s = GeoObject::Surface3D(Surface3DObj::new("x^2+y^2", (-2.0, 2.0), (-2.0, 2.0)));
        let t = s.canonical_equation_text().expect("sup");
        let b = parse_inspector_equation(&s, &t).expect("parse sup");
        assert_eq!(b.canonical_equation_text().as_deref(), Some(t.as_str()));
    }

    #[test]
    fn error_sintaxis_con_posicion() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let err = parse_inspector_equation(&obj, "y = x + *").expect_err("sintaxis");
        assert!(err.message.contains("columna") || err.message.contains("entender"));
        // Jamás aplica a medias: el objeto sigue igual.
        if let GeoObject::Function(f) = &obj {
            assert_eq!(f.expr, "x");
        }
    }

    #[test]
    fn error_presupuesto_honesto() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let long = "y = ".to_string() + &"x".repeat(MAX_EQUATION_CHARS);
        let err = parse_inspector_equation(&obj, &long).expect_err("presupuesto");
        assert!(err.message.contains("presupuesto"));
    }

    #[test]
    fn error_tipo_no_editable() {
        use grafito_core::{Cube3DObj, GeoObject};
        use grafito_geometry::Point3D;
        let obj = GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0));
        assert_eq!(obj.canonical_equation_text(), None);
        let err = parse_inspector_equation(&obj, "cualquier cosa").expect_err("no editable");
        assert!(err.message.contains("no se puede editar"));
    }

    #[test]
    fn undo_intacto_tras_update() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let (mut doc, id) = doc_with(obj);
        let before = doc.clone();
        let changed = apply_inspector_equation(&mut doc, id, "y = x^2").expect("aplica");
        assert!(changed);
        // Misma id, nueva ecuación.
        let after = doc.get_object(id).expect("existe");
        assert_eq!(after.id(), id);
        assert_eq!(after.canonical_equation_text().as_deref(), Some("y = x^2"));
        // Undo: restaura el previo sin perder la id.
        let prev = apply_inspector_equation_with_previous(&mut doc, id, "y = x^3")
            .expect("segundo update");
        assert!(prev.is_some());
        let _ = before;
        // El documento previo devuelto permite deshacer manual honesto.
        let mut doc2 = doc.clone();
        if let Some(snapshot) = prev {
            doc2 = snapshot;
        }
        assert_eq!(
            doc2.get_object(id)
                .and_then(|o| o.canonical_equation_text())
                .as_deref(),
            Some("y = x^2")
        );
    }

    #[test]
    fn apply_fallido_no_toca_documento() {
        let obj = GeoObject::Function(FunctionObj::new("x"));
        let (mut doc, id) = doc_with(obj);
        let snapshot = doc.clone();
        let err = apply_inspector_equation(&mut doc, id, "y = x + *").expect_err("falla");
        assert!(!err.message.is_empty());
        // Intacto: mismo estado semántico.
        let a = serde_json::to_value(&doc).expect("json");
        let b = serde_json::to_value(&snapshot).expect("json");
        assert_eq!(a, b);
        let _ = id;
    }
}
