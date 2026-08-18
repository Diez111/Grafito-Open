//! Transporte agéntico: proveedor real con tool calling y herramientas seguras.
//!
//! Conecta el núcleo de grafito-agent (loop, schema, router) con el transporte
//! OpenAI-compatible existente. Las herramientas nunca mutan el documento; sólo
//! producen resultados acotados para el modelo. El Apply de propuestas gráficas
//! sigue siendo una decisión explícita del usuario en la capa de UI.

use crate::ProviderSettings;
use grafito_agent::ledger::JSpaceLedger;
use grafito_agent::loop_engine::{
    run_agent, run_agent_with_ledger, AgentBudget, AgentChatResponse, AgentCompleter, AgentOutcome,
    Cancellation, ToolDispatcher,
};
use grafito_agent::schema::{ToolCall, ToolResult, ToolSchema};
use grafito_agent::AgentEvent;
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;

/// Límite del cuerpo de la respuesta del agente.
const MAX_AGENT_RESPONSE_BYTES: usize = 256 * 1024;

/// Proveedor de chat agéntico sobre un endpoint OpenAI-compatible.
pub struct RemoteAgentCompleter {
    settings: ProviderSettings,
    api_key: Option<String>,
}

impl RemoteAgentCompleter {
    pub fn new(settings: ProviderSettings, api_key: Option<String>) -> Self {
        Self { settings, api_key }
    }
}

impl AgentCompleter for RemoteAgentCompleter {
    fn complete(
        &self,
        messages: &[Value],
        tools: &[ToolSchema],
        max_output_tokens: usize,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> Result<AgentChatResponse, String> {
        request_agent_completion(
            &self.settings,
            self.api_key.as_deref(),
            messages,
            tools,
            max_output_tokens,
            timeout,
            cancellation,
        )
    }
}

/// Ejecuta el loop completo en un hilo de trabajo y expone sus eventos.
pub fn request_agent_on_worker(
    settings: ProviderSettings,
    api_key: Option<String>,
    system: String,
    user_messages: Vec<Value>,
    tools: Vec<ToolSchema>,
    budget: AgentBudget,
    cancellation: Cancellation,
) -> (
    JoinHandle<Result<AgentOutcome, String>>,
    Receiver<AgentEvent>,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let completer = RemoteAgentCompleter::new(settings, api_key);
        let dispatcher = SafeGrafitoDispatcher;
        run_agent(
            &completer,
            &dispatcher,
            &system,
            &user_messages,
            &tools,
            &budget,
            &cancellation,
            |event| {
                let _ = sender.send(event);
            },
        )
    });
    (handle, receiver)
}

/// Igual que request_agent_on_worker pero inyecta un ledger J-Space de tarea.
#[allow(clippy::too_many_arguments)]
pub fn request_agent_on_worker_with_ledger(
    settings: ProviderSettings,
    api_key: Option<String>,
    system: String,
    user_messages: Vec<Value>,
    tools: Vec<ToolSchema>,
    budget: AgentBudget,
    ledger: Option<JSpaceLedger>,
    cancellation: Cancellation,
) -> (
    JoinHandle<Result<AgentOutcome, String>>,
    Receiver<AgentEvent>,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let completer = RemoteAgentCompleter::new(settings, api_key);
        let dispatcher = SafeGrafitoDispatcher;
        run_agent_with_ledger(
            &completer,
            &dispatcher,
            &system,
            &user_messages,
            &tools,
            &budget,
            ledger.as_ref(),
            &cancellation,
            |event| {
                let _ = sender.send(event);
            },
        )
    });
    (handle, receiver)
}

/// Despachador de las herramientas incorporadas y seguras de Grafito.
///
/// Ninguna de estas herramientas muta el documento, accede a archivos ni
/// ejecuta código; sólo evalúan matemática y consultan conocimiento local.
pub struct SafeGrafitoDispatcher;

impl ToolDispatcher for SafeGrafitoDispatcher {
    fn dispatch(&self, call: &ToolCall) -> ToolResult {
        dispatch_safe_tool(call)
    }
}

