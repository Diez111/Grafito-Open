//! Frente G-D: action objects + subset GGBScript + custom tools `.ggt`.
//!
//! Todo vive en `grafito-command` a propósito: los objetos de acción se
//! modelan sobre `GeoObject::Text` existente (payload estructurado + estado en
//! `Document.variables`), sin nuevas variantes de `GeoObject` —eso exigiría
//! brazos en `validation.rs` y cableado en render/UI, fuera del alcance de
//! este frente—. El click visual lo cableará la piel (P2); aquí quedan modelo,
//! comandos, semántica de estado y tests de ida y vuelta.
//!
//! ## Objetos de acción (respaldados por `Text`)
//!
//! Contenido con dos/tres líneas:
//!
//! ```text
//! <glifo> <rótulo>
//! %%grafito:<kind> <clave>=<valor> ...%%
//! [<guion solo en button>]
//! ```
//!
//! - `Button`: `%%grafito:button%%` + tercera línea con el guion `;`-separado.
//! - `Checkbox`: `%%grafito:checkbox var=<n>%%`, estado en `variables[n]` (1/0).
//! - `InputBox`/`TextField`: `%%grafito:input var=<n>%%` o
//!   `%%grafito:textfield var=<n>%%`, valor en `variables[n]`.
//!
//! ## Subset GGBScript honesto
//!
//! Permitido en cuerpos de guion/herramienta ([`GGBSCRIPT_ALLOWLIST`]):
//! `SetValue`, `Show`, `Hide`, `ZoomIn`, `ZoomOut`, `PlayPause`, `If`,
//! `Repeat`, `Button`, `Checkbox`, `InputBox`, `TextField`, `DefineTool`,
//! `LoadTool`. El resto (incluido `Script` genérico, que salta la allowlist)
//! se rechaza con error honesto que nombra la alternativa cuando existe.
//!
//! ## Custom tools `.ggt` (JSON versionado, sin código arbitrario)
//!
//! `DefineTool[nombre, pasos]` valida y devuelve el JSON; `LoadTool[json]`
//! re-valida (versión, nombre, cotas, allowlist) y describe sin ejecutar.
//! La persistencia en archivo `.ggt` la hace la piel/export (P2); el núcleo es
//! puro sobre strings.

use crate::cas_parse::parse_cas_command;
use crate::command_registry;
use crate::commands::{
    execute_snippet_sequence, find_object_by_label, parse_numeric_arg, unique_object_label,
    CommandOutcome, ScriptBudget,
};
use grafito_core::validation::MAX_EXPR_LENGTH;
use grafito_core::{Document, GeoObject};
use grafito_geometry::expr::evaluate;
use grafito_geometry::Point2;

// ── Presupuestos G-D ────────────────────────────────────────────────

/// Pasos máximos de un guion GGB (If/Repeat/button/herramienta). Cota propia,
/// además del presupuesto de 100 comandos del `Script` genérico.
pub const MAX_GGBSCRIPT_STEPS: usize = 1000;
/// Iteraciones máximas de `Repeat[n, guion]`.
pub const MAX_GGB_REPEAT: usize = 1000;
/// Bytes máximos del JSON de una custom tool `.ggt`.
pub const MAX_GGT_BYTES: usize = 65_536;
/// Pasos máximos dentro de una custom tool.
pub const MAX_GGT_STEPS: usize = 100;
/// Longitud máxima del nombre de una custom tool (ASCII identificadora).
pub const MAX_GGT_NAME_LEN: usize = 64;
/// Versión del esquema JSON de custom tools.
pub const GGT_SCHEMA_VERSION: u32 = 1;
/// Caracteres máximos del rótulo visible de un action object.
pub const MAX_ACTION_CAPTION_CHARS: usize = 200;
/// Etiquetas máximas aceptadas por `Show`/`Hide` en una invocación.
pub const MAX_VISIBILITY_LABELS: usize = 4;
/// Factor de zoom por defecto (GeoGebra usa ×2/÷2 en botones; 1.25 es paso fino).
pub const DEFAULT_ZOOM_FACTOR: f64 = 1.25;
/// Factor máximo aceptado por invocación (evita saltos absurdos).
pub const MAX_ZOOM_FACTOR: f64 = 4.0;

// ── Allowlist G-D ───────────────────────────────────────────────────

/// Comandos canónicos permitidos dentro de guiones (`If`/`Repeat`/botones) y
/// pasos de custom tools. Todo lo demás se rechaza con error honesto: sin
/// ejecución de código arbitrario.
pub const GGBSCRIPT_ALLOWLIST: &[&str] = &[
    "SetValue",
    "Show",
    "Hide",
    "ZoomIn",
    "ZoomOut",
    "PlayPause",
    "If",
    "Repeat",
    "Button",
    "Checkbox",
    "InputBox",
    "TextField",
    "DefineTool",
    "LoadTool",
];

// Nota: el presupuesto de pasos G-D vive en
// `ScriptBudget.ggb_steps` (ver `run_ggb_steps`): todo anidado comparte la
// misma cota (≤1000) sin vía de escape por profundidad.

// ── Payload de action objects ───────────────────────────────────────

const PAYLOAD_MARKER: &str = "%%grafito:";
const BUTTON_GLYPH: &str = "▣";
const CHECKBOX_ON_GLYPH: &str = "☑";
const CHECKBOX_OFF_GLYPH: &str = "☐";
const INPUT_GLYPH: &str = "▤";

/// Kind de action object respaldado por `GeoObject::Text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Button,
    Checkbox,
    Input,
    TextField,
}

impl ActionKind {
    fn tag(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Checkbox => "checkbox",
            Self::Input => "input",
            Self::TextField => "textfield",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "button" => Some(Self::Button),
            "checkbox" => Some(Self::Checkbox),
            "input" => Some(Self::Input),
            "textfield" => Some(Self::TextField),
            _ => None,
        }
    }

    fn glyph(self, checked: bool) -> &'static str {
        match self {
            Self::Button => BUTTON_GLYPH,
            Self::Checkbox => {
                if checked {
                    CHECKBOX_ON_GLYPH
                } else {
                    CHECKBOX_OFF_GLYPH
                }
            }
            Self::Input | Self::TextField => INPUT_GLYPH,
        }
    }
}

