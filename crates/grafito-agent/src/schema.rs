//! Esquema de herramientas del agente y llamadas de herramientas.

use serde_json::{json, Value};

/// Límite de caracteres del texto que una herramienta devuelve al modelo.
pub const MAX_TOOL_RESULT_CHARS: usize = 2_048;
/// Límite del nombre de una herramienta.
pub const MAX_TOOL_NAME_CHARS: usize = 64;
/// Límite de la descripción de una herramienta.
pub const MAX_TOOL_DESCRIPTION_CHARS: usize = 1_024;
/// Límite de profundidad del schema de argumentos.
const MAX_PARAMETER_DEPTH: usize = 8;

/// Descripción declarativa de una herramienta que el agente puede invocar.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSchema {
    /// Identificador único usado por el modelo en las llamadas.
    pub name: String,
    /// Instrucciones para el modelo sobre cuándo y cómo llamar.
    pub description: String,
    /// JSON Schema (objeto) de los argumentos; subconjunto seguro de serde.
    pub parameters: Value,
    /// La herramienta necesita consentimiento explícito del usuario.
    pub needs_consent: bool,
}

impl ToolSchema {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            needs_consent: false,
        }
    }

    /// Marca la tool como sensible a consentimiento explícito.
    pub fn with_consent(mut self, needs_consent: bool) -> Self {
        self.needs_consent = needs_consent;
        self
    }

    /// Valida el schema acotando campos y profundidad antes de serializarlo.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty()
            || self.name.chars().count() > MAX_TOOL_NAME_CHARS
            || self.name.chars().any(|character| character.is_control())
        {
            return Err("assistant tool name is invalid".into());
        }
        if self.description.is_empty()
            || self.description.chars().count() > MAX_TOOL_DESCRIPTION_CHARS
        {
            return Err("assistant tool description is invalid".into());
        }
        if !self.parameters.is_object() {
            return Err("assistant tool parameters must be a JSON Schema object".into());
        }
        validate_schema_depth(&self.parameters, 0)
    }

    /// Serializa la tool al formato OpenAI-compatible.
    pub fn openai_tool(&self) -> Result<Value, String> {
        self.validate()?;
        Ok(json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        }))
    }
}

