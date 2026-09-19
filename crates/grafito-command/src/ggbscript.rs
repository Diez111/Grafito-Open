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
use grafito_core::{Decoration, DisplayFlags, Document, GeoObject, ObjectId, TooltipMode};
use grafito_geometry::expr::{evaluate, prepare_function_ast};
use grafito_geometry::{Color, Point2, ViewTransform};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

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
        {
            let var = part.strip_prefix("var=")?;
            variable = Some(var.to_string());
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
            // Z3: `Group`/`Wait` no pasan silenciosos — error honesto con
            // alternativa real: la secuencia con espera vive en la animación
            // del asistente (pedí «X y después Y» en el chat) o en los
            // comandos Repeat/PlayPause. El resto sigue con el genérico
            // fuera-del-subset.
            if canonical.eq_ignore_ascii_case("Group") || canonical.eq_ignore_ascii_case("Wait") {
                return Err(format!(
                    "paso '{step}' usa '{canonical}': la secuencia con espera vive en la animación del asistente; pedí «X y después Y» en el chat (o Repeat/PlayPause como alternativa)"
                ));
            }
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
    // P1a-2 atómico: snapshot pre-guion; si un paso falla a mitad (p. ej.
    // `Repeat 1000` con error en la iteración 500), se restaura el documento
    // previo para no dejar parcial. El presupuesto anti-DoS no se revierte.
    let doc_before = document.clone();
    script_budget.depth = script_budget.depth.saturating_add(1);
    let mut executed = 0usize;
    for step in steps {
        script_budget.ggb_steps = script_budget.ggb_steps.saturating_add(1);
        let mut nested = step.clone();
        match execute_snippet_sequence(document, &mut nested, script_budget) {
            Ok(()) => executed = executed.saturating_add(1),
            Err(message) => {
                *document = doc_before;
                script_budget.depth = script_budget.depth.saturating_sub(1);
                return Err(format!("{step}: {message} (cambios revertidos)"));
            }
        }
    }
    script_budget.depth = script_budget.depth.saturating_sub(1);
    Ok(executed)
}