/// Vista parseada de un action object (rótulo + binding + guion).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionObjectView {
    /// Kind del control.
    pub kind: ActionKind,
    /// Rótulo visible (primera línea sin glifo).
    pub caption: String,
    /// Variable ligada (`checkbox`/`input`/`textfield`).
    pub variable: Option<String>,
    /// Guion almacenado (solo `button`).
    pub script: Option<String>,
}

/// Quita comillas dobles/simples externas y recorta espacios.
pub fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

/// Valida un rótulo visible: no vacío, ≤200 caracteres, una línea, sin marcador.
fn check_caption(raw: &str) -> Result<String, String> {
    let caption = unquote(raw);
    let trimmed = caption.trim();
    if trimmed.is_empty() {
        return Err("el rótulo no debe estar vacío".into());
    }
    if trimmed.chars().count() > MAX_ACTION_CAPTION_CHARS {
        return Err(format!(
            "el rótulo excede {MAX_ACTION_CAPTION_CHARS} caracteres"
        ));
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("el rótulo debe ser una sola línea".into());
    }
    if trimmed.contains(PAYLOAD_MARKER) {
        return Err("el rótulo no debe contener el marcador interno".into());
    }
    Ok(trimmed.to_string())
}

/// Valida un nombre de variable ligable (`[A-Za-z_][A-Za-z0-9_]*`).
fn check_variable_name(raw: &str) -> Result<String, String> {
    let name = unquote(raw).trim().to_string();
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("el nombre de variable no debe estar vacío".into());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!("nombre de variable inválido: '{name}'"));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(format!("nombre de variable inválido: '{name}'"));
    }
    if name.len() > MAX_GGT_NAME_LEN {
        return Err(format!(
            "el nombre de variable excede {MAX_GGT_NAME_LEN} caracteres"
        ));
    }
    Ok(name)
}

/// Parsea la vista de un `GeoObject::Text` si porta payload de acción.
pub fn action_view_of(obj: &GeoObject) -> Option<ActionObjectView> {
    let GeoObject::Text(text) = obj else {
        return None;
    };
    let mut lines = text.content.lines();
    let first = lines.next()?;
    let second = lines.next()?;
    let marker = second.strip_prefix(PAYLOAD_MARKER)?.strip_suffix("%%")?;
    let mut parts = marker.split_whitespace();
    let kind = ActionKind::from_tag(parts.next()?)?;
    let caption = first
        .strip_prefix(&format!("{} ", kind.glyph(true)))
        .or_else(|| first.strip_prefix(&format!("{} ", kind.glyph(false))))?
        .to_string();
    let mut variable: Option<String> = None;
    for part in parts {
        if let Some(var) = part.strip_prefix("var=") {
            variable = Some(var.to_string());
        } else {
            return None;
        }
    }
    let script = if kind == ActionKind::Button {
        let rest: Vec<&str> = lines.collect();
        if rest.is_empty() {
            return None;
        }
        Some(rest.join("\n"))
    } else {
        if variable.is_none() || lines.next().is_some() {
            return None;
        }
        None
    };
    Some(ActionObjectView {
        kind,
        caption,
        variable,
        script,
    })
}

fn action_content(
    kind: ActionKind,
    caption: &str,
    checked: bool,
    variable: Option<&str>,
) -> String {
    let mut out = format!("{} {caption}", kind.glyph(checked));
    out.push('\n');
    out.push_str(PAYLOAD_MARKER);
    out.push_str(kind.tag());
    if let Some(var) = variable {
        out.push_str(" var=");
        out.push_str(var);
    }
    out.push_str("%%");
    out
}

/// Crea el `Text` de un botón (guion ya validado contra la allowlist).
fn make_button_text(caption: &str, script: &str) -> String {
    let mut out = action_content(ActionKind::Button, caption, false, None);
    out.push('\n');
    out.push_str(script.trim());
    out
}

/// Inserta un `Text` de acción con etiqueta única derivada del rótulo.
fn insert_action_text(
    document: &mut Document,
    mut text: grafito_core::TextObj,
    caption: &str,
) -> Result<String, String> {
    let label = unique_object_label(document, caption);
    text.label = label.clone();
    document.try_add_object(GeoObject::Text(text))?;
    Ok(label)
}

// ── Guiones: validación + ejecución acotada ──────────────────────────

/// Divide un guion en pasos y valida cada uno contra la allowlist.
/// Devuelve los pasos recortados (sin vacíos).
pub fn check_script_allowlist(script: &str) -> Result<Vec<String>, String> {
    if script.len() > MAX_EXPR_LENGTH {
        return Err(format!("el guion excede {MAX_EXPR_LENGTH} caracteres"));
    }
    let steps = crate::commands::split_script_commands(script)?;
    if steps.is_empty() {
        return Err("el guion no contiene pasos".into());
    }
    if steps.len() > MAX_GGT_STEPS {
        return Err(format!(
            "el guion excede {MAX_GGT_STEPS} pasos (tiene {})",
            steps.len()
        ));
    }
    for step in &steps {
        let parsed = parse_cas_command(step).ok_or_else(|| {
            format!("paso no es un comando válido del subset GGBScript: '{step}'")
        })?;
        let canonical =
            command_registry::canonicalize(&parsed.command).unwrap_or(parsed.command.as_str());
        // Compara por canónico insensible a mayúsculas contra la allowlist.
        let allowed = GGBSCRIPT_ALLOWLIST
            .iter()
            .any(|name| name.eq_ignore_ascii_case(canonical));
        if !allowed {
            return Err(format!(
                "paso '{step}' usa '{canonical}', fuera del subset GGBScript (permitidos: {})",
                GGBSCRIPT_ALLOWLIST.join(", ")
            ));
        }
        for arg in &parsed.args {
            if arg.len() > MAX_EXPR_LENGTH {
                return Err(format!(
                    "argumento de '{canonical}' excede {MAX_EXPR_LENGTH} caracteres"
                ));
            }
        }
    }
    Ok(steps)
}