fn dispatch_safe_tool(call: &ToolCall) -> ToolResult {
    match call.name.as_str() {
        "evaluate_expr" => evaluate_expr_tool(call),
        "grafito_docs" => grafito_docs_tool(call),
        "ask_user" => ToolResult::text(
            &call.id,
            false,
            "ask_user requires an explicit user answer in the Grafito chat; repeated it as a clarifying question instead".to_string(),
        ),
        unknown => ToolResult::text(
            &call.id,
            false,
            format!("tool '{unknown}' is not available in this session"),
        ),
    }
}

fn string_arg(call: &ToolCall, key: &str) -> Option<String> {
    call.arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
}

fn evaluate_expr_tool(call: &ToolCall) -> ToolResult {
    let Some(expression) = string_arg(call, "expression") else {
        return ToolResult::text(
            &call.id,
            false,
            "evaluate_expr requires an 'expression' string",
        );
    };
    let mut variables = Vec::new();
    if let Some(object) = call.arguments.get("variables").and_then(Value::as_object) {
        for (name, value) in object {
            if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
                variables.push((name.clone(), number));
            }
        }
    }
    match grafito_geometry::expr::evaluate(&expression, &variables) {
        Ok(value) if value.is_finite() => {
            ToolResult::text(&call.id, true, crate::format_number(value))
        }
        Ok(_) => ToolResult::text(
            &call.id,
            false,
            "the expression evaluated to a non-finite value",
        ),
        Err(error) => ToolResult::text(&call.id, false, format!("evaluation failed: {error}")),
    }
}

fn grafito_docs_tool(call: &ToolCall) -> ToolResult {
    let query = string_arg(call, "query").unwrap_or_default();
    let catalog = grafito_command::assistant_context::assistant_tool_catalog(&query, 2_048);
    if catalog.trim().is_empty() {
        return ToolResult::text(
            &call.id,
            false,
            "there is no catalogued Grafito command matching this query",
        );
    }
    ToolResult::text(&call.id, true, catalog)
}

/// Construye el payload OpenAI con la lista de herramientas del agente.
fn build_agent_payload(
    settings: &ProviderSettings,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
) -> Result<Value, String> {
    settings.validate()?;
    let mut tool_json = Vec::with_capacity(tools.len());
    for tool in tools {
        tool_json.push(tool.openai_tool()?);
    }
    Ok(json!({
        "model": settings.model,
        "stream": false,
        "max_tokens": max_output_tokens,
        "messages": messages,
        "tools": tool_json,
    }))
}