/// Parte una condición en `(izquierda, operador, derecha)` por el primer
/// comparador fuera de paréntesis/corchetes/comillas (`<=`, `>=`, `==`,
/// `!=`, `<`, `>`, `=` en ese orden de preferencia).
///
/// Extracción verbatim del escaneo que vivía inline en [`eval_condition`]:
/// mismo recorrido por frontera de `char` (nunca trocea multibyte), misma
/// profundidad, mismo orden. Devuelve los lados ya recortados (pueden venir
/// vacíos: el llamador decide si eso es error).
fn split_comparison(cond: &str) -> Option<(&str, &str, &str)> {
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
    let (at, op) = op_at?;
    Some((cond[..at].trim(), op, cond[at + op.len()..].trim()))
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
    if let Some((lhs, op, rhs)) = split_comparison(cond) {
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
/// Ejecuta el guion de un botón por etiqueta (API pública para la piel).
///
/// Crea presupuesto fresco dedicado: los clicks no comparten la cota de
/// otros guiones en curso. Error honesto si no es botón o falla el guion.
pub fn run_button_script(document: &mut Document, label: &str) -> Result<usize, String> {
    let mut budget = crate::commands::ScriptBudget::default();
    press_button(document, label, &mut budget)
}

/// Ejecuta el guion `OnClick` guardado para `label` (comando `OnClick`).
///
/// Sin guion → error honesto (la piel solo llama cuando hay algo que correr
/// o un botón que pulsar; ver `run_button_script`).
pub fn run_click_script(document: &mut Document, label: &str) -> Result<usize, String> {
    let script = document
        .object_scripts
        .get(label.trim().trim_matches('"').trim_matches('\''))
        .and_then(|scripts| scripts.on_click.clone())
        .ok_or_else(|| format!("'{label}' no tiene guion OnClick"))?;
    let steps = check_script_allowlist(&script)?;
    let mut budget = crate::commands::ScriptBudget::default();
    run_ggb_steps(document, &steps, &mut budget)
}

/// Valida y guarda un guion `OnClick`/`OnUpdate` para una etiqueta.
///
/// El guion pasa el allowlist al guardar (falla rápido y honesto); la
/// ejecución de `OnUpdate` vive en P3c (ver `ObjectScripts`).
pub fn store_object_script(
    document: &mut Document,
    label: &str,
    kind: &str,
    script: &str,
) -> Result<(), String> {
    let clean = label.trim().trim_matches('"').trim_matches('\'');
    if find_object_by_label(document, clean).is_none() {
        return Err(format!("no existe el objeto '{clean}'"));
    }
    let script = unquote(script);
    if script.len() > 65_536 {
        return Err("guion excede el tamaño máximo (65536 bytes)".to_string());
    }
    check_script_allowlist(&script)?;
    let entry = document
        .object_scripts
        .entry(clean.to_string())
        .or_default();
    match kind {
        "click" => entry.on_click = Some(script.to_string()),
        "update" => entry.on_update = Some(script.to_string()),
        _ => return Err(format!("kind de guion desconocido '{kind}'")),
    }
    Ok(())
}

// ── Tortuga (P3b): mini-lenguaje FD/BK/LT/RT/PU/PD/REPEAT ─────────────

/// Operaciones máximas de un programa tortuga (anti-DoS).
pub const MAX_TURTLE_OPS: usize = 10_000;
/// Anidamiento máximo de `REPEAT`.
pub const MAX_TURTLE_DEPTH: usize = 32;
/// Sub-trazados máximos (uno por objeto Polyline).
pub const MAX_TURTLE_PATHS: usize = 64;

/// Interpreta un programa tortuga a sub-trazados `[(x, y)]`.
///
/// Lenguaje: `FD n | BK n | LT n | RT n | PU | PD | REPEAT n [ ... ]`,
/// insensible a mayúsculas, números finitos. Arranca en `(0,0)` mirando a
/// `+x` con lápiz bajo. Cada `PD` tras `PU` abre un sub-trazado nuevo.
pub(crate) fn turtle_subpaths(program: &str) -> Result<Vec<Vec<(f64, f64)>>, String> {
    let spaced = program.replace('[', " [ ").replace(']', " ] ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    if tokens.len() > MAX_TURTLE_OPS {
        return Err(format!("programa tortuga excede {MAX_TURTLE_OPS} tokens"));
    }
    let mut turtle = Turtle {
        x: 0.0,
        y: 0.0,
        heading: 0.0,
        pen: true,
        paths: vec![Vec::new()],
        ops: 0,
    };
    let mut pos = 0usize;
    turtle_seq(&tokens, &mut pos, tokens.len(), 0, &mut turtle)?;
    if pos != tokens.len() {
        return Err("programa tortuga: tokens sobrantes".to_string());
    }
    let paths: Vec<Vec<(f64, f64)>> = turtle
        .paths
        .into_iter()
        .filter(|path| path.len() >= 2)
        .collect();
    if paths.len() > MAX_TURTLE_PATHS {
        return Err(format!(
            "programa tortuga excede {MAX_TURTLE_PATHS} sub-trazados"
        ));
    }
    if paths.is_empty() {
        return Err("programa tortuga: sin trazo (¿lápiz arriba todo el tiempo?)".to_string());
    }
    Ok(paths)
}

struct Turtle {
    x: f64,
    y: f64,
    heading: f64,
    pen: bool,
    paths: Vec<Vec<(f64, f64)>>,
    ops: usize,
}

impl Turtle {
    fn step(&mut self, what: &str) -> Result<(), String> {
        self.ops = self.ops.saturating_add(1);
        if self.ops > MAX_TURTLE_OPS {
            return Err(format!(
                "programa tortuga excede {MAX_TURTLE_OPS} operaciones ({what})"
            ));
        }
        Ok(())
    }

    fn advance(&mut self, distance: f64) -> Result<(), String> {
        if !distance.is_finite() {
            return Err("tortuga: distancia no finita".to_string());
        }
        let radians = self.heading.to_radians();
        let (nx, ny) = (
            self.x + distance * radians.cos(),
            self.y + distance * radians.sin(),
        );
        if !nx.is_finite() || !ny.is_finite() {
            return Err("tortuga: posición no finita".to_string());
        }
        if self.pen {
            let current = self
                .paths
                .last_mut()
                .ok_or_else(|| "programa tortuga: sin trazado activo".to_string())?;
            if current.is_empty() {
                current.push((self.x, self.y));
            }
            current.push((nx, ny));
        }
        self.x = nx;
        self.y = ny;
        Ok(())
    }

    fn number(&mut self, tokens: &[&str], pos: &mut usize, op: &str) -> Result<f64, String> {
        let raw = tokens.get(*pos).copied().unwrap_or("");
        *pos += 1;
        let value: f64 = raw
            .parse()
            .map_err(|_| format!("tortuga: {op} espera un número, llegó '{raw}'"))?;
        if !value.is_finite() {
            return Err(format!("tortuga: {op} con número no finito"));
        }
        Ok(value)
    }
}

/// Cierre `]` que empareja el `[` en `open` (excluye anidados).
fn turtle_matching_close(tokens: &[&str], open: usize) -> Option<usize> {
    let mut nested = 0usize;
    let mut scan = open + 1;
    while let Some(&token) = tokens.get(scan) {
        match token {
            "[" => nested += 1,
            "]" if nested == 0 => return Some(scan),
            "]" => nested = nested.saturating_sub(1),
            _ => {}
        }
        scan += 1;
    }
    None
}

/// Ejecuta `tokens[pos..end)`; `]` fuera de lugar es error.
fn turtle_seq(
    tokens: &[&str],
    pos: &mut usize,
    end: usize,
    depth: usize,
    turtle: &mut Turtle,
) -> Result<(), String> {
    if depth > MAX_TURTLE_DEPTH {
        return Err(format!("tortuga: anidamiento excede {MAX_TURTLE_DEPTH}"));
    }
    while *pos < end {
        let token = tokens.get(*pos).copied().unwrap_or("");
        match token.to_ascii_uppercase().as_str() {
            "FD" => {
                *pos += 1;
                let d = turtle.number(tokens, pos, "FD")?;
                turtle.step("FD")?;
                turtle.advance(d)?;
            }
            "BK" => {
                *pos += 1;
                let d = turtle.number(tokens, pos, "BK")?;
                turtle.step("BK")?;
                turtle.advance(-d)?;
            }
            "LT" => {
                *pos += 1;
                let a = turtle.number(tokens, pos, "LT")?;
                turtle.step("LT")?;
                turtle.heading += a;
            }
            "RT" => {
                *pos += 1;
                let a = turtle.number(tokens, pos, "RT")?;
                turtle.step("RT")?;
                turtle.heading -= a;
            }
            "PU" => {
                *pos += 1;
                turtle.step("PU")?;
                turtle.pen = false;
            }
            "PD" => {
                *pos += 1;
                turtle.step("PD")?;
                turtle.pen = true;
                turtle.paths.push(Vec::new());
            }
            "REPEAT" => {
                *pos += 1;
                let n = turtle.number(tokens, pos, "REPEAT")?;
                if n < 0.0 || n.fract() != 0.0 || n > MAX_TURTLE_OPS as f64 {
                    return Err("tortuga: REPEAT espera entero 0..=10000".to_string());
                }
                let open = *pos;
                if tokens.get(open).copied().unwrap_or("") != "[" {
                    return Err("tortuga: REPEAT espera '['".to_string());
                }
                let close = turtle_matching_close(tokens, open)
                    .ok_or_else(|| "tortuga: '[' sin cierre en REPEAT".to_string())?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let times = n as usize;
                for _ in 0..times {
                    let mut inner = open + 1;
                    turtle_seq(tokens, &mut inner, close, depth + 1, turtle)?;
                    turtle.step("REPEAT")?;
                }
                *pos = close + 1;
            }
            "[" => {
                return Err("tortuga: '[' suelto (solo vale tras REPEAT)".to_string());
            }
            "]" => {
                return Err("tortuga: ']' sin '[' que lo abra".to_string());
            }
            _ => {
                return Err(format!("tortuga: instrucción desconocida '{token}'"));
            }
        }
    }
    Ok(())
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

/// Ola 0.3: `StartAnimation`/`StopAnimation` con semántica set (GeoGebra), a
/// diferencia de `PlayPause` que alterna. `start=true` deja en marcha aunque ya
/// corriera; `start=false` pausa aunque ya estuviera pausada (idempotente).
fn run_start_stop_animation(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
    start: bool,
) -> CommandOutcome {
    let name = if start {
        "StartAnimation"
    } else {
        "StopAnimation"
    };
    let state = if start { "en marcha" } else { "pausada" };
    if args.len() > 1 {
        return CommandOutcome::Error(format!("{name}: usa {name}[] o {name}[variable]"));
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
                format!(
                    "{name}: no hay variables con animación configurada (crea un Slider primero)"
                ),
            );
        }
        let mut changed = 0usize;
        for name_var in &names {
            if let Some(meta) = document.variable_meta(name_var).cloned() {
                let mut next = meta;
                next.animating = start;
                match document.try_replace_variable_meta_with_previous(name_var, next) {
                    Ok(_) => changed = changed.saturating_add(1),
                    Err(error) => return CommandOutcome::Error(format!("{name}: {error}")),
                }
            }
        }
        return outcome_message(input_text, format!("{name}: {changed} variable(s) {state}"));
    }
    let var = match check_variable_name(&args[0]) {
        Ok(var) => var,
        Err(error) => return CommandOutcome::Error(format!("{name}: {error}")),
    };
    if !document.variables.contains_key(&var) {
        return CommandOutcome::Error(format!("{name}: no existe la variable '{var}'"));
    }
    if let Some(meta) = document.variable_meta(&var).cloned() {
        let mut next = meta;
        next.animating = start;
        match document.try_replace_variable_meta_with_previous(&var, next) {
            Ok(_) => return outcome_message(input_text, format!("{name}: '{var}' {state}")),
            Err(error) => return CommandOutcome::Error(format!("{name}: {error}")),
        }
    }
    if !start {
        // Pausar algo sin animación es no-op honesto (idempotente en guiones).
        return outcome_message(
            input_text,
            format!("{name}: '{var}' ya está pausada (sin animación configurada)"),
        );
    }
    // Arrancar sin meta configura el rango por defecto, como PlayPause.
    let current = document.variables.get(&var).copied().unwrap_or(0.0);
    if !current.is_finite() {
        return CommandOutcome::Error(format!("{name}: la variable '{var}' no es finita"));
    }
    let min = current - 1.0;
    let max = current + 1.0;
    if !min.is_finite() || !max.is_finite() || min >= max {
        return CommandOutcome::Error(format!(
            "{name}: no se pudo crear un rango por defecto para '{var}'"
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
            format!("{name}: '{var}' en marcha (rango [{min}, {max}])"),
        ),
        Err(error) => CommandOutcome::Error(format!("{name}: {error}")),
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

// ── Flujo mínimo UI-adjunta F3d: historial → `.ggt` → listado ──
//
// La piel (P2) persiste el JSON en archivo `.ggt`; el núcleo sigue puro sobre
// strings (sin `std::fs` en el cerebro). Este bloque es la única superficie
// UI-adjunta: empaquetar el historial de comandos como herramienta, guardarla
// en un store en memoria y listarla para que toolbar/paleta (P2) la muestren.
// Cero comandos fantasma: todo pasa por [`define_tool_json`] /
// [`parse_tool_json`] (registry `DefineTool`/`LoadTool` existente); el store
// no despacha nada, solo describe (`describe` usa el formato de `LoadTool`).

/// Herramientas personalizadas máximas en un [`CustomToolStore`].
/// Con el JSON acotado a [`MAX_GGT_BYTES`] (64 KiB), el store pesa ≤4 MiB.
pub const MAX_CUSTOM_TOOLS: usize = 64;

/// Empaqueta el historial de comandos como JSON `.ggt` versionado.
///
/// Filtra entradas vacías, une con `"; "` y valida con [`define_tool_json`]
/// (nombre, cotas, allowlist). Historial vacío → error honesto, nada que guardar.
pub fn define_tool_from_history(name: &str, history: &[String]) -> Result<String, String> {
    let mut script = String::new();
    for entry in history {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !script.is_empty() {
            script.push_str("; ");
        }
        script.push_str(trimmed);
    }
    if script.is_empty() {
        return Err("DefineTool: el historial no contiene pasos".into());
    }
    define_tool_json(name, &script)
}

/// Store en memoria de custom tools (sesión): lo que la toolbar/paleta listan.
///
/// Puro sobre datos validados: `define`/`load_json` reusan [`define_tool_json`]
/// y [`parse_tool_json`]. Sin despacho, sin I/O, sin `unwrap`.
#[derive(Debug, Clone, Default)]
pub struct CustomToolStore {
    tools: Vec<CustomToolDef>,
    /// Época que crece en cada mutación (`upsert`). La piel la usa para
    /// reconstruir las entradas de paleta solo cuando el store cambia, en
    /// vez de clonar `name`/`steps`/`keywords` en cada frame.
    revision: u64,
}

impl CustomToolStore {
    /// Store vacío.
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            revision: 0,
        }
    }

    /// Época de mutación (ver campo `revision`).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Cantidad de herramientas guardadas.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// `true` si no hay herramientas guardadas.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Todas las herramientas en orden de carga (para listar en UI).
    pub fn list(&self) -> &[CustomToolDef] {
        &self.tools
    }

    /// Busca una herramienta por nombre exacto.
    pub fn get(&self, name: &str) -> Option<&CustomToolDef> {
        let wanted = name.trim();
        self.tools.iter().find(|tool| tool.name == wanted)
    }

    /// Define desde una secuencia, guarda (reemplaza si el nombre existe) y
    /// devuelve el JSON `.ggt` listo para persistir en archivo (P2).
    /// Store lleno (y nombre nuevo) → error honesto, nada guardado.
    pub fn define(&mut self, name: &str, script: &str) -> Result<String, String> {
        let json = define_tool_json(name, script)?;
        let def = parse_tool_json(&json)?;
        self.upsert(def)?;
        Ok(json)
    }

    /// Define desde el historial de comandos (ver [`define_tool_from_history`]).
    pub fn define_from_history(
        &mut self,
        name: &str,
        history: &[String],
    ) -> Result<String, String> {
        let json = define_tool_from_history(name, history)?;
        let def = parse_tool_json(&json)?;
        self.upsert(def)?;
        Ok(json)
    }

    /// Carga un JSON `.ggt` (p. ej. leído de archivo por la piel) tras revalidar.
    pub fn load_json(&mut self, json: &str) -> Result<String, String> {
        let def = parse_tool_json(json)?;
        let name = def.name.clone();
        self.upsert(def)?;
        Ok(name)
    }

    /// Línea mostrable de una herramienta (mismo formato que `LoadTool`).
    /// La toolbar/paleta (P2) la renderiza sin despachar nada nuevo.
    pub fn describe(&self, name: &str) -> Option<String> {
        self.get(name).map(|tool| {
            format!(
                "'{}' válida con {} paso(s): {}",
                tool.name,
                tool.steps.len(),
                tool.steps.join(" | ")
            )
        })
    }

    /// Una línea por herramienta, en orden de carga (para toolbar/paleta).
    pub fn palette_entries(&self) -> Vec<String> {
        self.tools
            .iter()
            .map(|tool| {
                format!(
                    "'{}' válida con {} paso(s): {}",
                    tool.name,
                    tool.steps.len(),
                    tool.steps.join(" | ")
                )
            })
            .collect()
    }

    fn upsert(&mut self, def: CustomToolDef) -> Result<(), String> {
        if let Some(slot) = self.tools.iter_mut().find(|tool| tool.name == def.name) {
            *slot = def;
            self.revision = self.revision.wrapping_add(1);
            return Ok(());
        }
        if self.tools.len() >= MAX_CUSTOM_TOOLS {
            return Err(format!(
                "el store admite hasta {MAX_CUSTOM_TOOLS} herramientas"
            ));
        }
        self.tools.push(def);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

// ── Persistencia archivo `.ggt` (superficie P2-piel) ──
//
// F3d declaró la persistencia como trabajo de la piel; el núcleo sigue puro
// sobre strings y este bloque es su única I/O, autorizada como P2-piel para
// el frente W2. Validación estilo `validate_media_path`
// (`grafito-anim/src/engine.rs:886`): sin escapes del directorio base, solo
// extensión `.ggt` exacta, sin symlinks en el componente final, bytes
// acotados a [`MAX_GGT_BYTES`]. El JSON se revalida con [`parse_tool_json`]
// al guardar y al cargar: en disco nunca hay nada que el motor no ejecute
// (cero fantasma persistido).
//
// Límite honesto (sin `libc` en este crate): el rechazo de symlinks se hace
// con `symlink_metadata` (no sigue enlaces) antes de leer/escribir, no con
// `O_NOFOLLOW` atómico. La ventana TOCTOU residual es solo de usuario local
// y el contenido igual se revalida tras leer: el peor caso es cargar una
// herramienta válida desde un path inesperado, nunca código arbitrario.

/// Extensión obligatoria (exacta, minúsculas) de archivos de custom tools.
pub const GGT_FILE_EXTENSION: &str = "ggt";

/// Valida que `path` sea un archivo `.ggt` relativo contenido en `base_dir`.
///
/// Rechaza: vacío, NUL interior, absoluto, componentes `..`/`.`/prefijo,
/// sin nombre de archivo, extensión distinta de `.ggt` (incluido `.GGT`),
/// symlink en el componente final y —si los padres ya existen en disco—
///
/// escape por symlink tras `canonicalize` (el `join` debe seguir dentro del
/// base canonizado). Padres inexistentes pasan solo el chequeo léxico: la
/// verificación total ocurre al crearlos en [`save_ggt_file`].
pub fn validate_ggt_path(base_dir: &Path, path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("ggt: el path no debe estar vacío".to_string());
    }
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err("ggt: el path contiene NUL".to_string());
    }
    let mut has_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal = true,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "ggt: el path '{}' escapa el directorio base",
                    path.display()
                ));
            }
            Component::CurDir => {
                return Err(format!(
                    "ggt: el path '{}' debe ser relativo simple",
                    path.display()
                ));
            }
        }
    }
    if !has_normal {
        return Err("ggt: el path no nombra un archivo".to_string());
    }
    let extension_ok = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == GGT_FILE_EXTENSION);
    if !extension_ok {
        return Err(format!(
            "ggt: solo archivos '.{GGT_FILE_EXTENSION}' (recibido '{}')",
            path.display()
        ));
    }
    // Symlink en el componente final: nunca se sigue, se rechaza.
    let full = base_dir.join(path);
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            return Err(format!("ggt: '{}' es un enlace simbólico", path.display()));
        }
    }
    // Si los padres existen, el canonizado debe seguir dentro del base.
    let base_canon = std::fs::canonicalize(base_dir)
        .map_err(|error| format!("ggt: directorio base inválido: {error}"))?;
    if let Some(parent) = full.parent() {
        if let Ok(canon_parent) = std::fs::canonicalize(parent) {
            if !canon_parent.starts_with(&base_canon) {
                return Err(format!(
                    "ggt: el path '{}' escapa el directorio base",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Guarda un JSON `.ggt` en `base_dir/path`.
///
/// Revalida el JSON con [`parse_tool_json`] antes de tocar disco (nada
/// inválido se persiste) y acota a [`MAX_GGT_BYTES`]. Crea padres solo bajo
// el base ya validado y re-chequea el escape tras crearlos.
pub fn save_ggt_file(base_dir: &Path, path: &Path, json: &str) -> Result<(), String> {
    validate_ggt_path(base_dir, path).map_err(|error| format!("save .ggt: {error}"))?;
    if json.len() > MAX_GGT_BYTES {
        return Err(format!("save .ggt: el JSON excede {MAX_GGT_BYTES} bytes"));
    }
    parse_tool_json(json).map_err(|error| format!("save .ggt: {error}"))?;
    let full = base_dir.join(path);
    if let Some(parent) = full.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("save .ggt: no se pudo crear padres: {error}"))?;
        }
        // Re-chequeo post-mkdir: si un padre apareció como symlink entre el
        // validate y el mkdir, el canonizado lo delata.
        let base_canon = std::fs::canonicalize(base_dir)
            .map_err(|error| format!("save .ggt: directorio base inválido: {error}"))?;
        let canon_parent = std::fs::canonicalize(parent)
            .map_err(|error| format!("save .ggt: padres inválidos: {error}"))?;
        if !canon_parent.starts_with(&base_canon) {
            return Err(format!(
                "save .ggt: el path '{}' escapa el directorio base",
                path.display()
            ));
        }
    }
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            return Err(format!(
                "save .ggt: '{}' es un enlace simbólico",
                path.display()
            ));
        }
        if !meta.file_type().is_file() {
            return Err(format!(
                "save .ggt: '{}' no es un archivo regular",
                path.display()
            ));
        }
    }
    std::fs::write(&full, json).map_err(|error| format!("save .ggt: {error}"))?;
    Ok(())
}