/// Ejecuta pasos ya validados con doble presupuesto compartido: profundidad
/// (`ScriptBudget.depth`, cota del `Script` genérico) y pasos propios G-D
/// (`ScriptBudget.ggb_steps`, ≤1000). El contador es compartido, así que el
/// anidado (If/Repeat/button, venga de `Script` o de entrada directa) no abre
/// vía de escape por profundidad.
///
/// `pub(crate)` porque expone [`ScriptBudget`](crate::commands::ScriptBudget);
/// la piel (P2) lo usará para el click con un presupuesto público dedicado.
pub(crate) fn run_ggb_steps(
    document: &mut Document,
    steps: &[String],
    script_budget: &mut ScriptBudget,
) -> Result<usize, String> {
    if script_budget.depth >= crate::commands::MAX_SCRIPT_DEPTH {
        return Err(format!(
            "el guion excede la profundidad máxima {}",
            crate::commands::MAX_SCRIPT_DEPTH
        ));
    }
    if script_budget.ggb_steps.saturating_add(steps.len()) > MAX_GGBSCRIPT_STEPS {
        return Err(format!("el guion excede {MAX_GGBSCRIPT_STEPS} pasos"));
    }
    script_budget.depth = script_budget.depth.saturating_add(1);
    let mut executed = 0usize;
    for step in steps {
        script_budget.ggb_steps = script_budget.ggb_steps.saturating_add(1);
        let mut nested = step.clone();
        match execute_snippet_sequence(document, &mut nested, script_budget) {
            Ok(()) => executed = executed.saturating_add(1),
            Err(message) => {
                script_budget.depth = script_budget.depth.saturating_sub(1);
                return Err(format!("{step}: {message}"));
            }
        }
    }
    script_budget.depth = script_budget.depth.saturating_sub(1);
    Ok(executed)
}

/// Evalúa una condición numérica (`expr` o `a <cmp> b`) con las variables del
/// documento. Verdadero = comparación cierta o valor finito no nulo.
pub fn eval_condition(document: &Document, cond: &str) -> Result<bool, String> {
    let cond = cond.trim();
    if cond.is_empty() {
        return Err("la condición no debe estar vacía".into());
    }
    if cond.len() > MAX_EXPR_LENGTH {
        return Err(format!("la condición excede {MAX_EXPR_LENGTH} caracteres"));
    }
    let vars: Vec<(String, f64)> = document
        .variables
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    // Operadores de dos caracteres primero, fuera de paréntesis.
    let bytes = cond.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut op_at: Option<(usize, &str)> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        // Nunca trocear un `char` multibyte: solo inspeccionar fronteras.
        if !cond.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let ch = bytes[i] as char;
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            match ch {
                '(' | '[' => depth = depth.saturating_add(1),
                ')' | ']' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth == 0 {
                let rest = &cond[i..];
                let found = if rest.starts_with("<=") {
                    Some("<=")
                } else if rest.starts_with(">=") {
                    Some(">=")
                } else if rest.starts_with("==") {
                    Some("==")
                } else if rest.starts_with("!=") {
                    Some("!=")
                } else if rest.starts_with('<') {
                    Some("<")
                } else if rest.starts_with('>') {
                    Some(">")
                } else if rest.starts_with('=') {
                    Some("=")
                } else {
                    None
                };
                if let Some(op) = found {
                    op_at = Some((i, op));
                    break;
                }
            }
        }
        i += 1;
    }
    if let Some((at, op)) = op_at {
        let lhs = cond[..at].trim();
        let rhs = cond[at + op.len()..].trim();
        if lhs.is_empty() || rhs.is_empty() {
            return Err(format!("condición mal formada: '{cond}'"));
        }
        let left =
            evaluate(lhs, &vars).map_err(|error| format!("lado izquierdo inválido: {error}"))?;
        let right =
            evaluate(rhs, &vars).map_err(|error| format!("lado derecho inválido: {error}"))?;
        if !left.is_finite() || !right.is_finite() {
            return Err("la condición debe evaluar a valores finitos".into());
        }
        return Ok(match op {
            "<" => left < right,
            ">" => left > right,
            "<=" => left <= right,
            ">=" => left >= right,
            "=" | "==" => left == right,
            "!=" => left != right,
            _ => false,
        });
    }
    let value = evaluate(cond, &vars).map_err(|error| format!("condición inválida: {error}"))?;
    if !value.is_finite() {
        return Err("la condición debe evaluar a un valor finito".into());
    }
    Ok(value != 0.0)
}

// ── Handlers de comandos ────────────────────────────────────────────

fn outcome_message(cleared: &mut String, message: String) -> CommandOutcome {
    cleared.clear();
    CommandOutcome::Message(message)
}

fn run_button(document: &mut Document, args: &[String], input_text: &mut String) -> CommandOutcome {
    if args.len() != 2 {
        return CommandOutcome::Error("Button: usa Button[rotulo, guion]".into());
    }
    let caption = match check_caption(&args[0]) {
        Ok(caption) => caption,
        Err(error) => return CommandOutcome::Error(format!("Button: {error}")),
    };
    let script_raw = unquote(&args[1]);
    let steps = match check_script_allowlist(&script_raw) {
        Ok(steps) => steps,
        Err(error) => return CommandOutcome::Error(format!("Button: {error}")),
    };
    let script = steps.join("; ");
    let text =
        grafito_core::TextObj::new(make_button_text(&caption, &script), Point2::new(0.0, 0.0));
    match insert_action_text(document, text, &caption) {
        Ok(label) => outcome_message(
            input_text,
            format!("Button: '{label}' creado con {} paso(s)", steps.len()),
        ),
        Err(error) => CommandOutcome::Error(format!("Button: {error}")),
    }
}

