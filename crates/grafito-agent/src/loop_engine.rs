//! Loop de agente acotado y su contrato con proveedores y despachadores.

use crate::ledger::{JSpaceLedger, MAX_LEDGER_RENDER_BYTES};
use crate::schema::{ToolCall, ToolResult, ToolSchema};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Señal de cancelación cooperativa compartida por el loop y el transporte.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// Solicita que el loop abandone la ejecución cuanto antes.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Indica si se solicitó cancelación.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Presupuestos del loop del agente.
#[derive(Debug, Clone, Copy)]
pub struct AgentBudget {
    /// Máximo de turnos con llamadas de herramienta antes de converger.
    pub max_tool_turns: usize,
    /// Timeout por cada llamada al proveedor.
    pub per_turn_timeout: Duration,
    /// Tope de pared total para todo el loop.
    pub total_span: Duration,
    /// Máximo de caracteres de la respuesta final.
    pub max_output_chars: usize,
    /// Tope acumulado de caracteres (system + mensajes + resultados de tools).
    /// Evita que la conversación crezca sin control entre turnos.
    pub max_total_chars: usize,
    /// Reintentos de llamadas al proveedor con backoff (nunca de dispatches).
    pub max_retries: u32,
    /// Delay base del backoff entre reintentos (se duplica por intento).
    pub retry_base_delay_ms: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_tool_turns: 4,
            per_turn_timeout: Duration::from_secs(30),
            total_span: Duration::from_secs(120),
            max_output_chars: 8_192,
            max_total_chars: 48_000,
            max_retries: 2,
            retry_base_delay_ms: 200,
        }
    }
}

/// Respuesta de un proveedor de chat que soporta tool calling.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentChatResponse {
    /// Respuesta final de texto sin llamadas de herramienta.
    Text { content: String, truncated: bool },
    /// El modelo pidió invocar una o más herramientas.
    ToolCalls { calls: Vec<ToolCall> },
}

/// Contrato que un transporte de chat (OpenAI-compatible u otro) implementa.
pub trait AgentCompleter {
    /// Envía los mensajes en forma OpenAI-compatible y devuelve texto o llamadas.
    fn complete(
        &self,
        messages: &[Value],
        tools: &[ToolSchema],
        max_output_tokens: usize,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> Result<AgentChatResponse, String>;
}

/// Despacha una llamada de herramienta a su ejecutor local.
pub trait ToolDispatcher {
    /// Ejecuta la herramienta y devuelve un resultado acotado para el modelo.
    fn dispatch(&self, call: &ToolCall) -> ToolResult;
}

/// Eventos de actividad del loop, aptos para mostrar en la UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// Empezó la ejecución de una herramienta.
    ToolStarted { name: String, args_summary: String },
    /// Terminó la ejecución de una herramienta.
    ToolFinished { name: String, ok: bool },
    /// El agente produjo su respuesta final.
    Finalized { text: String },
    /// Estado de tarea (ledger J-Space) al comienzo de la ejecución.
    Ledger { render: String },
}

/// Resultado terminal del loop del agente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOutcome {
    /// Texto final entregado al usuario.
    pub final_text: String,
    /// La respuesta superó el presupuesto de caracteres del final.
    pub truncated: bool,
    /// Cantidad de turnos con herramientas consumidos.
    pub tool_turns: usize,
    /// Done-check: la respuesta final no deja pendientes declarados y es verificable.
    pub verified: bool,
}

const MAX_TOOL_ARGS_SUMMARY_CHARS: usize = 160;

fn summarize_args(arguments: &Value) -> String {
    let summary = arguments.to_string();
    if summary.chars().count() > MAX_TOOL_ARGS_SUMMARY_CHARS {
        let mut clipped = summary
            .chars()
            .take(MAX_TOOL_ARGS_SUMMARY_CHARS.saturating_sub(1))
            .collect::<String>();
        clipped.push('…');
        clipped
    } else {
        summary
    }
}