/// Carga y revalida un archivo `.ggt` desde `base_dir/path`.
///
/// Rechaza symlinks, no-regulares y tamaños sobre [`MAX_GGT_BYTES`] antes de
/// leer; el contenido se revalida con [`parse_tool_json`] (versión, nombre,
/// cotas, allowlist). Lo que vuelve es ejecutable por el motor o es error.
pub fn load_ggt_file(base_dir: &Path, path: &Path) -> Result<CustomToolDef, String> {
    validate_ggt_path(base_dir, path).map_err(|error| format!("load .ggt: {error}"))?;
    let full = base_dir.join(path);
    let meta = std::fs::symlink_metadata(&full)
        .map_err(|error| format!("load .ggt: no se pudo leer '{}': {error}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "load .ggt: '{}' es un enlace simbólico",
            path.display()
        ));
    }
    if !meta.file_type().is_file() {
        return Err(format!(
            "load .ggt: '{}' no es un archivo regular",
            path.display()
        ));
    }
    if meta.len() > MAX_GGT_BYTES as u64 {
        return Err(format!(
            "load .ggt: el archivo excede {MAX_GGT_BYTES} bytes"
        ));
    }
    let bytes = std::fs::read(&full).map_err(|error| format!("load .ggt: {error}"))?;
    if bytes.len() > MAX_GGT_BYTES {
        return Err(format!(
            "load .ggt: el archivo excede {MAX_GGT_BYTES} bytes"
        ));
    }
    let text =
        String::from_utf8(bytes).map_err(|error| format!("load .ggt: UTF-8 inválido: {error}"))?;
    parse_tool_json(&text).map_err(|error| format!("load .ggt: {error}"))
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

/// Tope de bytes para guiones Execute/OnClick (espejo de
/// `commands::MAX_COMMAND_INPUT_BYTES`, privado de ese módulo).
const MAX_SCRIPT_TEXT_BYTES: usize = 65_536;

/// Ejecuta un guion del subset con presupuesto y rollback atómico (P3b).
///
/// Comparte `script_budget` con el anidado (If/Repeat/Button): sin vía de
/// escape por profundidad. Cada paso pasa el allowlist al inicio.
fn run_execute(
    document: &mut Document,
    args: &[String],
    input_text: &mut String,
    script_budget: &mut ScriptBudget,
) -> CommandOutcome {
    if args.len() != 1 {
        return CommandOutcome::Error("Execute requiere Execute[guion]".into());
    }
    let script_raw = unquote(args[0].trim());
    let script = script_raw.as_str();
    if script.len() > MAX_SCRIPT_TEXT_BYTES {
        return CommandOutcome::Error("Execute: guion excede el tamaño máximo".into());
    }
    let steps = match check_script_allowlist(script) {
        Ok(steps) => steps,
        Err(error) => return CommandOutcome::Error(format!("Execute: {error}")),
    };
    match run_ggb_steps(document, &steps, script_budget) {
        Ok(count) => {
            input_text.clear();
            CommandOutcome::Message(format!("Execute: {count} pasos"))
        }
        Err(error) => CommandOutcome::Error(format!("Execute: {error}")),
    }
}

// ── Dispatcher G-D ──────────────────────────────────────────────────

// ── Frente P3 SCRIPTING: ejecución explícita, vistas, display, export ──
//
// Todo helper acá es puro sobre `&Document` / `&mut DisplayStore`: el
// comando (fase de cableado, `commands.rs` + `command_registry.rs`) valida
// args, llama al helper y traduce el `Result` a `CommandOutcome`. Nada se
// ejecuta solo: sin hooks globales (riesgo de recursión OnUpdate).
//
// Mapa de scripts: `Document.object_scripts` (`BTreeMap<String,
// ObjectScripts>`, `document.rs`, con `#[serde(default)]`) + `OnClick` /
// `OnUpdate` ya guardados por `store_object_script`. Este frente NO crea
// ningún mapa nuevo en `Document`.
//
// Estado de vista: `ViewTransform` (`grafito-geometry/src/types.rs:93-99`)
// solo tiene `offset/scale/screen_size/x_log/y_log`: sin `show_axes`,
// `show_grid` ni pasos por eje. La geometría queda fuera de alcance, así
// que los helpers de esta sección validan y devuelven intento puro; el
// render ya respeta lo que existe (`GrafitoApp.show_grid` en
// `render_2d.rs:draw_grid`, `Document.number_plane_labels` en `draw_axes`).

/// Ejecuta el guion `OnUpdate` guardado para `label`
/// (comando `RunUpdateScript[etiqueta]`).
///
/// Disparo explícito, igual que `run_click_script`: presupuesto fresco por
/// llamada y sin hooks automáticos en ningún commit (el tracking de cambios
/// por objeto + presupuesto de recursión quedan para P3c; ver
/// `ObjectScripts` en `document.rs`). Sin guion → error honesto.
pub fn run_update_script(document: &mut Document, label: &str) -> Result<usize, String> {
    let script = document
        .object_scripts
        .get(label.trim().trim_matches('"').trim_matches('\''))
        .and_then(|scripts| scripts.on_update.clone())
        .ok_or_else(|| format!("'{label}' no tiene guion OnUpdate"))?;
    let steps = check_script_allowlist(&script)?;
    let mut budget = crate::commands::ScriptBudget::default();
    run_ggb_steps(document, &steps, &mut budget)
}

/// Ejecuta el guion `OnLoad` del documento (Ola 2.7). Sin guion → `Ok(0)`.
///
/// Valida el allowlist al ejecutar (defensa en profundidad: ya se validó al
/// guardar) y usa un presupuesto fresco. La app lo llama al reemplazar el
/// documento (abrir/importar).
pub fn run_load_script(document: &mut Document) -> Result<usize, String> {
    let Some(script) = document.on_load_script.clone() else {
        return Ok(0);
    };
    let steps = check_script_allowlist(&script)?;
    let mut budget = crate::commands::ScriptBudget::default();
    run_ggb_steps(document, &steps, &mut budget)
}

/// Etiquetas con guion `OnUpdate`, en orden determinista (Ola 2.7).
///
/// La app las ejecuta una vez por commit mutante (con guard de reentrada y
/// tope de guiones por commit).
pub fn on_update_script_labels(document: &Document) -> Vec<String> {
    document
        .object_scripts
        .iter()
        .filter(|(_, scripts)| scripts.on_update.is_some())
        .map(|(label, _)| label.clone())
        .collect()
}

/// Etiquetas máximas aceptadas por `SelectObjects` en una invocación.
pub const MAX_SELECTION_LABELS: usize = 512;

/// Resuelve etiquetas a `ObjectId` existentes, sin tocar la selección.
///
/// Puro (`&Document`): el comando limpia con `Document::clear_selection` y
/// selecciona con `Document::select` (`document.rs`; `select` ya dedup).
/// Devuelve `(encontrados, faltantes)`; lista vacía o más de
/// `MAX_SELECTION_LABELS` etiquetas → error honesto.
pub fn resolve_selection_labels(
    document: &Document,
    labels: &[String],
) -> Result<(Vec<ObjectId>, Vec<String>), String> {
    if labels.is_empty() {
        return Err("SelectObjects: pasá al menos una etiqueta".into());
    }
    if labels.len() > MAX_SELECTION_LABELS {
        return Err(format!(
            "SelectObjects: más de {MAX_SELECTION_LABELS} etiquetas"
        ));
    }
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for raw in labels {
        let clean = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if clean.is_empty() {
            return Err("SelectObjects: hay una etiqueta vacía".into());
        }
        match find_object_by_label(document, clean) {
            Some(id) => {
                if !found.contains(&id) {
                    found.push(id);
                }
            }
            None => missing.push(clean.to_string()),
        }
    }
    Ok((found, missing))
}

// ── Vistas: perspectivas, dirección, ejes, grilla ─────────────────────

/// Perspectiva canónica como destino de `SetActiveView`/`SetPerspective`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerspectiveTarget {
    /// Identificador de variante (`Geometry2D`, …​).
    pub ident: &'static str,
    /// Título largo (`Geometría 2D`, …​).
    pub title: &'static str,
    /// Etiqueta corta del selector (`G2`, …​).
    pub short: &'static str,
    /// Atajo `Ctrl+Shift+N` (1..=9, 0 = Examen).
    pub shortcut: u8,
}

/// Las 10 perspectivas, espejo de `Perspective::ALL` + `title` +
/// `short_label` + `shortcut_number` (`grafito-app/src/lib.rs:106-191`).
/// El test `perspective_mirror_covers_all_ten` pinnea cantidad y contenido:
/// si la app agrega una perspectiva, este espejo debe actualizarse.
pub const CANONICAL_PERSPECTIVES: [(&str, &str, &str, u8); 10] = [
    ("Geometry2D", "Geometría 2D", "G2", 1),
    ("Geometry3D", "Geometría 3D", "G3", 2),
    ("AlgebraCas", "Álgebra y CAS", "AL", 3),
    ("Calculus", "Cálculo", "Cλ", 4),
    ("Probability", "Probabilidad", "P", 5),
    ("Statistics", "Estadística", "S", 6),
    ("Complex", "Complejos", "i", 7),
    ("Dynamics", "Dinámica", "Dn", 8),
    ("DataAnalysis", "Análisis de datos", "D", 9),
    ("Exam", "Examen", "E", 0),
];