fn run_checkbox(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
) -> CommandOutcome {
    if args.len() != 2 && args.len() != 3 {
        return CommandOutcome::Error(
            "Checkbox: usa Checkbox[rotulo, variable] o Checkbox[rotulo, variable, inicial]".into(),
        );
    }
    let caption = match check_caption(&args[0]) {
        Ok(caption) => caption,
        Err(error) => return CommandOutcome::Error(format!("Checkbox: {error}")),
    };
    let var = match check_variable_name(&args[1]) {
        Ok(var) => var,
        Err(error) => return CommandOutcome::Error(format!("Checkbox: {error}")),
    };
    let initial = if args.len() == 3 {
        match args[2].trim().to_lowercase().as_str() {
            "true" | "verdadero" | "1" => true,
            "false" | "falso" | "0" => false,
            _ => {
                return CommandOutcome::Error(
                    "Checkbox: inicial debe ser true/false (verdadero/falso, 1/0)".into(),
                )
            }
        }
    } else {
        false
    };
    if let Err(error) = document.try_set_variable(var.clone(), if initial { 1.0 } else { 0.0 }) {
        return CommandOutcome::Error(format!("Checkbox: {error}"));
    }
    let content = action_content(ActionKind::Checkbox, &caption, initial, Some(&var));
    let text = grafito_core::TextObj::new(content, Point2::new(0.0, 0.0));
    match insert_action_text(document, text, &caption) {
        Ok(label) => outcome_message(
            input_text,
            format!(
                "Checkbox: '{label}' ligado a '{var}' ({})",
                if initial { "activado" } else { "desactivado" }
            ),
        ),
        Err(error) => CommandOutcome::Error(format!("Checkbox: {error}")),
    }
}

fn run_input_box(
    command: &str,
    kind: ActionKind,
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
) -> CommandOutcome {
    if args.len() != 2 {
        return CommandOutcome::Error(format!("{command}: usa {command}[rotulo, variable]"));
    }
    let caption = match check_caption(&args[0]) {
        Ok(caption) => caption,
        Err(error) => return CommandOutcome::Error(format!("{command}: {error}")),
    };
    let var = match check_variable_name(&args[1]) {
        Ok(var) => var,
        Err(error) => return CommandOutcome::Error(format!("{command}: {error}")),
    };
    if !document.variables.contains_key(&var) {
        if let Err(error) = document.try_set_variable(var.clone(), 0.0) {
            return CommandOutcome::Error(format!("{command}: {error}"));
        }
    }
    let value = document.variables.get(&var).copied().unwrap_or(0.0);
    let content = format!(
        "{} {caption}: {value}\n{marker}{tag} var={var}%%",
        kind.glyph(false),
        marker = PAYLOAD_MARKER,
        tag = kind.tag(),
    );
    let text = grafito_core::TextObj::new(content, Point2::new(0.0, 0.0));
    match insert_action_text(document, text, &caption) {
        Ok(label) => outcome_message(
            input_text,
            format!("{command}: '{label}' ligado a '{var}' (valor {value})"),
        ),
        Err(error) => CommandOutcome::Error(format!("{command}: {error}")),
    }
}

/// Simula el click en un botón: ejecuta su guion almacenado (la piel llamará
/// a este helper desde el evento de puntero; P2, con presupuesto público).
#[allow(dead_code)]
pub(crate) fn press_button(
    document: &mut Document,
    label: &str,
    script_budget: &mut ScriptBudget,
) -> Result<usize, String> {
    let id = find_object_by_label(document, label.trim().trim_matches('"').trim_matches('\''))
        .ok_or_else(|| format!("no existe el objeto '{label}'"))?;
    let view = document
        .get_object(id)
        .cloned()
        .ok_or_else(|| format!("objeto '{label}' inválido"))?;
    let action =
        action_view_of(&view).ok_or_else(|| format!("'{label}' no es un action object"))?;
    if action.kind != ActionKind::Button {
        return Err(format!("'{label}' no es un botón"));
    }
    let script = action.script.unwrap_or_default();
    let steps = check_script_allowlist(&script)?;
    run_ggb_steps(document, &steps, script_budget)
}

/// Alterna un checkbox: invierte su variable ligada y refresca el glifo.
pub fn toggle_checkbox(document: &mut Document, label: &str) -> Result<bool, String> {
    let clean = label.trim().trim_matches('"').trim_matches('\'');
    let id = find_object_by_label(document, clean)
        .ok_or_else(|| format!("no existe el objeto '{label}'"))?;
    let view = document
        .get_object(id)
        .cloned()
        .ok_or_else(|| format!("objeto '{label}' inválido"))?;
    let action =
        action_view_of(&view).ok_or_else(|| format!("'{label}' no es un action object"))?;
    if action.kind != ActionKind::Checkbox {
        return Err(format!("'{label}' no es un checkbox"));
    }
    let var = action
        .variable
        .ok_or_else(|| format!("checkbox '{label}' sin variable ligada"))?;
    let current = document.variables.get(&var).copied().unwrap_or(0.0);
    let next = current == 0.0;
    document.try_set_variable(var.clone(), if next { 1.0 } else { 0.0 })?;
    let content = action_content(ActionKind::Checkbox, &action.caption, next, Some(&var));
    if let Some(GeoObject::Text(text)) = document.get_object_mut(id) {
        text.content = content;
    }
    Ok(next)
}

fn run_visibility(
    command: &str,
    visible: bool,
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
) -> CommandOutcome {
    if args.is_empty() || args.len() > MAX_VISIBILITY_LABELS {
        return CommandOutcome::Error(format!(
            "{command}: usa {command}[objeto] (1 a {MAX_VISIBILITY_LABELS} etiquetas)"
        ));
    }
    let mut missing = Vec::new();
    let mut ids = Vec::new();
    for arg in args {
        let label = unquote(arg);
        let label = label.trim();
        if label.is_empty() {
            return CommandOutcome::Error(format!("{command}: hay una etiqueta vacía"));
        }
        match find_object_by_label(document, label) {
            Some(id) => ids.push((label.to_string(), id)),
            None => missing.push(label.to_string()),
        }
    }
    if !missing.is_empty() {
        return CommandOutcome::Error(format!(
            "{command}: no existe(n) el/los objeto(s) '{}'",
            missing.join("', '")
        ));
    }
    for (_, id) in &ids {
        if let Some(obj) = document.get_object_mut(*id) {
            obj.set_visible(visible);
        }
    }
    let names: Vec<&str> = ids.iter().map(|(label, _)| label.as_str()).collect();
    outcome_message(
        input_text,
        format!(
            "{}: {} objeto(s) {}: {}",
            command,
            ids.len(),
            if visible { "visible(s)" } else { "oculto(s)" },
            names.join(", ")
        ),
    )
}