/// Ejecuta el loop de agente acotado, sin ledger de tarea.
#[allow(clippy::too_many_arguments)]
pub fn run_agent<C, D>(
    completer: &C,
    dispatcher: &D,
    system: &str,
    initial_user_messages: &[Value],
    tools: &[ToolSchema],
    budget: &AgentBudget,
    cancellation: &Cancellation,
    on_event: impl FnMut(AgentEvent),
) -> Result<AgentOutcome, String>
where
    C: AgentCompleter,
    D: ToolDispatcher,
{
    run_agent_with_ledger(
        completer,
        dispatcher,
        system,
        initial_user_messages,
        tools,
        budget,
        None,
        cancellation,
        on_event,
    )
}

/// Variante que inyecta un ledger J-Space (Goal/Core/Verified/Open/Next).
/// El ledger se clona y evoluciona por turno: cada resultado de herramienta se
/// registra (`Verified`/`Open`) y se re-emite como evento `Ledger` para la UI.
/// El done-check final es real: sin pendientes abiertos y sin tools fallidas.
#[allow(clippy::too_many_arguments)]
pub fn run_agent_with_ledger<C, D>(
    completer: &C,
    dispatcher: &D,
    system: &str,
    initial_user_messages: &[Value],
    tools: &[ToolSchema],
    budget: &AgentBudget,
    ledger: Option<&JSpaceLedger>,
    cancellation: &Cancellation,
    mut on_event: impl FnMut(AgentEvent),
) -> Result<AgentOutcome, String>
where
    C: AgentCompleter,
    D: ToolDispatcher,
{
    let started = Instant::now();
    let mut system_owned = system.to_owned();
    // Estado vivo del ledger: clon acotado que evoluciona con cada turno.
    let mut tracked: Option<JSpaceLedger> = ledger.cloned();
    if let Some(tracked) = tracked.as_ref() {
        tracked.validate()?;
        emit_ledger(&mut on_event, tracked);
        let render = tracked.render_bounded(MAX_LEDGER_RENDER_BYTES);
        if !render.trim().is_empty() {
            system_owned.push_str("\n\nLedger de tarea:\n");
            system_owned.push_str(&render);
        }
    }
    let mut messages = Vec::with_capacity(initial_user_messages.len() + 16);
    if !system_owned.trim().is_empty() {
        messages.push(json!({"role": "system", "content": system_owned}));
    }
    messages.extend_from_slice(initial_user_messages);
    for tool in tools {
        tool.validate()?;
    }
    let mut accumulated_chars: usize = messages.iter().map(message_chars).sum();
    let mut all_tools_ok = true;

    let max_turns = budget.max_tool_turns.max(1);
    for turn in 0..=max_turns {
        if cancellation.is_cancelled() {
            return Err("assistant agent request was cancelled".into());
        }
        let remaining = budget.total_span.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err("assistant agent loop exceeded its total span".into());
        }
        if accumulated_chars > budget.max_total_chars {
            return Err("assistant agent loop exceeded its total char budget".into());
        }
        let per_turn_timeout = budget.per_turn_timeout.min(remaining);
        let response = complete_with_retries(
            completer,
            &messages,
            tools,
            completion_token_budget(budget.max_output_chars),
            per_turn_timeout,
            cancellation,
            budget,
        )?;
        match response {
            AgentChatResponse::Text { content, truncated } => {
                on_event(AgentEvent::Finalized {
                    text: content.clone(),
                });
                let lower = content.to_ascii_lowercase();
                let text_ok = !lower.contains("pendiente")
                    && !lower.contains("sin verificar")
                    && !lower.contains("no pude");
                let ledger_ok = tracked
                    .as_ref()
                    .is_none_or(|tracked| !tracked.has_open_items());
                let verified = text_ok && all_tools_ok && ledger_ok;
                return Ok(AgentOutcome {
                    final_text: content,
                    truncated,
                    tool_turns: turn,
                    verified,
                });
            }
            AgentChatResponse::ToolCalls { calls } => {
                if calls.is_empty() {
                    return Err("assistant agent received an empty tool_calls list".into());
                }
                if turn == max_turns {
                    return Err("assistant agent exceeded its tool-call turn budget".into());
                }
                let tool_calls_json = calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": serde_json::to_string(&call.arguments)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            }
                        })
                    })
                    .collect::<Vec<Value>>();
                let assistant_msg = json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": tool_calls_json,
                });
                accumulated_chars += message_chars(&assistant_msg);
                messages.push(assistant_msg);
                for call in &calls {
                    on_event(AgentEvent::ToolStarted {
                        name: call.name.clone(),
                        args_summary: summarize_args(&call.arguments),
                    });
                    if cancellation.is_cancelled() {
                        return Err("assistant agent request was cancelled".into());
                    }
                    let result = dispatcher.dispatch(call);
                    on_event(AgentEvent::ToolFinished {
                        name: call.name.clone(),
                        ok: result.ok,
                    });
                    all_tools_ok &= result.ok;
                    if let Some(tracked) = tracked.as_mut() {
                        tracked.record_tool_outcome(&call.name, result.ok, &result.content);
                        emit_ledger(&mut on_event, tracked);
                    }
                    let tool_msg = json!({
                        "role": "tool",
                        "tool_call_id": result.call_id,
                        "content": result.content,
                    });
                    accumulated_chars += message_chars(&tool_msg);
                    messages.push(tool_msg);
                }
            }
        }
    }
    Err("assistant agent loop did not converge".into())
}