/// Valida el destino de `SetActiveView`/`SetPerspective` (sinónimos en
/// Grafito: una sola ventana, la perspectiva es el layout).
///
/// Acepta identificador, título o etiqueta corta (insensible a mayúsculas,
/// con o sin comillas) o número de atajo (`"1"`..`"9"`, `"0"`). El comando
/// aplica con `set_perspective` en el cableado (respeta `exam_locked`).
pub fn parse_perspective(raw: &str) -> Result<PerspectiveTarget, String> {
    let clean = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if clean.is_empty() {
        return Err("la perspectiva no debe estar vacía".into());
    }
    let lowered = clean.to_lowercase();
    for (ident, title, short, shortcut) in CANONICAL_PERSPECTIVES {
        if lowered == ident.to_lowercase()
            || lowered == title.to_lowercase()
            || lowered == short.to_lowercase()
            || lowered == shortcut.to_string()
        {
            return Ok(PerspectiveTarget {
                ident,
                title,
                short,
                shortcut,
            });
        }
    }
    Err(format!(
        "perspectiva desconocida '{clean}' (10 válidas: {})",
        CANONICAL_PERSPECTIVES
            .iter()
            .map(|(_, title, _, _)| *title)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Dirección de vista 3D (`SetViewDirection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewDirection {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}

impl ViewDirection {
    /// Nombre canónico en inglés.
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Isometric => "isometric",
        }
    }

    /// Parsea alias en español/inglés (insensible a mayúsculas, con comillas).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_lowercase()
            .as_str()
        {
            "front" | "frontal" | "frente" | "alzado" => Some(Self::Front),
            "back" | "trasera" | "posterior" => Some(Self::Back),
            "left" | "izquierda" | "lateral-izq" => Some(Self::Left),
            "right" | "derecha" | "lateral-der" => Some(Self::Right),
            "top" | "superior" | "planta" | "arriba" => Some(Self::Top),
            "bottom" | "inferior" | "abajo" => Some(Self::Bottom),
            "isometric" | "isometrica" | "isométrica" | "3d" => Some(Self::Isometric),
            _ => None,
        }
    }
}

/// Valida la dirección de `SetViewDirection`.
///
/// El `Document` no guarda cámara (la órbita 3D vive en estado de app), así
/// que el comando la aplica a la cámara en el cableado; acá solo validación.
pub fn parse_view_direction(raw: &str) -> Result<ViewDirection, String> {
    ViewDirection::parse(raw).ok_or_else(|| {
        "dirección desconocida (front, back, left, right, top, bottom, isometric)".to_string()
    })
}

/// Parsea un booleano de `ShowAxes`/`ShowGrid`/`ShowLabel`/`SetFixed`.
///
/// Acepta `true/false`, `1/0`, `sí/si/no`, `on/off` y `verdadero/falso`
/// (insensible a mayúsculas, con o sin comillas).
pub fn parse_toggle_bool(raw: &str) -> Result<bool, String> {
    match raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_lowercase()
        .as_str()
    {
        "true" | "1" | "on" | "sí" | "si" | "verdadero" => Ok(true),
        "false" | "0" | "off" | "no" | "falso" => Ok(false),
        _ => Err("se esperaba true/false (1/0, sí/no, on/off)".into()),
    }
}

/// Paso de eje máximo aceptado (`AxisStepX`/`AxisStepY`, unidades de mundo).
pub const MAX_AXIS_STEP: f64 = 1e12;

/// Valida el paso de `AxisStepX`/`AxisStepY`: finito, > 0, ≤ `MAX_AXIS_STEP`.
///
/// Sin hogar en `ViewTransform` (escala uniforme): el comando guarda el
/// intento para el cableado (grilla/ejes por eje en P3c).
pub fn parse_axis_step(raw: &str, variables: &BTreeMap<String, f64>) -> Result<f64, String> {
    let value = parse_numeric_arg(raw, variables)
        .map_err(|error| format!("paso de eje inválido: {error}"))?;
    if !value.is_finite() || value <= 0.0 || value > MAX_AXIS_STEP {
        return Err(format!(
            "el paso de eje debe ser finito entre 0 (excluido) y {MAX_AXIS_STEP}"
        ));
    }
    Ok(value)
}

/// Valida la razón de `SetAxesRatio[x, y]`: dos números finitos > 0.
///
/// `ViewTransform` es de escala uniforme (`world_to_screen` usa un solo
/// `scale`): razón no-uniforme exige cambio geométrico, fuera de alcance.
/// El comando guarda el intento validado para P3c.
pub fn parse_axes_ratio(
    x_raw: &str,
    y_raw: &str,
    variables: &BTreeMap<String, f64>,
) -> Result<(f64, f64), String> {
    let error = |side: &str| format!("SetAxesRatio: el lado {side} debe ser finito mayor a 0");
    let x = parse_numeric_arg(x_raw, variables).map_err(|_| error("x"))?;
    let y = parse_numeric_arg(y_raw, variables).map_err(|_| error("y"))?;
    if !x.is_finite() || x <= 0.0 {
        return Err(error("x"));
    }
    if !y.is_finite() || y <= 0.0 {
        return Err(error("y"));
    }
    Ok((x, y))
}

// ── Lecturas: etiqueta, coordenadas, esquina de vista, hora ────────────

/// Etiqueta existente (`Name[etiqueta]`): verifica y devuelve el `label`.
pub fn object_label_of(document: &Document, raw: &str) -> Result<String, String> {
    require_existing_label(document, raw)
}

/// Coordenadas vivas de un punto (`DynamicCoordinates[punto]`).
///
/// Solo `Point` 2D; el resto (incluido `Point3D`) da error honesto que nombra
/// el tipo real en vez de inventar una proyección.
pub fn point_coords_of(document: &Document, raw: &str) -> Result<(f64, f64), String> {
    let clean = require_existing_label(document, raw)?;
    let id = find_object_by_label(document, &clean)
        .ok_or_else(|| format!("no existe el objeto '{clean}'"))?;
    match document.get_object(id) {
        Some(GeoObject::Point(point)) => Ok((point.position.x, point.position.y)),
        Some(other) => Err(format!(
            "DynamicCoordinates: '{clean}' es {} (solo puntos 2D)",
            other.name()
        )),
        None => Err(format!("no existe el objeto '{clean}'")),
    }
}

/// Esquina visible de la vista (`Corner[n]`).
///
/// `n` 1..=4 en orden horario desde arriba-izquierda de pantalla
/// (1 = sup-izq, 2 = sup-der, 3 = inf-der, 4 = inf-izq), proyectada a mundo
/// con el `screen_size` actual. Tamaño de pantalla inválido → error honesto.
pub fn view_corner(view: &ViewTransform, n: u8) -> Result<Point2, String> {
    let (width, height) = (view.screen_size.x, view.screen_size.y);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("Corner: la vista tiene un tamaño de pantalla inválido".into());
    }
    let screen = match n {
        1 => glam::Vec2::new(0.0, 0.0),
        2 => glam::Vec2::new(width, 0.0),
        3 => glam::Vec2::new(width, height),
        4 => glam::Vec2::new(0.0, height),
        _ => {
            return Err(
                "Corner: n debe ser 1..=4 (1=sup-izq, 2=sup-der, 3=inf-der, 4=inf-izq)".into(),
            )
        }
    };
    Ok(view.screen_to_world(screen))
}

/// Parte de fecha-hora (`GetTime`: `[año, mes, día, hora, min, seg]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl TimeParts {
    /// Lista `[año, mes, día, hora, min, seg]` como la devuelve `GetTime`.
    pub const fn as_list(self) -> [i64; 6] {
        [
            self.year as i64,
            self.month as i64,
            self.day as i64,
            self.hour as i64,
            self.minute as i64,
            self.second as i64,
        ]
    }
}

/// Convierte días desde la época Unix a `(año, mes, día)` (algoritmo civil
/// de Howard Hinnant, división euclidiana: vale también pre-1970).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// Convierte segundos Unix a partes locales UTC, sin dependencias.
pub fn time_parts_from_unix(secs: i64) -> TimeParts {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    TimeParts {
        year,
        month,
        day,
        hour: (tod / 3_600) as u32,
        minute: ((tod % 3_600) / 60) as u32,
        second: (tod % 60) as u32,
    }
}

/// Hora actual del sistema como partes (`GetTime`). Reloj previo a 1970 o
/// ilegible → época Unix (nunca falla, nunca paniquea).
pub fn time_parts_now() -> TimeParts {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|delta| i64::try_from(delta.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    time_parts_from_unix(secs)
}

// ── Protocolo de construcción (vista del log de app) ────────────────────

/// Entrada del protocolo tal como la guarda `GrafitoApp.construction_log`
/// (`app.rs`; cota `MAX_CONSTRUCTION_LOG = 500`, cronológico).
///
/// El `command` no puede importar la app (dependencia invertida), así que el
/// cableado mapea `ConstructionStep → ConstructionLogEntry` y llama acá.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionLogEntry {
    pub action: String,
    pub inputs: Vec<String>,
    pub output: String,
}

/// Valida el paso `n` (1-based) contra un log de largo `len` y devuelve el
/// índice 0-based. Log vacío o fuera de rango → error honesto con el rango.
pub fn check_construction_step(n: usize, len: usize) -> Result<usize, String> {
    if len == 0 {
        return Err("el protocolo de construcción está vacío".into());
    }
    if n == 0 || n > len {
        return Err(format!("el paso debe estar entre 1 y {len}"));
    }
    Ok(n - 1)
}

/// Reporta el contenido del paso `n` (`ConstructionStep[n]`).
///
/// Solo lectura: `SetConstructionStep[n]` valida el mismo rango y reporta
/// sin time-travel (no hay rebobinado del documento; honesto por diseño).
pub fn construction_step_text(
    entries: &[ConstructionLogEntry],
    n: usize,
) -> Result<String, String> {
    let index = check_construction_step(n, entries.len())?;
    let entry = entries
        .get(index)
        .ok_or_else(|| "paso fuera de rango".to_string())?;
    Ok(format!(
        "{n}. {}({}) -> {}",
        entry.action,
        entry.inputs.join(", "),
        entry.output
    ))
}

// ── ExportImage: solo resolución pura, el worker va en el cableado ──────

/// Formato de `ExportImage[ruta]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportImageFormat {
    Png,
    Svg,
}

impl ExportImageFormat {
    /// Extensión sin punto.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }

    /// Nombre para mensajes.
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Svg => "SVG",
        }
    }

    /// Parsea una extensión (insensible a mayúsculas, sin punto).
    pub fn parse_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "svg" => Some(Self::Svg),
            _ => None,
        }
    }
}

/// Destino resuelto de `ExportImage`: ruta saneada + formato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportImageTarget {
    pub path: PathBuf,
    pub format: ExportImageFormat,
}

/// Caracteres máximos de la ruta (anti-DoS, espejo del espíritu de
/// `validate_ggt_path`; acá se permite absoluta o relativa).
pub const MAX_EXPORT_IMAGE_PATH_CHARS: usize = 1024;

/// Resuelve ruta + formato sin tocar disco (puro, testeable).
///
/// Sin extensión → `.png`. Otra extensión (`.pdf`/`.tex`/…) → error honesto
/// (PDF y TikZ tienen su propio flujo de export). `..` o NUL → error.
/// Decisión de I/O documentada: el render (`build_export_scene` +
/// `render_png`, `export.rs`) es pesado y la escritura va por worker
/// (`PendingExportJob`, precedente en `app.rs`), así que el comando solo
/// resuelve y el cableado encola el worker con `export_png`/`export_document`.
pub fn resolve_export_image(raw: &str) -> Result<ExportImageTarget, String> {
    let clean = unquote(raw).trim().to_string();
    if clean.is_empty() {
        return Err("ExportImage: la ruta no debe estar vacía".into());
    }
    if clean.chars().count() > MAX_EXPORT_IMAGE_PATH_CHARS {
        return Err(format!(
            "ExportImage: la ruta excede {MAX_EXPORT_IMAGE_PATH_CHARS} caracteres"
        ));
    }
    if clean.contains('\0') {
        return Err("ExportImage: la ruta contiene NUL".into());
    }
    let provisional = Path::new(&clean);
    if provisional
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("ExportImage: la ruta no debe contener '..'".into());
    }
    let ext = provisional
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext.is_empty() {
        return Ok(ExportImageTarget {
            path: PathBuf::from(format!("{clean}.png")),
            format: ExportImageFormat::Png,
        });
    }
    match ExportImageFormat::parse_extension(&ext) {
        Some(format) => Ok(ExportImageTarget {
            path: PathBuf::from(&clean),
            format,
        }),
        None => Err(format!(
            "ExportImage: formato '.{ext}' no soportado (solo .png/.svg; PDF y TikZ van por su propio export)"
        )),
    }
}