fn run_zoom(
    command: &str,
    zoom_in: bool,
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
) -> CommandOutcome {
    if args.len() > 1 {
        return CommandOutcome::Error(format!("{command}: usa {command}[] o {command}[factor]"));
    }
    let factor = if args.is_empty() {
        DEFAULT_ZOOM_FACTOR
    } else {
        match parse_numeric_arg(&args[0], &document.variables) {
            Ok(value) if value.is_finite() && value > 1.0 && value <= MAX_ZOOM_FACTOR => value,
            _ => {
                return CommandOutcome::Error(format!(
                    "{command}: el factor debe ser finito entre 1 (excluido) y {MAX_ZOOM_FACTOR}"
                ))
            }
        }
    };
    #[allow(clippy::cast_possible_truncation)]
    let applied = if zoom_in { factor } else { 1.0 / factor };
    let before = document.view().scale;
    let center = {
        let view = document.view();
        glam::Vec2::new(view.screen_size.x * 0.5, view.screen_size.y * 0.5)
    };
    #[allow(clippy::cast_possible_truncation)]
    let factor_f32 = applied as f32;
    document.view_mut().zoom(factor_f32, center);
    let after = document.view().scale;
    if after == before {
        return outcome_message(
            input_text,
            format!(
                "{command}: sin cambios (escala {before:.6}, posible eje logarítmico o límite)"
            ),
        );
    }
    outcome_message(
        input_text,
        format!("{command}: escala {before:.6} → {after:.6}"),
    )
}

fn run_play_pause(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
) -> CommandOutcome {
    if args.len() > 1 {
        return CommandOutcome::Error("PlayPause: usa PlayPause[] o PlayPause[variable]".into());
    }
    if args.is_empty() {
        // Solo variables con animación configurada (meta existente).
        let names: Vec<String> = document
            .variables()
            .keys()
            .filter(|name| document.variable_meta(name).is_some())
            .cloned()
            .collect();
        if names.is_empty() {
            return outcome_message(
                input_text,
                "PlayPause: no hay variables con animación configurada (crea un Slider primero)"
                    .into(),
            );
        }
        let any_running = names.iter().any(|name| {
            document
                .variable_meta(name)
                .is_some_and(|meta| meta.animating)
        });
        let mut changed = 0usize;
        for name in &names {
            if let Some(meta) = document.variable_meta(name).cloned() {
                let mut next = meta;
                next.animating = !any_running;
                match document.try_replace_variable_meta_with_previous(name, next) {
                    Ok(_) => changed = changed.saturating_add(1),
                    Err(error) => return CommandOutcome::Error(format!("PlayPause: {error}")),
                }
            }
        }
        return outcome_message(
            input_text,
            format!(
                "PlayPause: {} variable(s) {}",
                changed,
                if any_running { "pausadas" } else { "en marcha" }
            ),
        );
    }
    let var = match check_variable_name(&args[0]) {
        Ok(var) => var,
        Err(error) => return CommandOutcome::Error(format!("PlayPause: {error}")),
    };
    if !document.variables.contains_key(&var) {
        return CommandOutcome::Error(format!("PlayPause: no existe la variable '{var}'"));
    }
    if let Some(meta) = document.variable_meta(&var).cloned() {
        let mut next = meta;
        next.animating = !next.animating;
        let state = if next.animating {
            "en marcha"
        } else {
            "pausada"
        };
        match document.try_replace_variable_meta_with_previous(&var, next) {
            Ok(_) => return outcome_message(input_text, format!("PlayPause: '{var}' {state}")),
            Err(error) => return CommandOutcome::Error(format!("PlayPause: {error}")),
        }
    }
    let current = document.variables.get(&var).copied().unwrap_or(0.0);
    if !current.is_finite() {
        return CommandOutcome::Error(format!("PlayPause: la variable '{var}' no es finita"));
    }
    let min = current - 1.0;
    let max = current + 1.0;
    if !min.is_finite() || !max.is_finite() || min >= max {
        return CommandOutcome::Error(format!(
            "PlayPause: no se pudo crear un rango por defecto para '{var}'"
        ));
    }
    match document.configure_variable_animation(
        &var,
        min,
        max,
        1.0,
        grafito_core::AnimationMode::PingPong,
    ) {
        Ok(()) => outcome_message(
            input_text,
            format!("PlayPause: '{var}' en marcha (rango [{min}, {max}])"),
        ),
        Err(error) => CommandOutcome::Error(format!("PlayPause: {error}")),
    }
}

fn run_if(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
    script_budget: &mut ScriptBudget,
) -> CommandOutcome {
    if args.len() != 2 && args.len() != 3 {
        return CommandOutcome::Error(
            "If: usa If[condicion, guion_si] o If[condicion, guion_si, guion_no]".into(),
        );
    }
    let cond = match eval_condition(document, &args[0]) {
        Ok(cond) => cond,
        Err(error) => return CommandOutcome::Error(format!("If: {error}")),
    };
    let branch_raw = if cond {
        &args[1]
    } else if args.len() == 3 {
        &args[2]
    } else {
        return outcome_message(input_text, "If: condición falsa; nada que hacer".into());
    };
    // Las ramas suelen venir entrecomilladas (el `;` interno no debe partir
    // args): se desenvuelve un nivel de comillas antes de validar.
    let branch = unquote(branch_raw);
    let steps = match check_script_allowlist(&branch) {
        Ok(steps) => steps,
        Err(error) => return CommandOutcome::Error(format!("If: {error}")),
    };
    match run_ggb_steps(document, &steps, script_budget) {
        Ok(count) => outcome_message(
            input_text,
            format!(
                "If: condición {}; {} paso(s) ejecutado(s)",
                if cond { "verdadera" } else { "falsa" },
                count
            ),
        ),
        Err(error) => CommandOutcome::Error(format!("If: {error}")),
    }
}