/// Envía una petición agéntica y devuelve texto final o llamadas de herramienta.
fn request_agent_completion(
    settings: &ProviderSettings,
    api_key: Option<&str>,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
    timeout: Duration,
    cancellation: &Cancellation,
) -> Result<AgentChatResponse, String> {
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    if crate::remote_protocol(settings) != crate::RemoteProtocol::OpenAiChatCompletions {
        return Err("assistant agent requires an OpenAI-compatible chat endpoint".into());
    }
    let payload = build_agent_payload(settings, messages, tools, max_output_tokens)?;
    let client = crate::shared_http_client()?;
    let mut call = client
        .post(crate::chat_completion_endpoint(settings)?)
        .json(&payload)
        .timeout(timeout);
    if let Some(key) = api_key {
        if key.trim().is_empty() {
            return Err("assistant agent API key is unavailable".into());
        }
        call = call.bearer_auth(key);
    }
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    let response = call
        .send()
        .map_err(|_| "assistant agent request failed or timed out".to_string())?;
    if cancellation.is_cancelled() {
        return Err("assistant agent request was cancelled".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "assistant agent returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let response_bytes = crate::read_bounded_response_body(response, MAX_AGENT_RESPONSE_BYTES)?;
    let body: Value = serde_json::from_slice(&response_bytes)
        .map_err(|_| "assistant agent response JSON is invalid".to_string())?;
    parse_agent_completion(&body)
}

/// Parsea la primera elección de un completion agéntico (texto o tool_calls).
fn parse_agent_completion(body: &Value) -> Result<AgentChatResponse, String> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| "assistant agent response has no choices array".to_string())?;
    let choice = choices
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| "assistant agent response has no first choice".to_string())?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| "assistant agent response has no assistant message".to_string())?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("assistant agent response message is not an assistant message".into());
    }
    let calls = grafito_agent::schema::parse_tool_calls(&Value::Object(message.clone()));
    if let Ok(parsed) = calls {
        if !parsed.is_empty() {
            return Ok(AgentChatResponse::ToolCalls { calls: parsed });
        }
    }
    let content = message
        .get("content")
        .ok_or_else(|| "assistant agent response has no text content".to_string())?;
    let text = match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                let block = block.as_object().ok_or_else(|| {
                    "assistant agent response text block is malformed".to_string()
                })?;
                if block.get("type").and_then(Value::as_str) != Some("text") {
                    return Err("assistant agent response contains non-text blocks".into());
                }
                let block_text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "assistant agent response text block has no text".to_string())?;
                text.push_str(block_text);
            }
            if text.is_empty() {
                return Err("assistant agent response text block is empty".into());
            }
            Ok(text)
        }
        _ => Err("assistant agent response content is not displayable text".to_string()),
    }?;
    let truncated = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason == "length" || reason == "max_tokens");
    if text.trim().is_empty() {
        return Err("assistant agent response is empty".into());
    }
    Ok(AgentChatResponse::Text {
        content: text,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_payload_contains_tools_and_messages_without_secrets() {
        let settings = ProviderSettings::for_profile(crate::ProviderProfile::OllamaLocal, "local");
        let tool = ToolSchema::new(
            "evaluate_expr",
            "Evalúa una expresión.",
            json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
        );
        let payload = build_agent_payload(
            &settings,
            &[json!({"role": "user", "content": "hola"})],
            &[tool],
            256,
        )
        .unwrap();
        let serialized = payload.to_string();
        assert!(serialized.contains("\"tools\""));
        assert!(serialized.contains("evaluate_expr"));
        assert!(serialized.contains("\"role\":\"user\""));
        assert!(!serialized.contains("api_key"));
    }

    #[test]
    fn agent_completion_parses_text_and_tool_calls() {
        let text_body = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "final"}
            }]
        });
        match parse_agent_completion(&text_body).unwrap() {
            AgentChatResponse::Text { content, truncated } => {
                assert_eq!(content, "final");
                assert!(!truncated);
            }
            _ => panic!("expected text"),
        }

        let tool_body = json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "evaluate_expr", "arguments": "{\"expression\":\"2+2\"}"}
                    }]
                }
            }]
        });
        match parse_agent_completion(&tool_body).unwrap() {
            AgentChatResponse::ToolCalls { calls } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "evaluate_expr");
            }
            _ => panic!("expected tool calls"),
        }
    }

    #[test]
    fn safe_dispatcher_never_writes_documents_or_files() {
        let call = ToolCall {
            id: "call-e".into(),
            name: "evaluate_expr".into(),
            arguments: json!({"expression": "sin(0)", "variables": {}}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(result.ok);
        assert!(result.content.contains("0"));

        let unknown = ToolCall {
            id: "call-u".into(),
            name: "bash".into(),
            arguments: json!({"command": "rm -rf /"}),
        };
        let denied = dispatch_safe_tool(&unknown);
        assert!(!denied.ok);
        assert!(denied.content.contains("not available"));
    }

    #[test]
    fn ask_user_requires_explicit_consent_and_never_runs_silently() {
        let call = ToolCall {
            id: "call-a".into(),
            name: "ask_user".into(),
            arguments: json!({"question": "cuánto es x?"}),
        };
        let result = dispatch_safe_tool(&call);
        assert!(!result.ok);
        assert!(result.content.contains("explicit user answer"));
    }
}