// ── Display por objeto: store + setters puros ────────────────────────────
//
// El store (`etiqueta → DisplayFlags`) lo posee el llamador (fase de
// cableado: estado de app + persistencia serde, ya que `DisplayFlags`
// deriva `Serialize/Deserialize`). Sin hooks globales: cada setter valida
// etiqueta existente + sintaxis y guarda; el respeto en render/input se
// cablea por comando según su veredicto (ver reporte del frente).

/// Store de flags de display por etiqueta (propiedad del llamador).
pub type DisplayStore = BTreeMap<String, DisplayFlags>;

/// Entrada del store para una etiqueta ya validada.
fn display_entry<'a>(store: &'a mut DisplayStore, clean: &str) -> &'a mut DisplayFlags {
    store.entry(clean.to_string()).or_default()
}

/// Limpia comillas/espacios y exige que la etiqueta exista en el documento.
fn require_existing_label(document: &Document, raw: &str) -> Result<String, String> {
    let clean = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    if clean.is_empty() {
        return Err("la etiqueta no debe estar vacía".into());
    }
    if find_object_by_label(document, &clean).is_none() {
        return Err(format!("no existe el objeto '{clean}'"));
    }
    Ok(clean)
}

/// Revisa paréntesis/corchetes balanceados fuera de comillas (sintaxis).
fn check_balanced_delimiters(text: &str) -> Result<(), String> {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut in_str = false;
    for ch in text.chars() {
        if ch == '"' {
            in_str = !in_str;
        } else if !in_str {
            match ch {
                '(' => paren += 1,
                ')' => {
                    paren -= 1;
                    if paren < 0 {
                        return Err("paréntesis de cierre sin apertura".into());
                    }
                }
                '[' => bracket += 1,
                ']' => {
                    bracket -= 1;
                    if bracket < 0 {
                        return Err("corchete de cierre sin apertura".into());
                    }
                }
                _ => {}
            }
        }
    }
    if in_str {
        return Err("comilla sin cerrar".into());
    }
    if paren != 0 {
        return Err("paréntesis sin balancear".into());
    }
    if bracket != 0 {
        return Err("corchetes sin balancear".into());
    }
    Ok(())
}

/// Valida sintaxis aritmética con el parser real (`prepare_function_ast`
/// con variables vacías: parsea sin necesitar valores). `what` nombra el
/// operando en el error (`lado izquierdo`, `componente rojo`, …​).
fn check_arith_syntax(expr: &str, what: &str) -> Result<(), String> {
    let clean = expr.trim();
    if clean.is_empty() {
        return Err(format!("{what}: expresión vacía"));
    }
    if clean.len() > MAX_EXPR_LENGTH {
        return Err(format!("{what}: excede {MAX_EXPR_LENGTH} caracteres"));
    }
    check_balanced_delimiters(clean).map_err(|detail| format!("{what}: {detail}"))?;
    prepare_function_ast(clean, &BTreeMap::new(), &[])
        .map(|_| ())
        .map_err(|error| format!("{what}: sintaxis inválida ({error})"))?;
    Ok(())
}

/// Valida sintaxis de condición (comparación partida por
/// `split_comparison` o expresión suelta). No evalúa: una condición puede
/// referenciar variables que existen y valer `false` hoy sin ser inválida.
pub fn check_condition_syntax(cond: &str) -> Result<(), String> {
    let clean = cond.trim();
    if clean.is_empty() {
        return Err("la condición no debe estar vacía".into());
    }
    if clean.len() > MAX_EXPR_LENGTH {
        return Err(format!("la condición excede {MAX_EXPR_LENGTH} caracteres"));
    }
    check_balanced_delimiters(clean).map_err(|detail| format!("la condición: {detail}"))?;
    if let Some((lhs, _op, rhs)) = split_comparison(clean) {
        if lhs.is_empty() || rhs.is_empty() {
            return Err(format!("condición mal formada: '{clean}'"));
        }
        check_arith_syntax(lhs, "lado izquierdo de la condición")?;
        check_arith_syntax(rhs, "lado derecho de la condición")?;
    } else {
        check_arith_syntax(clean, "la condición")?;
    }
    Ok(())
}

/// Guarda la condición (`SetConditionToShowObject[etiqueta, condición]`).
///
/// Flag-guardado: la evaluación vive en el cableado con [`eval_condition`]
/// (P3c decide el punto exacto: filtro en visibles o skip en render).
pub fn set_condition_to_show(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    cond: &str,
) -> Result<(), String> {
    let clean = require_existing_label(document, label)?;
    check_condition_syntax(cond)?;
    display_entry(store, &clean).condition = Some(cond.trim().to_string());
    Ok(())
}

/// Guarda la expresión de color (`SetDynamicColor[etiqueta, r, g, b]`).
///
/// Tres componentes en 0..=1 (se clamp al evaluar). Flag-guardado: el render
/// la evalúa por frame con [`eval_dynamic_color`] en el cableado (vía
/// `StyleOverride.color`, `render_2d.rs:2303-2326`).
pub fn set_dynamic_color(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    red: &str,
    green: &str,
    blue: &str,
) -> Result<(), String> {
    let clean = require_existing_label(document, label)?;
    check_arith_syntax(red, "componente rojo")?;
    check_arith_syntax(green, "componente verde")?;
    check_arith_syntax(blue, "componente azul")?;
    display_entry(store, &clean).dynamic_color = Some([
        red.trim().to_string(),
        green.trim().to_string(),
        blue.trim().to_string(),
    ]);
    Ok(())
}

/// Evalúa un color dinámico guardado con las variables del documento.
///
/// Cada componente debe dar finito (si no, error honesto en vez de color
/// inventado); se clamp a 0..=1. Alfa siempre 1.0 (la transparencia la
/// maneja `SetLineOpacity`). Puro: el cableado lo llama por frame.
pub fn eval_dynamic_color(document: &Document, exprs: &[String; 3]) -> Result<Color, String> {
    let vars: Vec<(String, f64)> = document
        .variables
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    let names = ["rojo", "verde", "azul"];
    let mut rgb = [0.0f32; 3];
    for (index, expr) in exprs.iter().enumerate() {
        let value = evaluate(expr, &vars)
            .map_err(|error| format!("componente {} inválido: {error}", names[index]))?;
        if !value.is_finite() {
            return Err(format!("componente {} no finito", names[index]));
        }
        rgb[index] = (value as f32).clamp(0.0, 1.0);
    }
    Ok(Color::new(rgb[0], rgb[1], rgb[2], 1.0))
}

/// Guarda el modo de tooltip (`SetTooltipMode[etiqueta, modo]`).
///
/// Flag-guardado (0 = auto, 1 = on, 2 = off + alias): el hover real
/// (`hovered_analysis`) se respeta en el cableado UI.
pub fn set_tooltip_mode(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    raw: &str,
) -> Result<TooltipMode, String> {
    let clean = require_existing_label(document, label)?;
    let mode = TooltipMode::parse(raw)
        .ok_or_else(|| "el modo debe ser 0 (auto), 1 (on) o 2 (off)".to_string())?;
    display_entry(store, &clean).tooltip_mode = mode;
    Ok(mode)
}

/// `SetVisibleInView[etiqueta, vista]`: error honesto documentado.
///
/// Grafito tiene una sola vista 2D/3D (`ViewTransform` único por documento;
/// las 10 perspectivas son layouts de UI, no vistas nombradas por objeto),
/// así que no hay destino válido que guardar. Primero valida la etiqueta
/// para que un typo reporte `no existe` en vez del genérico.
pub fn validate_visible_in_view(
    document: &Document,
    label: &str,
    view: &str,
) -> Result<(), String> {
    let clean = require_existing_label(document, label)?;
    Err(format!(
        "SetVisibleInView: '{clean}' pide vista '{view}', pero Grafito tiene una sola vista por documento (sin vistas múltiples nombradas); las 10 perspectivas son layouts de UI, no destinos por objeto"
    ))
}

/// Guarda la visibilidad de etiqueta (`SetLabelMode`/`ShowLabel[etiqueta, bool]`).
///
/// Flag-guardado: el canvas dibuja etiquetas vía `get_label` + `hide_label`
/// de `StyleOverride` (`render_2d.rs:2358-2365`); el cableado alimenta el
/// override desde el store.
pub fn set_show_label(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    raw: &str,
) -> Result<bool, String> {
    let clean = require_existing_label(document, label)?;
    let show = parse_toggle_bool(raw)?;
    display_entry(store, &clean).show_label = show;
    Ok(show)
}

/// Fija un objeto (`SetFixed[etiqueta, bool]`).
///
/// Flag-guardado: el drag vive en `input.rs` (`is_free_object`, sin acceso
/// al store que poseerá la app) así que el cableado suma `!is_locked` al
/// gate de `try_move_point_and_re_evaluate` (una línea, sin cambiar firmas).
pub fn set_locked(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    raw: &str,
) -> Result<bool, String> {
    let clean = require_existing_label(document, label)?;
    let locked = parse_toggle_bool(raw)?;
    display_entry(store, &clean).locked = locked;
    Ok(locked)
}

/// ¿Está fijo este rótulo? Etiqueta ausente o sin flag → `false`.
pub fn is_locked(store: &DisplayStore, label: &str) -> bool {
    let clean = label.trim().trim_matches('"').trim_matches('\'').trim();
    store.get(clean).is_some_and(|flags| flags.locked)
}

/// `SetImage[etiqueta, ruta]`: error honesto documentado.
///
/// No hay pipeline de imágenes para objetos: ningún `GeoObject::Image`
/// existe en `object.rs` y `Tool::Image` está deshabilitado
/// (`tool_dispatcher.rs` → `unavailable_tool`, `ui.rs` avisa "no
/// disponible"). Inventar un objeto rompería validación/render/export.
pub fn check_set_image(document: &Document, label: &str, _path: &str) -> Result<(), String> {
    let clean = require_existing_label(document, label)?;
    Err(format!(
        "SetImage: '{clean}' sin pipeline de imágenes para objetos (Tool::Image no disponible en esta versión)"
    ))
}

/// Guarda marcas de ángulo/segmento (`SetDecoration[etiqueta, n]`, 0..=4).
///
/// Flag-guardado + nota: el render 2D no tiene punto de inserción limpio
/// para tildes (cada familia dibuja su propio trazo), así que el cableado
/// dibuja los ticks; acá solo validación y guarda.
pub fn set_decoration(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    raw: &str,
) -> Result<Decoration, String> {
    let clean = require_existing_label(document, label)?;
    let decoration = Decoration::parse(raw).ok_or_else(|| {
        "la decoración debe ser 0 (ninguna), 1-3 (tildes) o 4 (flecha)".to_string()
    })?;
    display_entry(store, &clean).decoration = decoration;
    Ok(decoration)
}

/// Guarda el nivel de detalle (`SetLevelOfDetail[etiqueta, 0..=2]`).
///
/// Flag-guardado: el cableado usa [`lod_allows_dense`] como respeto mínimo
/// en render denso.
pub fn set_level_of_detail(
    store: &mut DisplayStore,
    document: &Document,
    label: &str,
    raw: &str,
) -> Result<u8, String> {
    let clean = require_existing_label(document, label)?;
    let lod = DisplayFlags::check_lod(raw)?;
    display_entry(store, &clean).lod = lod;
    Ok(lod)
}