fn run_repeat(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
    script_budget: &mut ScriptBudget,
) -> CommandOutcome {
    if args.len() != 2 {
        return CommandOutcome::Error("Repeat: usa Repeat[n, guion]".into());
    }
    let count: usize = match args[0].trim().parse() {
        Ok(count) => count,
        Err(_) => {
            return CommandOutcome::Error("Repeat: n debe ser un entero entre 1 y 1000".into())
        }
    };
    if count == 0 || count > MAX_GGB_REPEAT {
        return CommandOutcome::Error(format!(
            "Repeat: n debe ser un entero entre 1 y {MAX_GGB_REPEAT}"
        ));
    }
    let body = unquote(&args[1]);
    let steps = match check_script_allowlist(&body) {
        Ok(steps) => steps,
        Err(error) => return CommandOutcome::Error(format!("Repeat: {error}")),
    };
    let total = steps.len().saturating_mul(count);
    if script_budget.ggb_steps.saturating_add(total) > MAX_GGBSCRIPT_STEPS {
        return CommandOutcome::Error(format!(
            "Repeat: {count}×{} pasos excede {MAX_GGBSCRIPT_STEPS} pasos",
            steps.len()
        ));
    }
    for _ in 0..count {
        if let Err(error) = run_ggb_steps(document, &steps, script_budget) {
            return CommandOutcome::Error(format!("Repeat: {error}"));
        }
    }
    outcome_message(
        input_text,
        format!("Repeat: {count} iteración(es), {total} paso(s) ejecutado(s)"),
    )
}

// ── Custom tools `.ggt` ─────────────────────────────────────────────

/// Definición validada de una custom tool (esquema JSON versionado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomToolDef {
    /// Nombre identificadora ASCII.
    pub name: String,
    /// Pasos del macro-comando (ya validados contra la allowlist).
    pub steps: Vec<String>,
}

fn check_tool_name(raw: &str) -> Result<String, String> {
    let name = unquote(raw).trim().to_string();
    if name.is_empty() || name.len() > MAX_GGT_NAME_LEN {
        return Err(format!(
            "el nombre debe tener 1 a {MAX_GGT_NAME_LEN} caracteres"
        ));
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("el nombre no debe estar vacío".into());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!("nombre de herramienta inválido: '{name}'"));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(format!("nombre de herramienta inválido: '{name}'"));
    }
    Ok(name)
}

fn json_escape(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("no se pudo codificar JSON: {error}"))
}

/// Valida una secuencia de pasos y la empaqueta como JSON `.ggt` versionado.
pub fn define_tool_json(name: &str, script: &str) -> Result<String, String> {
    let checked_name = check_tool_name(name).map_err(|error| format!("DefineTool: {error}"))?;
    let steps = check_script_allowlist(script).map_err(|error| format!("DefineTool: {error}"))?;
    let mut out = format!(
        "{{\"grafito_tool\":{GGT_SCHEMA_VERSION},\"name\":{},\"steps\":[",
        json_escape(&checked_name).map_err(|error| format!("DefineTool: {error}"))?
    );
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_escape(step).map_err(|error| format!("DefineTool: {error}"))?);
    }
    out.push_str("]}");
    if out.len() > MAX_GGT_BYTES {
        return Err(format!("DefineTool: el JSON excede {MAX_GGT_BYTES} bytes"));
    }
    Ok(out)
}

/// Valida un JSON `.ggt` (versión, nombre, cotas, allowlist) sin ejecutar nada.
pub fn parse_tool_json(json: &str) -> Result<CustomToolDef, String> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Err("LoadTool: el JSON no debe estar vacío".into());
    }
    if trimmed.len() > MAX_GGT_BYTES {
        return Err(format!("LoadTool: el JSON excede {MAX_GGT_BYTES} bytes"));
    }
    let value: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|error| format!("LoadTool: JSON inválido: {error}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "LoadTool: se esperaba un objeto JSON".to_string())?;
    let version = obj
        .get("grafito_tool")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "LoadTool: falta 'grafito_tool' (versión de esquema)".to_string())?;
    if version != u64::from(GGT_SCHEMA_VERSION) {
        return Err(format!(
            "LoadTool: versión {version} no soportada (se esperaba {GGT_SCHEMA_VERSION})"
        ));
    }
    let name = obj
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "LoadTool: falta 'name'".to_string())?;
    let checked_name = check_tool_name(name).map_err(|error| format!("LoadTool: {error}"))?;
    let steps_value = obj
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "LoadTool: falta 'steps' (lista de pasos)".to_string())?;
    if steps_value.is_empty() || steps_value.len() > MAX_GGT_STEPS {
        return Err(format!(
            "LoadTool: 'steps' debe tener 1 a {MAX_GGT_STEPS} pasos"
        ));
    }
    let mut steps = Vec::with_capacity(steps_value.len());
    for step_value in steps_value {
        let step = step_value
            .as_str()
            .ok_or_else(|| "LoadTool: cada paso debe ser texto".to_string())?;
        if step.len() > MAX_EXPR_LENGTH {
            return Err(format!(
                "LoadTool: un paso excede {MAX_EXPR_LENGTH} caracteres"
            ));
        }
        steps.push(step.to_string());
    }
    // Re-valida cada paso contra la allowlist: el JSON es dato, nunca código.
    let joined = steps.join("; ");
    check_script_allowlist(&joined).map_err(|error| format!("LoadTool: {error}"))?;
    Ok(CustomToolDef {
        name: checked_name,
        steps,
    })
}

fn run_define_tool(args: &[String], input_text: &mut String) -> CommandOutcome {
    if args.len() != 2 {
        return CommandOutcome::Error("DefineTool: usa DefineTool[nombre, pasos]".into());
    }
    match define_tool_json(&args[0], &unquote(&args[1])) {
        Ok(json) => outcome_message(input_text, json),
        Err(error) => CommandOutcome::Error(error),
    }
}

fn run_load_tool(args: &[String], input_text: &mut String) -> CommandOutcome {
    if args.len() != 1 {
        return CommandOutcome::Error("LoadTool: usa LoadTool[json]".into());
    }
    match parse_tool_json(&args[0]) {
        Ok(tool) => outcome_message(
            input_text,
            format!(
                "LoadTool: '{}' válida con {} paso(s): {}",
                tool.name,
                tool.steps.len(),
                tool.steps.join(" | ")
            ),
        ),
        Err(error) => CommandOutcome::Error(error),
    }
}

/// Error honesto para comandos GGBScript conocidos pero no soportados.
fn unsupported(command: &str, alternative: &str) -> CommandOutcome {
    CommandOutcome::Error(format!(
        "{command} no está soportado en Grafito ({alternative})"
    ))
}

// ── Dispatcher G-D ──────────────────────────────────────────────────

