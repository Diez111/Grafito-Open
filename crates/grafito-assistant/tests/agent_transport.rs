#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integración del loop de agente contra un endpoint OpenAI-compatible simulado.

use grafito_agent::loop_engine::{AgentBudget, Cancellation};
use grafito_agent::schema::ToolSchema;
use grafito_assistant::agent::request_agent_on_worker;
use grafito_assistant::ProviderSettings;
use grafito_assistant_types::ProviderProfile;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

fn http_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{}",
        body.len(),
        body
    )
}

#[test]
fn agent_loop_evaluates_a_tool_and_converges_over_a_mock_provider() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();

    let tool_body = json!({
        "choices": [{
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-1",
                    "type": "function",
                    "function": {
                        "name": "evaluate_expr",
                        "arguments": serde_json::to_string(&json!({"expression": "sin(0)"})).unwrap(),
                    }
                }]
            }
        }]
    })
    .to_string();
    let final_body = json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": "resultado: 0"}
        }]
    })
    .to_string();

    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let mut buffer = [0; 16_384];
        let _ = first.read(&mut buffer);
        let request = String::from_utf8_lossy(&buffer[..]);
        assert!(
            request.contains(r#""tools""#),
            "the first request must advertise tools"
        );
        assert!(request.contains("evaluate_expr"));
        first
            .write_all(http_response(&tool_body).as_bytes())
            .unwrap();

        let (mut second, _) = listener.accept().unwrap();
        let _ = second.read(&mut buffer);
        // The second request carries the tool result back to the model.
        let loopback = String::from_utf8_lossy(&buffer[..]);
        assert!(loopback.contains(r#""role":"tool""#));
        assert!(loopback.contains("call-1"));
        second
            .write_all(http_response(&final_body).as_bytes())
            .unwrap();
    });

    let settings = ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local")
        .with_endpoint(format!("http://{address}/v1"))
        .expect("loopback endpoint is valid");
    let tools = vec![ToolSchema::new(
        "evaluate_expr",
        "Evaluates a mathematical expression with optional variables.",
        json!({"type": "object", "properties": {"expression": {"type": "string"}}}),
    )];
    let budget = AgentBudget {
        max_tool_turns: 2,
        per_turn_timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let (handle, _events) = request_agent_on_worker(
        settings,
        None,
        "You are a helpful math assistant.".to_owned(),
        vec![json!({"role": "user", "content": "calculá sin(0)"})],
        tools,
        budget,
        Cancellation::default(),
    );

    let outcome = handle.join().unwrap().expect("agent loop converges");
    server.join().unwrap();

    assert_eq!(outcome.final_text, "resultado: 0");
    assert_eq!(outcome.tool_turns, 1);
    assert!(!outcome.truncated);
}

#[test]
fn cancelled_agent_worker_exits_before_contacting_the_provider() {
    let cancellation = Cancellation::default();
    cancellation.cancel();

    let settings = ProviderSettings::for_profile(ProviderProfile::OllamaLocal, "local");
    let (handle, _events) = request_agent_on_worker(
        settings,
        None,
        "you are a helpful assistant".to_owned(),
        vec![json!({"role": "user", "content": "hola"})],
        Vec::new(),
        AgentBudget::default(),
        cancellation,
    );

    let error = handle.join().unwrap().expect_err("cancelled request fails");
    assert_eq!(error, "assistant agent request was cancelled");
}