/// Re-emite el ledger acotado para la UI (misma variante `Ledger`, sin romper matches).
fn emit_ledger(on_event: &mut impl FnMut(AgentEvent), tracked: &JSpaceLedger) {
    let render = tracked.render_bounded(MAX_LEDGER_RENDER_BYTES);
    if !render.trim().is_empty() {
        on_event(AgentEvent::Ledger {
            render: render.clone(),
        });
    }
}

/// Llama al proveedor con reintentos y backoff (solo transporte, nunca dispatches).
/// No reintenta si hay cancelación ni cuando se agotan los intentos.
fn complete_with_retries<C>(
    completer: &C,
    messages: &[Value],
    tools: &[ToolSchema],
    max_output_tokens: usize,
    timeout: Duration,
    cancellation: &Cancellation,
    budget: &AgentBudget,
) -> Result<AgentChatResponse, String>
where
    C: AgentCompleter,
{
    let mut attempt = 0u32;
    loop {
        match completer.complete(messages, tools, max_output_tokens, timeout, cancellation) {
            Ok(response) => return Ok(response),
            Err(error) => {
                if cancellation.is_cancelled() || attempt >= budget.max_retries {
                    return Err(error);
                }
                attempt += 1;
                let backoff_ms = budget
                    .retry_base_delay_ms
                    .saturating_mul(1u64 << attempt.min(6));
                std::thread::sleep(Duration::from_millis(backoff_ms));
            }
        }
    }
}

fn message_chars(message: &Value) -> usize {
    message.to_string().len()
}