/// Despacha los comandos del frente G-D. Devuelve `None` si no es un comando
/// G-D (el dispatcher general sigue su curso).
pub(crate) fn handle_ggb_command(
    document: &mut Document,
    command: &str,
    args: &[String],
    input_text: &mut String,
    script_budget: &mut ScriptBudget,
) -> Option<CommandOutcome> {
    let outcome = match command {
        "Button" => run_button(document, args, input_text),
        "Checkbox" => run_checkbox(document, args, input_text),
        "InputBox" => run_input_box("InputBox", ActionKind::Input, document, args, input_text),
        "TextField" => run_input_box(
            "TextField",
            ActionKind::TextField,
            document,
            args,
            input_text,
        ),
        "Show" => run_visibility("Show", true, document, args, input_text),
        "Hide" => run_visibility("Hide", false, document, args, input_text),
        "ZoomIn" => run_zoom("ZoomIn", true, document, args, input_text),
        "ZoomOut" => run_zoom("ZoomOut", false, document, args, input_text),
        "PlayPause" => run_play_pause(document, args, input_text),
        "If" => run_if(document, args, input_text, script_budget),
        "Repeat" => run_repeat(document, args, input_text, script_budget),
        "DefineTool" => run_define_tool(args, input_text),
        "LoadTool" => run_load_tool(args, input_text),
        "Execute" => unsupported(
            "Execute",
            "usa If/Repeat con pasos del subset o pulsa un Button",
        ),
        "StartAnimation" => unsupported("StartAnimation", "usa PlayPause[variable] o PlayPause[]"),
        "StopAnimation" => unsupported("StopAnimation", "usa PlayPause[variable] o PlayPause[]"),
        "Delete" => unsupported("Delete", "usa Erase[etiqueta] o EraseAll[]"),
        "Rename" => unsupported(
            "Rename",
            "Grafito aún no renombra objetos por comando; edita la etiqueta en la UI",
        ),
        _ => return None,
    };
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::process_input;

    fn doc_with_point(label: &str) -> Document {
        let mut doc = Document::new();
        let mut input = format!("{label} = (1, 2)");
        let outcome = process_input(&mut doc, &mut input);
        assert!(
            matches!(outcome, CommandOutcome::Ok),
            "punto base: {outcome:?}"
        );
        doc
    }

    fn last_text_label(doc: &Document) -> String {
        doc.objects_iter()
            .filter(|(_, obj)| matches!(obj, GeoObject::Text(_)))
            .map(|(_, obj)| obj.label().to_string())
            .last()
            .expect("debe existir un Text")
    }

    #[test]
    fn button_round_trip_and_press() {
        let mut doc = doc_with_point("A");
        let mut input = "Button[MiBoton, \"SetValue[a, 3]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        let label = last_text_label(&doc);
        let view = action_view_of(
            doc.get_object(find_object_by_label(&doc, &label).expect("botón"))
                .expect("obj"),
        )
        .expect("payload");
        assert_eq!(view.kind, ActionKind::Button);
        assert_eq!(view.caption, "MiBoton");
        assert_eq!(view.script.as_deref(), Some("SetValue[a, 3]"));

        let mut budget = ScriptBudget::default();
        let executed = press_button(&mut doc, &label, &mut budget).expect("press");
        assert_eq!(executed, 1);
        assert_eq!(doc.variables.get("a"), Some(&3.0));
    }

    #[test]
    fn button_rejects_non_allowlisted_script() {
        let mut doc = Document::new();
        let mut input = "Button[B, \"EraseAll[]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(
            matches!(outcome, CommandOutcome::Error(_)),
            "EraseAll fuera del subset: {outcome:?}"
        );
        assert!(doc.objects_iter().next().is_none());
    }

    #[test]
    fn checkbox_toggle_flips_variable() {
        let mut doc = Document::new();
        let mut input = "Checkbox[Sonido, snd]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        assert_eq!(doc.variables.get("snd"), Some(&0.0));
        let label = last_text_label(&doc);

        assert_eq!(toggle_checkbox(&mut doc, &label), Ok(true));
        assert_eq!(doc.variables.get("snd"), Some(&1.0));
        assert_eq!(toggle_checkbox(&mut doc, &label), Ok(false));
        assert_eq!(doc.variables.get("snd"), Some(&0.0));
    }

    #[test]
    fn input_and_textfield_bind_variable() {
        let mut doc = Document::new();
        let mut input = "InputBox[Edad, edad]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        let mut input = "TextField[Nombre, n]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        assert_eq!(doc.variables.get("edad"), Some(&0.0));
        // TextField conserva su kind en el payload (no colapsa a input).
        let kinds: Vec<ActionKind> = doc
            .objects_iter()
            .filter_map(|(_, obj)| action_view_of(obj).map(|view| view.kind))
            .collect();
        assert!(kinds.contains(&ActionKind::Input));
        assert!(kinds.contains(&ActionKind::TextField));
    }

    #[test]
    fn show_hide_round_trip() {
        let mut doc = doc_with_point("A");
        let mut input = "Hide[A]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        let id = find_object_by_label(&doc, "A").expect("A");
        assert!(!doc.get_object(id).expect("obj").is_visible());

        let mut input = "Show[A]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        assert!(doc.get_object(id).expect("obj").is_visible());

        let mut input = "Hide[NoExiste]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Error(_)
        ));
    }

    #[test]
    fn zoom_in_out_changes_scale() {
        let mut doc = Document::new();
        let before = doc.view().scale;
        let mut input = "ZoomIn[]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        assert!(doc.view().scale > before);

        let mut input = "ZoomOut[]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        let after = doc.view().scale;
        // Ida y vuelta en f32: tolerancia acorde a la precisión del pipeline.
        assert!(
            (after - before).abs() / before < 1e-5,
            "{after} vs {before}"
        );

        let mut input = "ZoomIn[0.5]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Error(_)
        ));
    }

    #[test]
    fn play_pause_toggles_animation() {
        let mut doc = Document::new();
        let mut input = "Slider[a, 0, 10, 1]".to_string();
        let slider = process_input(&mut doc, &mut input);
        assert!(
            matches!(slider, CommandOutcome::Ok | CommandOutcome::Message(_)),
            "{slider:?}"
        );
        let initial = doc
            .variable_meta("a")
            .map(|meta| meta.animating)
            .unwrap_or(false);
        let mut input = "PlayPause[]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        let after_all = doc
            .variable_meta("a")
            .map(|meta| meta.animating)
            .unwrap_or(false);
        // PlayPause[] invierte el estado global: si algo corría, pausa todo.
        assert_eq!(!after_all, initial, "PlayPause[] debe invertir");

        let mut input = "PlayPause[a]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        let after_single = doc
            .variable_meta("a")
            .map(|meta| meta.animating)
            .unwrap_or(false);
        assert_eq!(after_single, !after_all, "PlayPause[a] debe invertir");

        let mut input = "PlayPause[fantasma]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Error(_)
        ));
    }

    #[test]
    fn if_branches_execute_allowlisted_steps() {
        let mut doc = Document::new();
        doc.try_set_variable("a".into(), 5.0).expect("var");
        let mut input = "If[a > 3, \"SetValue[b, 1]\", \"SetValue[b, 2]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        assert_eq!(doc.variables.get("b"), Some(&1.0));

        let mut input = "If[a < 3, \"SetValue[c, 1]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        assert_eq!(doc.variables.get("c"), None);
    }

    #[test]
    fn repeat_is_bounded_and_allowlisted() {
        let mut doc = Document::new();
        doc.try_set_variable("a".into(), 0.0).expect("var");
        let mut input = "Repeat[5, \"SetValue[a, a + 1]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        assert!(matches!(outcome, CommandOutcome::Message(_)), "{outcome:?}");
        assert_eq!(doc.variables.get("a"), Some(&5.0));

        let mut input = format!("Repeat[{}, \"SetValue[a, a + 1]\"]", MAX_GGB_REPEAT + 1);
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Error(_)
        ));

        let mut input = "Repeat[2, \"EraseAll[]\"]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Error(_)
        ));
    }

    #[test]
    fn tool_json_round_trip_versioned() {
        let json = define_tool_json("MiTool", "SetValue[a, 1]; Show[A]").expect("define");
        assert!(json.contains("\"grafito_tool\":1"));
        let tool = parse_tool_json(&json).expect("parse");
        assert_eq!(tool.name, "MiTool");
        assert_eq!(tool.steps.len(), 2);

        // Versión futura se rechaza.
        let future = json.replacen("\"grafito_tool\":1", "\"grafito_tool\":2", 1);
        assert!(parse_tool_json(&future).is_err());
        // Paso fuera de allowlist se rechaza al cargar.
        let evil = "{\"grafito_tool\":1,\"name\":\"Evil\",\"steps\":[\"EraseAll[]\"]}";
        assert!(parse_tool_json(evil).is_err());
        // Nombre inválido se rechaza al definir.
        assert!(define_tool_json("9mal", "Show[A]").is_err());
    }

    #[test]
    fn define_load_tool_commands_round_trip() {
        let mut doc = doc_with_point("A");
        let mut input = "DefineTool[Saludar, \"Show[A]; Hide[A]\"]".to_string();
        let outcome = process_input(&mut doc, &mut input);
        let json = match outcome {
            CommandOutcome::Message(json) => json,
            other => panic!("DefineTool debe devolver JSON: {other:?}"),
        };
        assert!(json.contains("Saludar"));
        let mut input = format!("LoadTool[{json}]");
        // El JSON contiene comas dentro de llaves: el parser las respeta.
        let outcome = process_input(&mut doc, &mut input);
        assert!(
            matches!(outcome, CommandOutcome::Message(_)),
            "LoadTool ida y vuelta: {outcome:?}"
        );
    }

    #[test]
    fn unsupported_commands_fail_honestly() {
        for (cmd, hint) in [
            ("Execute[\"Show[A]\"]", "If/Repeat"),
            ("StartAnimation[]", "PlayPause"),
            ("StopAnimation[]", "PlayPause"),
            ("Delete[A]", "Erase"),
            ("Rename[A, B]", "etiqueta"),
        ] {
            let mut doc = doc_with_point("A");
            let mut input = cmd.to_string();
            match process_input(&mut doc, &mut input) {
                CommandOutcome::Error(message) => assert!(
                    message.contains(hint),
                    "{cmd} debe sugerir '{hint}': {message}"
                ),
                other => panic!("{cmd} debe fallar honesto, dio {other:?}"),
            }
        }
    }

    #[test]
    fn every_gd_command_has_dispatcher_arm() {
        // Cero fantasma: cada canónico G-D resuelve en registry y despacha a un
        // brazo propio (nunca "no reconocido").
        let commands = [
            "Button[x, \"Show[A]\"]",
            "Checkbox[x, v]",
            "InputBox[x, v]",
            "TextField[x, v]",
            "Show[A]",
            "Hide[A]",
            "ZoomIn[]",
            "ZoomOut[]",
            "PlayPause[]",
            "If[1, \"Show[A]\"]",
            "Repeat[1, \"Show[A]\"]",
            "DefineTool[T, \"Show[A]\"]",
            "LoadTool[{\"grafito_tool\":1,\"name\":\"T\",\"steps\":[\"Show[A]\"]}]",
            "Execute[\"Show[A]\"]",
            "StartAnimation[]",
            "StopAnimation[]",
            "Delete[A]",
            "Rename[A, B]",
        ];
        for cmd in commands {
            let canonical = cmd.split('[').next().expect("comando");
            assert!(
                command_registry::resolve(canonical).is_some(),
                "{canonical} debe estar registrado"
            );
            let mut doc = doc_with_point("A");
            let mut input = cmd.to_string();
            match process_input(&mut doc, &mut input) {
                CommandOutcome::Error(message) => assert!(
                    !message.contains("no reconocido"),
                    "{cmd} sin brazo despachador: {message}"
                ),
                CommandOutcome::Ok | CommandOutcome::Message(_) => {}
            }
        }
    }

    #[test]
    fn captions_and_names_are_validated() {
        let mut doc = Document::new();
        for bad in [
            "Button[, \"Show[A]\"]",
            "Checkbox[X, 9mal]",
            "InputBox[X, \"\"]",
            "DefineTool[, \"Show[A]\"]",
        ] {
            let mut input = bad.to_string();
            assert!(
                matches!(
                    process_input(&mut doc, &mut input),
                    CommandOutcome::Error(_)
                ),
                "{bad} debe fallar"
            );
        }
    }
}
