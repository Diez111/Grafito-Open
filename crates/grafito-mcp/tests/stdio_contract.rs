//! Contrato MCP punta a punta a nivel `dispatch` (sin spawnear proceso).
//!
//! Usa un ledger temporal (`GRAFITO_LAB_LEDGER`) para no ensuciar el real:
//! initialize → tools/list (6) → search_topp39 → verify → export →
//! resources/read(ledger/run/bounds) → method desconocido (-32601).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_mcp::{protocol, LabLimits};
use serde_json::{json, Value};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn limits_small() -> LabLimits {
    LabLimits {
        max_points: 2000,
        max_edges: 200_000,
        max_dimacs_vars: 20_000,
        max_cnf_bytes: 512 * 1024,
        max_seeds: 4096,
    }
}

fn call(limits: &LabLimits, id: u64, name: &str, args: Value) -> Value {
    protocol::dispatch(
        &json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}}),
        limits,
    )
    .expect("tools/call siempre responde")
}

#[test]
fn contrato_completo_con_ledger_temporal() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join("grafito-mcp-contract");
    let _ = std::fs::create_dir_all(&dir);
    let ledger = dir.join(format!("lab-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&ledger);
    unsafe {
        std::env::set_var("GRAFITO_LAB_LEDGER", ledger.to_string_lossy().to_string());
    }
    let limits = limits_small();

    // initialize + lists
    let init = protocol::dispatch(
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        &limits,
    )
    .unwrap();
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        Value::String("grafito-mcp".into())
    );
    let tools = protocol::dispatch(
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        &limits,
    )
    .unwrap();
    // 8 lab + execute + 37 proxedas + 2 Lean + 2 policy + 1 GPU + 2 Colab.
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 53);
    let res = protocol::dispatch(
        &json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list", "params": {}}),
        &limits,
    )
    .unwrap();
    assert_eq!(res["result"]["resources"].as_array().unwrap().len(), 9);

    // search triangular n=12 → referencia unit=23 distinct=7
    let s = call(
        &limits,
        4,
        "search_topp39",
        json!({"seed": 42, "n": 12, "family": "triangular"}),
    );
    assert!(s["result"]["isError"].is_null());
    let sc = &s["result"]["structuredContent"];
    assert_eq!(sc["verified"], Value::Bool(true));
    assert_eq!(sc["unit"], json!(23));
    let run_id = sc["run_id"].as_str().unwrap().to_string();

    // verify OK
    let v = call(&limits, 5, "verify_search_run", json!({"run_id": run_id}));
    assert_eq!(v["result"]["structuredContent"]["ok"], Value::Bool(true));

    // best_of sobre 4 seeds
    let b = call(
        &limits,
        6,
        "topp39_best_of",
        json!({"seeds": [1, 2, 3, 4], "n": 10}),
    );
    assert!(
        b["result"]["structuredContent"]["verified_count"]
            .as_u64()
            .unwrap()
            >= 4
    );

    // export cuadrado k=2
    let e = call(
        &limits,
        7,
        "export_dimacs",
        json!({"points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], "k": 2}),
    );
    assert!(
        e["result"]["structuredContent"]["cnf_hash"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );

    // execute_command real: Punto + inventario.
    let x = call(
        &limits,
        20,
        "execute_command",
        json!({"command": "Punto[(1, 2)]"}),
    );
    assert_eq!(x["result"]["structuredContent"]["ok"], json!(true));
    // Proxeda: análisis real del CAS.
    let d = call(&limits, 21, "diff", json!({"expression": "x^2"}));
    assert!(d["result"]["isError"].is_null(), "diff falló: {d}");
    let ev = call(&limits, 22, "evaluate_expr", json!({"expression": "2+2"}));
    assert!(ev["result"]["isError"].is_null(), "evaluate falló: {ev}");

    // Lean denylist sin toolchain (sorry siempre se rechaza antes del kernel).
    let l = call(
        &limits,
        23,
        "lean_check",
        json!({"statement": "theorem x : 1 = 1", "proof": "by sorry"}),
    );
    assert_eq!(
        l["result"]["isError"],
        json!(true),
        "sorry debe rechazarse: {l}"
    );
    assert!(
        l.to_string().contains("sorry"),
        "el rechazo debe nombrar el token: {l}"
    );
    // Dream loop: suggest + replay sobre el ledger temporal.
    let ps = call(&limits, 24, "policy_suggest", json!({}));
    assert!(ps["result"]["structuredContent"]["recommended"].is_object());
    let rp = call(
        &limits,
        25,
        "replay_score",
        json!({"policies": [{"family": "seeded", "n": 8, "seeds": [1, 2]}]}),
    );
    assert!(rp["result"]["structuredContent"]["winner"] == json!(0));

    // Colab: export job + import simulado + recurso del job.
    let cj = call(
        &limits,
        27,
        "export_colab_job",
        json!({"kind": "unit_sweep", "params": {"family": "seeded", "n": 8, "seeds": [3]}}),
    );
    let cjob = cj["result"]["structuredContent"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(cjob.len(), 64);
    // PII se rechaza antes de empaquetar.
    let pii = call(
        &limits,
        28,
        "export_colab_job",
        json!({"kind": "cas_crosscheck", "params": {"expression": "a@b.com", "claim": "1", "check": "identity"}}),
    );
    assert_eq!(pii["result"]["isError"], json!(true));

    // resources/read ledger + run + bounds + catalog + policy archive + colab job
    for (id, uri) in [
        (8, "grafito://ledger".to_string()),
        (9, format!("grafito://run/{run_id}")),
        (10, "grafito://bounds/known".to_string()),
        (11, "grafito://catalog/commands".to_string()),
        (26, "policy://archive".to_string()),
        (29, format!("colab://jobs/{cjob}")),
    ] {
        let r = protocol::dispatch(
            &json!({"jsonrpc": "2.0", "id": id, "method": "resources/read",
                "params": {"uri": uri}}),
            &limits,
        )
        .unwrap();
        assert!(r["result"]["contents"].is_array(), "uri={uri}");
    }

    // método desconocido → -32601
    let bad = protocol::dispatch(
        &json!({"jsonrpc": "2.0", "id": 12, "method": "nope", "params": {}}),
        &limits,
    )
    .unwrap();
    assert_eq!(bad["error"]["code"], json!(-32601));

    let _ = std::fs::remove_file(&ledger);
    unsafe {
        std::env::remove_var("GRAFITO_LAB_LEDGER");
    }
}