fn completion_token_budget(max_output_chars: usize) -> usize {
    (max_output_chars / 4).clamp(1, 8_192)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ToolSchema;
    use std::sync::Mutex;

    fn tools() -> Vec<ToolSchema> {
        vec![ToolSchema {
            name: "echo".into(),
            description: "Devuelve el argumento.".into(),
            parameters: json!({"type": "object", "properties": {"value": {"type": "string"}}}),
            needs_consent: false,
        }]
    }

    struct ScriptedCompleter {
        responses: Mutex<Vec<AgentChatResponse>>,
    }

    impl AgentCompleter for ScriptedCompleter {
        fn complete(
            &self,
            _messages: &[Value],
            _tools: &[ToolSchema],
            _max_output_tokens: usize,
            _timeout: Duration,
            _cancellation: &Cancellation,
        ) -> Result<AgentChatResponse, String> {
            let mut guard = self.responses.lock().unwrap_or_else(|p| {
                log::warn!("lock poisoned");
                p.into_inner()
            });
            if guard.is_empty() {
                return Err("scripted completer ran out of responses".into());
            }
            Ok(guard.remove(0))
        }
    }

    struct EchoDispatcher;

    impl ToolDispatcher for EchoDispatcher {
        fn dispatch(&self, call: &ToolCall) -> ToolResult {
            let value = call
                .arguments
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("");
            ToolResult::text(&call.id, true, format!("echo:{value}"))
        }
    }

    fn user_message(text: &str) -> Value {
        json!({"role": "user", "content": text})
    }

    #[test]
    fn agent_loop_converges_after_one_tool_call() {
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-1".into(),
                        name: "echo".into(),
                        arguments: json!({}),
                    }],
                },
                AgentChatResponse::Text {
                    content: "resultado".into(),
                    truncated: false,
                },
            ]),
        };
        let mut events = Vec::new();
        let outcome = run_agent(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            &Cancellation::default(),
            |event| events.push(event),
        )
        .unwrap();
        assert_eq!(outcome.final_text, "resultado");
        assert_eq!(outcome.tool_turns, 1);
        assert!(!outcome.truncated);
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolStarted { name, .. } if name == "echo"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolFinished { name, ok: true } if name == "echo"
        )));
    }

    #[test]
    fn agent_loop_stops_at_the_tool_turn_budget() {
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-a".into(),
                        name: "echo".into(),
                        arguments: json!({}),
                    }],
                },
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-b".into(),
                        name: "echo".into(),
                        arguments: json!({}),
                    }],
                },
            ]),
        };
        let budget = AgentBudget {
            max_tool_turns: 1,
            ..Default::default()
        };
        let result = run_agent(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &budget,
            &Cancellation::default(),
            |_| {},
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("tool-call turn budget"));
    }

    #[test]
    fn cancelled_agent_exits_before_any_provider_call() {
        let completer = ScriptedCompleter {
            responses: Mutex::new(Vec::new()),
        };
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let result = run_agent(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            &cancellation,
            |_| {},
        );
        assert_eq!(result.unwrap_err(), "assistant agent request was cancelled");
    }

    #[test]
    fn ledger_is_emitted_and_injected_into_the_system_prompt() {
        #[derive(Default)]
        struct CaptureCompleter {
            captured: Mutex<Option<String>>,
        }
        impl AgentCompleter for CaptureCompleter {
            fn complete(
                &self,
                messages: &[Value],
                _tools: &[ToolSchema],
                _max_output_tokens: usize,
                _timeout: Duration,
                _cancellation: &Cancellation,
            ) -> Result<AgentChatResponse, String> {
                let system = messages
                    .iter()
                    .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
                    .and_then(|message| message.get("content").and_then(Value::as_str))
                    .unwrap_or("")
                    .to_owned();
                *self.captured.lock().unwrap_or_else(|p| {
                    log::warn!("lock poisoned");
                    p.into_inner()
                }) = Some(system);
                Ok(AgentChatResponse::Text {
                    content: "respuesta".into(),
                    truncated: false,
                })
            }
        }
        let completer = CaptureCompleter::default();
        let ledger = crate::ledger::JSpaceLedger::with_task("analizar f", "evaluar g");
        let outcome = run_agent_with_ledger(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            Some(&ledger),
            &Cancellation::default(),
            |_| {},
        )
        .unwrap();
        assert!(outcome.verified);
        let system = completer
            .captured
            .lock()
            .unwrap_or_else(|p| {
                log::warn!("lock poisoned");
                p.into_inner()
            })
            .clone()
            .unwrap();
        assert!(system.contains("Ledger de tarea:"));
        assert!(system.contains("Goal: analizar f"));
        assert!(system.contains("Next: evaluar g"));
    }

    #[test]
    fn failing_tool_results_are_forwarded_without_aborting_the_loop() {
        struct FailingDispatcher;
        impl ToolDispatcher for FailingDispatcher {
            fn dispatch(&self, call: &ToolCall) -> ToolResult {
                ToolResult::text(&call.id, false, "tool did not converge")
            }
        }
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-x".into(),
                        name: "echo".into(),
                        arguments: json!({}),
                    }],
                },
                AgentChatResponse::Text {
                    content: "recuperado".into(),
                    truncated: false,
                },
            ]),
        };
        let outcome = run_agent(
            &completer,
            &FailingDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            &Cancellation::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(outcome.final_text, "recuperado");
    }

    #[test]
    fn tracked_ledger_records_outcomes_and_reemits_progress() {
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-1".into(),
                        name: "echo".into(),
                        arguments: json!({"value": "x"}),
                    }],
                },
                AgentChatResponse::Text {
                    content: "listo".into(),
                    truncated: false,
                },
            ]),
        };
        let ledger = crate::ledger::JSpaceLedger::with_task("probar", "cerrar");
        let mut events = Vec::new();
        let outcome = run_agent_with_ledger(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            Some(&ledger),
            &Cancellation::default(),
            |event| events.push(event),
        )
        .unwrap();
        assert!(outcome.verified);
        let ledger_renders: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Ledger { render } => Some(render.clone()),
                _ => None,
            })
            .collect();
        // Inicial + actualización por turno con la tool registrada.
        assert!(ledger_renders.len() >= 2);
        assert!(ledger_renders
            .last()
            .is_some_and(|render| render.contains("Verified: echo")));
    }

    #[test]
    fn tracked_ledger_marks_unverified_when_tools_fail() {
        struct FailingDispatcher;
        impl ToolDispatcher for FailingDispatcher {
            fn dispatch(&self, call: &ToolCall) -> ToolResult {
                ToolResult::text(&call.id, false, "boom")
            }
        }
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![
                AgentChatResponse::ToolCalls {
                    calls: vec![ToolCall {
                        id: "call-x".into(),
                        name: "echo".into(),
                        arguments: json!({}),
                    }],
                },
                AgentChatResponse::Text {
                    content: "recuperado".into(),
                    truncated: false,
                },
            ]),
        };
        let ledger = crate::ledger::JSpaceLedger::with_task("probar", "cerrar");
        let outcome = run_agent_with_ledger(
            &completer,
            &FailingDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &AgentBudget::default(),
            Some(&ledger),
            &Cancellation::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(outcome.final_text, "recuperado");
        // Hay un item abierto y una tool fallida: el done-check es real.
        assert!(!outcome.verified);
    }

    #[test]
    fn loop_aborts_when_total_char_budget_is_exceeded() {
        let completer = ScriptedCompleter {
            responses: Mutex::new(vec![AgentChatResponse::Text {
                content: "x".into(),
                truncated: false,
            }]),
        };
        let budget = AgentBudget {
            max_total_chars: 10,
            ..Default::default()
        };
        let result = run_agent(
            &completer,
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &budget,
            &Cancellation::default(),
            |_| {},
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("total char budget"));
    }

    #[test]
    fn loop_retries_transient_provider_errors_with_backoff() {
        struct FlakyCompleter {
            failures_left: Mutex<u32>,
        }
        impl AgentCompleter for FlakyCompleter {
            fn complete(
                &self,
                _messages: &[Value],
                _tools: &[ToolSchema],
                _max_output_tokens: usize,
                _timeout: Duration,
                _cancellation: &Cancellation,
            ) -> Result<AgentChatResponse, String> {
                let mut guard = self.failures_left.lock().unwrap_or_else(|p| {
                    log::warn!("lock poisoned");
                    p.into_inner()
                });
                if *guard > 0 {
                    *guard -= 1;
                    return Err("transient 503".into());
                }
                Ok(AgentChatResponse::Text {
                    content: "al segundo intento".into(),
                    truncated: false,
                })
            }
        }
        let budget = AgentBudget {
            max_retries: 2,
            retry_base_delay_ms: 1,
            ..Default::default()
        };
        let outcome = run_agent(
            &FlakyCompleter {
                failures_left: Mutex::new(1),
            },
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &budget,
            &Cancellation::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(outcome.final_text, "al segundo intento");

        let no_retry = AgentBudget {
            max_retries: 0,
            ..Default::default()
        };
        let result = run_agent(
            &FlakyCompleter {
                failures_left: Mutex::new(1),
            },
            &EchoDispatcher,
            "system",
            &[user_message("hola")],
            &tools(),
            &no_retry,
            &Cancellation::default(),
            |_| {},
        );
        assert!(result.is_err());
    }
}