fn validate_schema_depth(schema: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_PARAMETER_DEPTH {
        return Err("assistant tool parameters exceed the depth budget".into());
    }
    if let Value::Object(map) = schema {
        for (key, value) in map {
            match key.as_str() {
                "type" if value.is_string() => {}
                "type" => return Err("assistant tool schema type must be a string".into()),
                "properties" => {
                    let properties = value.as_object().ok_or_else(|| {
                        String::from("assistant tool schema properties must be an object")
                    })?;
                    for nested in properties.values() {
                        validate_schema_depth(nested, depth + 1)?;
                    }
                }
                "items" | "additionalProperties" => {
                    validate_schema_depth(value, depth + 1)?;
                }
                "enum" | "required" | "oneOf" if value.is_array() => {}
                "enum" | "required" | "oneOf" => {
                    return Err("assistant tool schema list fields must be arrays".into())
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Llamada de herramienta emitida por el modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// Id de correlación que el resultado debe replicar.
    pub id: String,
    /// Nombre de la herramienta invocada.
    pub name: String,
    /// Argumentos ya parseados y acotados.
    pub arguments: Value,
}

/// Resultado acotado que el agente devuelve al modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub call_id: String,
    pub ok: bool,
    pub content: String,
}

impl ToolResult {
    /// Construye un resultado para el modelo, truncado al presupuesto.
    pub fn text(call_id: impl Into<String>, ok: bool, content: impl Into<String>) -> Self {
        let content = content.into();
        let content = if content.chars().count() > MAX_TOOL_RESULT_CHARS {
            let mut clipped = content
                .chars()
                .take(MAX_TOOL_RESULT_CHARS.saturating_sub(1))
                .collect::<String>();
            clipped.push('…');
            clipped
        } else {
            content
        };
        Self {
            call_id: call_id.into(),
            ok,
            content,
        }
    }
}

/// Parsea las llamadas de herramienta de un mensaje assistant OpenAI-compatible.
pub fn parse_tool_calls(message: &Value) -> Result<Vec<ToolCall>, String> {
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("assistant tool_calls must live on an assistant message".into());
    }
    let calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .ok_or_else(|| "assistant message has no tool_calls array".to_string())?;
    let mut parsed = Vec::with_capacity(calls.len());
    for (index, call) in calls.iter().enumerate() {
        let function = call
            .get("function")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("assistant tool call {index} lacks a function object"))?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if name.is_empty() || name.chars().count() > MAX_TOOL_NAME_CHARS {
            return Err(format!("assistant tool call {index} has an invalid name"));
        }
        let raw_arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        if raw_arguments.chars().count() > MAX_TOOL_RESULT_CHARS {
            return Err(format!(
                "assistant tool call {index} arguments exceed the budget"
            ));
        }
        let arguments = serde_json::from_str(raw_arguments).unwrap_or_else(|_| json!({}));
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        parsed.push(ToolCall {
            id,
            name,
            arguments,
        });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_tool_calls, ToolResult, ToolSchema, MAX_TOOL_RESULT_CHARS};
    use serde_json::json;

    #[test]
    fn tool_schema_validates_and_serializes_openai() {
        let tool = ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión matemática con variables dadas.",
            json!({
                "type": "object",
                "properties": {
                    "expression": {"type": "string"},
                    "x": {"type": "number"}
                },
                "required": ["expression"]
            }),
        );
        assert!(tool.validate().is_ok());

        let rendered = tool.openai_tool().unwrap();
        assert_eq!(rendered["type"], "function");
        assert_eq!(rendered["function"]["name"], "evaluate_expr");
        assert!(rendered["function"]["parameters"]["properties"]["expression"].is_object());
    }

    #[test]
    fn tool_schema_rejects_invalid_names_and_non_object_parameters() {
        assert!(ToolSchema::new("", ".", json!({})).validate().is_err());
        assert!(ToolSchema::new("a\u{0}", ".", json!({}))
            .validate()
            .is_err());
        assert!(ToolSchema::new("ok", ".", json!([])).validate().is_err());
        assert!(ToolSchema::new("ok", ".", json!({})).validate().is_ok());
    }

    #[test]
    fn tool_result_is_truncated_to_the_budget() {
        let long = "x".repeat(MAX_TOOL_RESULT_CHARS + 50);
        let result = ToolResult::text("id-1", true, long);
        assert!(result.content.chars().count() <= MAX_TOOL_RESULT_CHARS);
        assert!(result.content.ends_with('…'));
        assert_eq!(result.call_id, "id-1");
    }

    #[test]
    fn parse_tool_calls_reads_function_arguments() {
        let message = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {
                    "id": "call-1",
                    "type": "function",
                    "function": {"name": "evaluate_expr", "arguments": "{\"expression\":\"2+2\"}"}
                }
            ]
        });
        let calls = parse_tool_calls(&message).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "evaluate_expr");
        assert_eq!(calls[0].arguments["expression"], "2+2");
    }

    #[test]
    fn parse_tool_calls_rejects_non_assistant_and_malformed_calls() {
        assert!(parse_tool_calls(&json!({"role": "user", "tool_calls": []})).is_err());
        assert!(parse_tool_calls(&json!({"role": "assistant"})).is_err());
        assert!(
            parse_tool_calls(&json!({
                "role": "assistant",
                "tool_calls": [{"function": {"name": "x", "arguments": "{"}}]
            }))
            .is_ok(),
            "falls back to an empty args object on malformed arguments"
        );
        assert!(parse_tool_calls(&json!({
            "role": "assistant",
            "tool_calls": [{"function": {"arguments": "{}"}}]
        }))
        .is_err());
    }
}