/// Puntos máximos que un `lod` deja dibujar en una pasada densa (heurística
/// para el cableado: 0 = todo, 1 = hasta 65536, 2+ = hasta 4096).
pub const fn lod_allows_dense(lod: u8, points: usize) -> bool {
    match lod {
        0 => true,
        1 => points <= 65_536,
        _ => points <= 4_096,
    }
}

/// Opacidad de línea (`SetLineOpacity[etiqueta, 0..=1]`).
///
/// Implementado sin campos nuevos: reescribe el alfa del `color` del objeto
/// (vía `set_color`, que delega al interior en `Transformed`).
pub fn apply_line_opacity(document: &mut Document, label: &str, raw: &str) -> Result<f32, String> {
    let clean = require_existing_label(document, label)?;
    let value: f64 = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .parse()
        .map_err(|_| "la opacidad debe ser un número entre 0 y 1".to_string())?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("la opacidad debe ser finita entre 0 y 1".into());
    }
    let id = find_object_by_label(document, &clean)
        .ok_or_else(|| format!("no existe el objeto '{clean}'"))?;
    let object = document
        .get_object_mut(id)
        .ok_or_else(|| format!("objeto '{clean}' inválido"))?;
    let mut color = object.color();
    color.a = value as f32;
    object.set_color(color);
    Ok(value as f32)
}

/// Tamaño de punto (`SetPointSize[etiqueta, tamaño]`, 0.5..=64).
///
/// Implementado sobre los campos existentes (`Point.size`, `Point3D.size`,
/// `ScatterPlot.point_size`); otro tipo → error honesto que nombra el tipo
/// real (texto usa tamaño de fuente, sin comando aún).
pub fn apply_point_size(document: &mut Document, label: &str, raw: &str) -> Result<f32, String> {
    let clean = require_existing_label(document, label)?;
    let value: f64 = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .parse()
        .map_err(|_| "el tamaño debe ser un número entre 0.5 y 64".to_string())?;
    if !value.is_finite() || !(0.5..=64.0).contains(&value) {
        return Err("el tamaño debe ser finito entre 0.5 y 64".into());
    }
    let size = value as f32;
    let id = find_object_by_label(document, &clean)
        .ok_or_else(|| format!("no existe el objeto '{clean}'"))?;
    match document.get_object_mut(id) {
        Some(GeoObject::Point(point)) => point.size = size,
        Some(GeoObject::Point3D(point)) => point.size = size,
        Some(GeoObject::ScatterPlot(plot)) => plot.point_size = size,
        Some(other) => {
            return Err(format!(
                "SetPointSize: '{clean}' es {} (solo puntos 2D/3D y nubes)",
                other.name()
            ))
        }
        None => return Err(format!("no existe el objeto '{clean}'")),
    }
    Ok(size)
}

// ── Sin camino limpio: errores honestos con motivo ───────────────────────

/// `SlowPlot`: el trazado progresivo exigiría una plantilla de animación
/// nueva en el motor (las 13 plantillas nativas son fijas y ninguna hace
/// reveal progresivo de un objeto arbitrario); la animación a pedido vive en
/// el chat del asistente.
pub const SLOW_PLOT_UNAVAILABLE: &str = "SlowPlot: sin camino limpio (el motor de animación no tiene plantilla de trazado progresivo; pedí la animación en el chat del asistente)";

/// `PlaySound`: la app no reproduce audio (piper renderiza wav para muxear
/// en MP4, no reproduce; sin rodio/kira/cpal en el workspace).
pub const PLAY_SOUND_UNAVAILABLE: &str = "PlaySound: sin pipeline de reproducción de audio en la app (la voz piper solo narra para export MP4)";

/// `StartRecord`: la app no graba pantalla (el MP4 existe vía el motor de
/// animación + ffmpeg-sidecar, que es otro flujo: escenas, no captura).
pub const START_RECORD_UNAVAILABLE: &str = "StartRecord: sin grabación de pantalla en la app (el video se genera con el motor de animación, no por captura)";

/// `ToolImage`: alias del veredicto de `check_set_image` para el cableado.
pub const TOOL_IMAGE_UNAVAILABLE: &str =
    "ToolImage: sin pipeline de imágenes para objetos (Tool::Image no disponible en esta versión)";

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
        "Execute" => run_execute(document, args, input_text, script_budget),
        "StartAnimation" => run_start_stop_animation(document, args, input_text, true),
        "StopAnimation" => run_start_stop_animation(document, args, input_text, false),
        // Delete cae a handle_remaining_cas_commands (brazo propio, Ola 0.3).
        "Rename" => crate::commands::run_rename(document, args, input_text),
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
    fn ggb_steps_fallidos_revierten_atomico() {
        // P1a-2: guion que falla a mitad no deja parcial (compara serializado).
        use crate::commands::ScriptBudget;
        let mut doc = Document::new();
        doc.try_set_variable("a".into(), 0.0).expect("var");
        let before = serde_json::to_value(&doc).expect("previo serializa");
        let steps = vec![
            "SetValue[a, 1]".to_string(),
            "SetValue[a, no_existe_xyz + 1]".to_string(),
        ];
        let mut budget = ScriptBudget::default();
        let err = run_ggb_steps(&mut doc, &steps, &mut budget).expect_err("debe fallar");
        assert!(
            err.contains("revertidos"),
            "aviso atómico esperado, fue: {err}"
        );
        let after = serde_json::to_value(&doc).expect("posterior serializa");
        assert_eq!(before, after, "documento idéntico tras fallo a mitad");
    }

    #[test]
    fn group_y_wait_fallan_honesto_aun_sin_ui() {
        // Z3: nada aceptado-y-mudo — `Group`/`Wait` devuelven error honesto
        // con alternativa real (animación del asistente «X y después Y» o
        // Repeat/PlayPause), jamás silencio ni ejecución parcial.
        for cmd in [
            "Group[Show[A]]",
            "Wait[1000]",
            "group[Show[A]]",
            "wait[500]",
        ] {
            let err = check_script_allowlist(cmd).expect_err("debe rechazar");
            assert!(
                err.contains("animación del asistente"),
                "{cmd} debe apuntar al asistente, fue: {err}"
            );
            assert!(
                err.contains("Repeat") || err.contains("PlayPause"),
                "{cmd} debe sugerir alternativa, fue: {err}"
            );
        }
        // El resto fuera-del-subset sigue con su mensaje genérico.
        let err = check_script_allowlist("Delete[A]").expect_err("debe rechazar");
        assert!(err.contains("fuera del subset"), "genérico esperado: {err}");
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
    fn animation_commands_set_state() {
        // Start/Stop con semántica set (GeoGebra), no toggle como PlayPause.
        let mut doc = Document::new();
        // Sin sliders: mensaje honesto, no error.
        let mut input = "StartAnimation[]".to_string();
        match process_input(&mut doc, &mut input) {
            CommandOutcome::Message(message) => assert!(
                message.contains("StartAnimation") && message.contains("Slider"),
                "sin sliders honesto: {message}"
            ),
            other => panic!("StartAnimation[] debe dar Message, dio {other:?}"),
        }
        // Con slider: Start pone en marcha, Stop pausa (idempotente).
        let mut slider = "Slider[v, 0, 10, 1]".to_string();
        assert!(
            matches!(
                process_input(&mut doc, &mut slider),
                CommandOutcome::Message(_)
            ),
            "Slider de prueba"
        );
        let mut start = "StartAnimation[v]".to_string();
        match process_input(&mut doc, &mut start) {
            CommandOutcome::Message(message) => {
                assert!(message.contains("en marcha"), "start: {message}")
            }
            other => panic!("StartAnimation[v] debe dar Message, dio {other:?}"),
        }
        assert!(
            doc.variable_meta("v").is_some_and(|meta| meta.animating),
            "StartAnimation[v] deja animating=true"
        );
        // Start repetido no alterna (diferencia con PlayPause).
        let mut start2 = "StartAnimation[v]".to_string();
        let _ = process_input(&mut doc, &mut start2);
        assert!(
            doc.variable_meta("v").is_some_and(|meta| meta.animating),
            "StartAnimation repetido sigue en marcha"
        );
        let mut stop = "StopAnimation[]".to_string();
        match process_input(&mut doc, &mut stop) {
            CommandOutcome::Message(message) => {
                assert!(message.contains("pausada"), "stop: {message}")
            }
            other => panic!("StopAnimation[] debe dar Message, dio {other:?}"),
        }
        assert!(
            doc.variable_meta("v").is_some_and(|meta| !meta.animating),
            "StopAnimation[] deja animating=false"
        );
        // Stop sobre variable sin animación: no-op honesto, no error.
        doc.try_set_variable("v2".to_string(), 3.0)
            .expect("variable de prueba");
        let mut stop_plain = "StopAnimation[v2]".to_string();
        match process_input(&mut doc, &mut stop_plain) {
            CommandOutcome::Message(message) => {
                assert!(message.contains("pausada"), "stop sin meta: {message}")
            }
            other => panic!("StopAnimation[v2] debe dar Message, dio {other:?}"),
        }
        // Variable inexistente: error honesto.
        let mut missing = "StartAnimation[q]".to_string();
        match process_input(&mut doc, &mut missing) {
            CommandOutcome::Error(message) => {
                assert!(message.contains("no existe"), "var inexistente: {message}")
            }
            other => panic!("var inexistente debe dar Error, dio {other:?}"),
        }
    }

    #[test]
    fn delete_removes_object_by_label() {
        // Delete[objeto] es el nombre GeoGebra de Erase[etiqueta].
        let mut doc = doc_with_point("A");
        let mut input = "Delete[A]".to_string();
        match process_input(&mut doc, &mut input) {
            CommandOutcome::Message(message) => {
                assert!(message.contains("borrado"), "delete: {message}")
            }
            other => panic!("Delete[A] debe dar Message, dio {other:?}"),
        }
        assert!(
            doc.objects_iter().next().is_none(),
            "Delete[A] borra el objeto"
        );
        let mut missing = "Delete[A]".to_string();
        match process_input(&mut doc, &mut missing) {
            CommandOutcome::Error(message) => {
                assert!(message.contains("no encontrado"), "doble delete: {message}")
            }
            other => panic!("Delete repetido debe dar Error, dio {other:?}"),
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

    #[test]
    fn define_from_history_round_trip() {
        // F3d: el historial se empaqueta como `.ggt` y vuelve a validar.
        let history = vec![
            "Show[A]".to_string(),
            "  ".to_string(),
            "Hide[A]".to_string(),
        ];
        let json = define_tool_from_history("MiMacro", &history).expect("define");
        assert!(json.contains("\"grafito_tool\":1"));
        let tool = parse_tool_json(&json).expect("parse");
        assert_eq!(tool.name, "MiMacro");
        assert_eq!(tool.steps.len(), 2);

        // Historial vacío o solo blancos → error honesto.
        assert!(define_tool_from_history("Vacia", &[]).is_err());
        assert!(define_tool_from_history("Blancos", &["   ".to_string()]).is_err());
        // Paso fuera del subset → error honesto (misma allowlist).
        assert!(define_tool_from_history("Mala", &["EraseAll[]".to_string()]).is_err());
        // Nombre inválido → error honesto.
        assert!(define_tool_from_history("9mal", &history).is_err());
    }

    #[test]
    fn custom_tool_store_defines_lists_and_replaces() {
        // F3d: guardar + cargar + listar usable (toolbar/paleta P2).
        let mut store = CustomToolStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        let json = store.define("Macro", "Show[A]; Hide[A]").expect("define");
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("Macro").expect("get").steps.len(), 2);
        // `describe` usa el formato de `LoadTool`: la UI lo muestra sin despachar.
        assert_eq!(
            store.describe("Macro").expect("describe"),
            "'Macro' válida con 2 paso(s): Show[A] | Hide[A]"
        );
        assert_eq!(store.palette_entries().len(), 1);

        // Redefinir el mismo nombre reemplaza, no duplica.
        store.define("Macro", "Show[A]").expect("redefine");
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("Macro").expect("get").steps.len(), 1);

        // `load_json` revalida: el JSON de `define` entra, el maligno no.
        assert_eq!(store.load_json(&json).expect("load"), "Macro");
        assert_eq!(store.len(), 1);
        let evil = "{\"grafito_tool\":1,\"name\":\"Evil\",\"steps\":[\"EraseAll[]\"]}";
        assert!(store.load_json(evil).is_err());
        assert!(store.get("Evil").is_none());
        assert!(store.describe("Fantasma").is_none());

        // Historial → store en una llamada.
        let history = vec!["ZoomIn[]".to_string()];
        store
            .define_from_history("Acerca", &history)
            .expect("from history");
        assert_eq!(store.len(), 2);
        assert_eq!(store.palette_entries().len(), 2);
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn custom_tool_store_is_bounded() {
        // Presupuesto: hasta MAX_CUSTOM_TOOLS; lleno → error honesto.
        let mut store = CustomToolStore::new();
        for i in 0..MAX_CUSTOM_TOOLS {
            let name = format!("Tool{i:03}");
            store.define(&name, "Show[A]").expect("define");
        }
        assert_eq!(store.len(), MAX_CUSTOM_TOOLS);
        assert!(store.define("DeMas", "Show[A]").is_err());
        // Reemplazar un nombre existente con el store lleno sí vale.
        store.define("Tool000", "Hide[A]").expect("replace");
        assert_eq!(store.len(), MAX_CUSTOM_TOOLS);
        assert_eq!(store.get("Tool000").expect("get").steps.len(), 1);
    }

    #[test]
    fn custom_tool_steps_use_only_registered_commands() {
        // Cero fantasma: cada paso del store resuelve en el registry y está
        // en la allowlist del subset GGBScript.
        let mut store = CustomToolStore::new();
        store
            .define("Todo", "Show[A]; Hide[A]; ZoomIn[]; PlayPause[]")
            .expect("define");
        let tool = store.get("Todo").expect("get");
        for step in &tool.steps {
            let parsed = parse_cas_command(step).expect("paso parseable");
            let canonical = command_registry::canonicalize(&parsed.command).expect("registrado");
            assert!(
                GGBSCRIPT_ALLOWLIST
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(canonical)),
                "{step} fuera del subset"
            );
        }
    }

    // ── Persistencia `.ggt` (P2-piel): ida y vuelta + rechazos ──

    #[cfg(test)]
    fn ggt_tmp_base(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "grafito_ggt_{}_{}_{tag}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("tmp base");
        dir
    }

    #[test]
    fn ggt_file_round_trip_save_load() {
        let base = ggt_tmp_base("roundtrip");
        let json = define_tool_json("MiMacro", "Show[A]; Hide[A]").expect("define");
        save_ggt_file(&base, std::path::Path::new("macros/macro.ggt"), &json)
            .expect("save con padres inexistentes");
        assert!(base.join("macros/macro.ggt").is_file());
        let tool = load_ggt_file(&base, std::path::Path::new("macros/macro.ggt")).expect("load");
        assert_eq!(tool.name, "MiMacro");
        assert_eq!(tool.steps.len(), 2);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ggt_file_never_persists_invalid_json() {
        let base = ggt_tmp_base("invalid");
        let evil = "{\"grafito_tool\":1,\"name\":\"Evil\",\"steps\":[\"EraseAll[]\"]}";
        let path = std::path::Path::new("evil.ggt");
        assert!(save_ggt_file(&base, path, evil).is_err());
        assert!(!base.join(path).exists(), "nada inválido queda en disco");
        // JSON gigante (válido en forma, excedido en bytes) tampoco se guarda.
        let big = format!(
            "{{\"grafito_tool\":1,\"name\":\"G\",\"steps\":[\"Show[A]\"]}}{}",
            " ".repeat(MAX_GGT_BYTES)
        );
        assert!(save_ggt_file(&base, path, &big).is_err());
        assert!(!base.join(path).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ggt_path_rejects_escape_and_wrong_extension() {
        let base = ggt_tmp_base("paths");
        for bad in [
            "",
            "tool.json",
            "tool.GGT",
            "tool",
            "/absoluta.ggt",
            "../afuera.ggt",
            "sub/../../afuera.ggt",
            "./relativa.ggt",
        ] {
            assert!(
                validate_ggt_path(&base, std::path::Path::new(bad)).is_err(),
                "{bad:?} debe rechazarse"
            );
        }
        assert!(validate_ggt_path(&base, std::path::Path::new("ok.ggt")).is_ok());
        assert!(validate_ggt_path(&base, std::path::Path::new("sub/ok.ggt")).is_ok());
        // Cargar lo inexistente falla honesto (no pánico).
        assert!(load_ggt_file(&base, std::path::Path::new("falta.ggt")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ggt_file_rejects_oversized_and_non_utf8() {
        let base = ggt_tmp_base("sizes");
        std::fs::write(base.join("grande.ggt"), vec![b'x'; MAX_GGT_BYTES + 1])
            .expect("write grande");
        assert!(load_ggt_file(&base, std::path::Path::new("grande.ggt")).is_err());
        std::fs::write(base.join("bin.ggt"), [0xFF, 0xFE, 0x00]).expect("write bin");
        assert!(load_ggt_file(&base, std::path::Path::new("bin.ggt")).is_err());
        // Directorio con extensión .ggt no es archivo regular.
        std::fs::create_dir_all(base.join("dir.ggt")).expect("mkdir");
        assert!(load_ggt_file(&base, std::path::Path::new("dir.ggt")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    #[cfg(unix)]
    fn ggt_file_rejects_symlinks() {
        use std::os::unix::ffi::OsStringExt;
        let base = ggt_tmp_base("links");
        let json = define_tool_json("Real", "ZoomIn[]").expect("define");
        std::fs::write(base.join("real.ggt"), &json).expect("write real");
        std::os::unix::fs::symlink(base.join("real.ggt"), base.join("link.ggt")).expect("symlink");
        assert!(load_ggt_file(&base, std::path::Path::new("link.ggt")).is_err());
        assert!(
            save_ggt_file(&base, std::path::Path::new("link.ggt"), &json).is_err(),
            "no se escribe sobre symlinks"
        );
        // NUL interior se rechaza antes de tocar disco.
        let nul = std::ffi::OsString::from_vec(b"nu\0l.ggt".to_vec());
        assert!(validate_ggt_path(&base, std::path::Path::new(&nul)).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── Frente P3 SCRIPTING ──────────────────────────────────────────

    fn doc_with_button() -> (Document, String) {
        let mut doc = doc_with_point("A");
        let mut input = "Button[B, \"Show[A]\"]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        let label = last_text_label(&doc);
        (doc, label)
    }

    fn empty_vars() -> BTreeMap<String, f64> {
        BTreeMap::new()
    }

    #[test]
    fn update_script_runs_explicitly_and_missing_is_honest() {
        let mut doc = doc_with_point("A");
        assert!(run_update_script(&mut doc, "A").is_err());
        let mut input = "OnUpdate[A, \"Show[A]\"]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        assert_eq!(run_update_script(&mut doc, "A"), Ok(1));
        assert_eq!(run_update_script(&mut doc, "\"A\""), Ok(1));
        assert!(run_update_script(&mut doc, "Falta").is_err());
    }

    #[test]
    fn click_script_pin_and_selection_resolver() {
        let mut doc = doc_with_point("A");
        let mut input = "OnClick[A, \"Show[A]\"]".to_string();
        assert!(matches!(
            process_input(&mut doc, &mut input),
            CommandOutcome::Message(_)
        ));
        assert_eq!(run_click_script(&mut doc, "A"), Ok(1));

        let (found, missing) =
            resolve_selection_labels(&doc, &["A".to_string(), "A".to_string(), "Z".to_string()])
                .expect("resuelve");
        assert_eq!(found.len(), 1);
        assert_eq!(missing, vec!["Z".to_string()]);
        assert!(resolve_selection_labels(&doc, &[]).is_err());
        assert!(resolve_selection_labels(&doc, &["  ".to_string()]).is_err());
    }

    #[test]
    fn perspective_mirror_covers_all_ten() {
        assert_eq!(CANONICAL_PERSPECTIVES.len(), 10);
        let mut shortcuts = std::collections::BTreeSet::new();
        for (_, _, _, shortcut) in CANONICAL_PERSPECTIVES {
            assert!(shortcuts.insert(shortcut), "atajo duplicado");
        }
        assert_eq!(
            parse_perspective("geometría 2d").expect("g2").ident,
            "Geometry2D"
        );
        assert_eq!(parse_perspective("\"G3\"").expect("g3").shortcut, 2);
        assert_eq!(parse_perspective("AL").expect("al").ident, "AlgebraCas");
        assert_eq!(parse_perspective("cλ").expect("calc").ident, "Calculus");
        assert_eq!(parse_perspective("0").expect("exam").ident, "Exam");
        assert_eq!(
            parse_perspective("Análisis de datos").expect("d").short,
            "D"
        );
        assert!(parse_perspective("").is_err());
        assert!(parse_perspective("Narnia").is_err());
    }

    #[test]
    fn view_direction_aliases_and_toggle_bool() {
        assert_eq!(ViewDirection::parse("frontal"), Some(ViewDirection::Front));
        assert_eq!(ViewDirection::parse("\"TOP\""), Some(ViewDirection::Top));
        assert_eq!(
            ViewDirection::parse("isométrica"),
            Some(ViewDirection::Isometric)
        );
        assert_eq!(
            parse_view_direction("3d").expect("3d"),
            ViewDirection::Isometric
        );
        assert!(parse_view_direction("nadir").is_err());
        assert_eq!(parse_toggle_bool("true"), Ok(true));
        assert_eq!(parse_toggle_bool("0"), Ok(false));
        assert_eq!(parse_toggle_bool("SÍ"), Ok(true));
        assert_eq!(parse_toggle_bool("\"no\""), Ok(false));
        assert!(parse_toggle_bool("quizás").is_err());
    }

    #[test]
    fn axis_step_and_ratio_validate_ranges() {
        assert_eq!(parse_axis_step("2", &empty_vars()), Ok(2.0));
        assert!(parse_axis_step("0", &empty_vars()).is_err());
        assert!(parse_axis_step("-3", &empty_vars()).is_err());
        assert!(parse_axis_step("abc", &empty_vars()).is_err());
        assert_eq!(parse_axes_ratio("16", "9", &empty_vars()), Ok((16.0, 9.0)));
        assert!(parse_axes_ratio("0", "9", &empty_vars()).is_err());
        assert!(parse_axes_ratio("16", "-1", &empty_vars()).is_err());
    }

    #[test]
    fn view_corners_map_screen_to_world() {
        let doc = Document::new();
        let view = doc.view();
        let c1 = view_corner(view, 1).expect("c1");
        let c3 = view_corner(view, 3).expect("c3");
        assert!((c1.x + 8.0).abs() < 1e-9, "c1.x={}", c1.x);
        assert!((c1.y - 6.0).abs() < 1e-9, "c1.y={}", c1.y);
        assert!((c3.x - 8.0).abs() < 1e-9, "c3.x={}", c3.x);
        assert!((c3.y + 6.0).abs() < 1e-9, "c3.y={}", c3.y);
        assert!(view_corner(view, 0).is_err());
        assert!(view_corner(view, 5).is_err());
    }

    #[test]
    fn unix_time_vectors_cover_epoch_leap_and_negative() {
        let epoch = time_parts_from_unix(0);
        assert_eq!(epoch.as_list(), [1970, 1, 1, 0, 0, 0]);
        assert_eq!(
            time_parts_from_unix(946_684_800).as_list(),
            [2000, 1, 1, 0, 0, 0]
        );
        assert_eq!(
            time_parts_from_unix(1_582_934_400).as_list(),
            [2020, 2, 29, 0, 0, 0]
        );
        assert_eq!(
            time_parts_from_unix(-1).as_list(),
            [1969, 12, 31, 23, 59, 59]
        );
        assert_eq!(
            time_parts_from_unix(1_690_848_000).as_list(),
            [2023, 8, 1, 0, 0, 0]
        );
        let now = time_parts_now();
        assert!((1..=12).contains(&now.month));
        assert!((1..=31).contains(&now.day));
        assert!(now.hour < 24 && now.minute < 60 && now.second < 60);
    }

    #[test]
    fn label_and_coords_readers_are_honest() {
        let (doc, button) = doc_with_button();
        assert_eq!(object_label_of(&doc, "A"), Ok("A".to_string()));
        assert!(object_label_of(&doc, "Falta").is_err());
        assert_eq!(point_coords_of(&doc, "A"), Ok((1.0, 2.0)));
        let err = point_coords_of(&doc, &button).expect_err("texto no es punto");
        assert!(err.contains("solo puntos"), "{err}");
    }

    #[test]
    fn construction_log_reports_without_time_travel() {
        let entries = vec![
            ConstructionLogEntry {
                action: "Point".to_string(),
                inputs: vec![],
                output: "A".to_string(),
            },
            ConstructionLogEntry {
                action: "Circle".to_string(),
                inputs: vec!["A".to_string(), "B".to_string()],
                output: "C".to_string(),
            },
        ];
        assert_eq!(
            construction_step_text(&entries, 2).expect("paso 2"),
            "2. Circle(A, B) -> C"
        );
        assert_eq!(check_construction_step(1, 2), Ok(0));
        assert!(construction_step_text(&entries, 0).is_err());
        assert!(construction_step_text(&entries, 3).is_err());
        assert!(construction_step_text(&[], 1).is_err());
    }

    #[test]
    fn export_image_resolves_purely_and_rejects() {
        let target = resolve_export_image("foto.png").expect("png");
        assert_eq!(target.format, ExportImageFormat::Png);
        assert_eq!(target.path, PathBuf::from("foto.png"));
        let svg = resolve_export_image("dir/graf.SVG").expect("svg");
        assert_eq!(svg.format, ExportImageFormat::Svg);
        let def = resolve_export_image("foto").expect("default png");
        assert_eq!(def.format, ExportImageFormat::Png);
        assert_eq!(def.path, PathBuf::from("foto.png"));
        assert!(resolve_export_image("").is_err());
        assert!(resolve_export_image("../afuera.png").is_err());
        assert!(resolve_export_image("a\0b.png").is_err());
        let pdf = resolve_export_image("doc.pdf").expect_err("pdf aparte");
        assert!(pdf.contains(".pdf"), "{pdf}");
    }

    #[test]
    fn condition_syntax_is_parse_level_not_eval_level() {
        assert!(check_condition_syntax("a > 1").is_ok());
        assert!(check_condition_syntax("x").is_ok());
        assert!(check_condition_syntax("2*(a+b) <= 10").is_ok());
        assert!(check_condition_syntax("").is_err());
        assert!(check_condition_syntax(">").is_err());
        assert!(check_condition_syntax("a >").is_err());
        assert!(check_condition_syntax("(a > 1").is_err());
        assert!(check_condition_syntax("\"abierta").is_err());
        // La refactorización no cambió la evaluación real.
        let doc = doc_with_point("A");
        assert_eq!(eval_condition(&doc, "2 > 1"), Ok(true));
        assert_eq!(eval_condition(&doc, "2 < 1"), Ok(false));
    }

    #[test]
    fn condition_and_color_store_validated_flags() {
        let doc = doc_with_point("A");
        let mut store: DisplayStore = DisplayStore::new();
        assert!(set_condition_to_show(&mut store, &doc, "A", "a > 1").is_ok());
        assert_eq!(
            store.get("A").and_then(|flags| flags.condition.clone()),
            Some("a > 1".to_string())
        );
        assert!(set_condition_to_show(&mut store, &doc, "A", ">").is_err());
        assert!(set_condition_to_show(&mut store, &doc, "Falta", "a > 1").is_err());

        assert!(set_dynamic_color(&mut store, &doc, "A", "a", "0.5", "1").is_ok());
        assert!(set_dynamic_color(&mut store, &doc, "A", "((", "0.5", "1").is_err());
        let triple = store
            .get("A")
            .and_then(|flags| flags.dynamic_color.clone())
            .expect("triple");
        let mut doc_vars = doc_with_point("A");
        doc_vars
            .try_set_variable("a".into(), 0.25)
            .expect("variable a");
        let color = eval_dynamic_color(&doc_vars, &triple).expect("color");
        assert!((color.r - 0.25).abs() < 1e-6);
        assert!((color.g - 0.5).abs() < 1e-6);
        assert_eq!(color.b, 1.0);
        // Clamp honesto + no-finito honesto.
        let clamped = eval_dynamic_color(
            &doc_vars,
            &["2".to_string(), "0".to_string(), "0".to_string()],
        )
        .expect("clamp");
        assert_eq!(clamped.r, 1.0);
        assert!(eval_dynamic_color(
            &doc_vars,
            &["1/0".to_string(), "0".to_string(), "0".to_string()]
        )
        .is_err());
    }

    #[test]
    fn tooltip_visible_in_view_and_image_verdicts() {
        let doc = doc_with_point("A");
        let mut store: DisplayStore = DisplayStore::new();
        assert_eq!(
            set_tooltip_mode(&mut store, &doc, "A", "1").expect("on"),
            TooltipMode::On
        );
        assert_eq!(
            set_tooltip_mode(&mut store, &doc, "A", "off").expect("off"),
            TooltipMode::Off
        );
        assert!(set_tooltip_mode(&mut store, &doc, "A", "7").is_err());

        let err = validate_visible_in_view(&doc, "A", "Vista2").expect_err("sin vistas múltiples");
        assert!(err.contains("una sola vista"), "{err}");
        let missing = validate_visible_in_view(&doc, "Falta", "Vista2").expect_err("typo");
        assert!(missing.contains("no existe"), "{missing}");

        let img = check_set_image(&doc, "A", "foto.png").expect_err("sin pipeline");
        assert!(img.contains("sin pipeline"), "{img}");
        assert!(check_set_image(&doc, "Falta", "foto.png").is_err());
        assert!(TOOL_IMAGE_UNAVAILABLE.contains("sin pipeline"));
    }

    #[test]
    fn label_lock_decoration_and_lod_flags() {
        let doc = doc_with_point("A");
        let mut store: DisplayStore = DisplayStore::new();
        assert!(DisplayFlags::default().show_label);
        let flags: DisplayFlags =
            serde_json::from_str("{}").expect("serde default migra a histórico");
        assert!(flags.show_label && !flags.locked && flags.lod == 0);

        assert_eq!(set_show_label(&mut store, &doc, "A", "false"), Ok(false));
        assert!(set_show_label(&mut store, &doc, "A", "quizás").is_err());
        assert_eq!(set_locked(&mut store, &doc, "A", "true"), Ok(true));
        assert!(is_locked(&store, "A"));
        assert!(is_locked(&store, "\"A\""));
        assert!(!is_locked(&store, "B"));

        assert_eq!(
            set_decoration(&mut store, &doc, "A", "2").expect("ticks"),
            Decoration::Tick2
        );
        assert_eq!(
            set_decoration(&mut store, &doc, "A", "flecha").expect("flecha"),
            Decoration::Arrow
        );
        assert!(set_decoration(&mut store, &doc, "A", "9").is_err());

        assert_eq!(set_level_of_detail(&mut store, &doc, "A", "2"), Ok(2));
        assert!(set_level_of_detail(&mut store, &doc, "A", "3").is_err());
        assert!(lod_allows_dense(0, usize::MAX));
        assert!(lod_allows_dense(1, 65_536));
        assert!(!lod_allows_dense(1, 65_537));
        assert!(lod_allows_dense(2, 4_096));
        assert!(!lod_allows_dense(2, 4_097));
        assert_eq!(TooltipMode::parse("2"), Some(TooltipMode::Off));
        assert_eq!(DisplayFlags::check_lod("1"), Ok(1));
        assert!(DisplayFlags::check_lod("5").is_err());
    }

    #[test]
    fn line_opacity_and_point_size_mutate_existing_fields() {
        let mut doc = doc_with_point("A");
        assert_eq!(apply_line_opacity(&mut doc, "A", "0.5"), Ok(0.5));
        let id = find_object_by_label(&doc, "A").expect("A");
        let alpha = doc.get_object(id).expect("obj").color().a;
        assert!((alpha - 0.5).abs() < 1e-6);
        assert!(apply_line_opacity(&mut doc, "A", "2").is_err());
        assert!(apply_line_opacity(&mut doc, "A", "-0.1").is_err());
        assert!(apply_line_opacity(&mut doc, "Falta", "0.5").is_err());

        assert_eq!(apply_point_size(&mut doc, "A", "8"), Ok(8.0));
        assert!(apply_point_size(&mut doc, "A", "0").is_err());
        assert!(apply_point_size(&mut doc, "A", "100").is_err());
        let (mut doc_btn, button) = doc_with_button();
        let err = apply_point_size(&mut doc_btn, &button, "8").expect_err("texto no es punto");
        assert!(err.contains("solo puntos"), "{err}");

        let mut scatter = Document::new();
        scatter
            .try_add_object(GeoObject::ScatterPlot(
                grafito_core::ScatterPlotObj::new(vec![1.0], vec![2.0]).with_label("Nube"),
            ))
            .expect("nube");
        assert_eq!(apply_point_size(&mut scatter, "Nube", "7"), Ok(7.0));
    }

    #[test]
    fn unavailable_features_name_themselves_honestly() {
        assert!(SLOW_PLOT_UNAVAILABLE.contains("SlowPlot"));
        assert!(PLAY_SOUND_UNAVAILABLE.contains("PlaySound"));
        assert!(START_RECORD_UNAVAILABLE.contains("StartRecord"));
    }

    /// Ola 2.7: la allowlist es benigna por construcción y queda pinneada.
    /// Cualquier comando nuevo (I/O, red, borrado) rompe este test a
    /// propósito: los guiones `OnLoad`/`OnUpdate` ahora se ejecutan solos.
    #[test]
    fn auto_script_allowlist_is_benign_and_pinned() {
        assert_eq!(
            GGBSCRIPT_ALLOWLIST,
            &[
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
            ]
        );
        // Ni I/O ni borrado masivo: nombres peligrosos fuera de la lista.
        for forbidden in [
            "Save",
            "Load",
            "Export",
            "Import",
            "PlaySound",
            "Delete",
            "Erase",
        ] {
            assert!(
                !GGBSCRIPT_ALLOWLIST.contains(&forbidden),
                "{forbidden} no debe ser auto-ejecutable"
            );
        }
    }

    #[test]
    fn run_load_script_without_script_is_a_noop() {
        let mut doc = Document::new();
        assert_eq!(run_load_script(&mut doc), Ok(0));
        doc.on_load_script = Some("SetValue[k, 5]".to_string());
        doc.try_set_variable("k".to_string(), 0.0).expect("k");
        assert_eq!(run_load_script(&mut doc), Ok(1));
        assert_eq!(doc.variables.get("k"), Some(&5.0));
    }

    #[test]
    fn on_update_labels_are_sorted_and_only_with_script() {
        let mut doc = doc_with_point("A");
        assert!(on_update_script_labels(&doc).is_empty());
        doc.object_scripts
            .entry("A".to_string())
            .or_default()
            .on_update = Some("SetValue[k, 1]".to_string());
        assert_eq!(on_update_script_labels(&doc), vec!["A".to_string()]);
    }
}
