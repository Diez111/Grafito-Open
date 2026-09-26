//! Protocolo MCP sobre stdio (JSON-RPC 2.0, una línea por mensaje).
//!
//! Cubre lo que el Lab necesita, ni más ni menos:
//! `initialize`, `notifications/initialized`, `ping`, `tools/list`,
//! `tools/call`, `resources/list`, `resources/read`.
//! Los errores de herramienta van con `isError:true` (el LLM se autocorrige);
//! los errores de protocolo usan los códigos JSON-RPC estándar.

use crate::ledger;
use crate::tools;
use crate::{LabLimits, MCP_PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION};
use serde_json::{json, Value};

// ── Esquemas de tools (proyección del registro) ─────────────────────

fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "search_topp39",
            "description": "Corre una búsqueda reproducible TOPP 39 (familias seeded/grid/triangular/regular_polygon): genera, mide con índice espacial O(n) y devuelve el registro con hash re-verificado y guardado en el ledger. Solo el JSONL devuelto cuenta como evidencia.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "seed": {"type": "integer", "minimum": 0},
                    "n": {"type": "integer", "minimum": 1},
                    "family": {"type": "string", "enum": ["seeded", "grid", "triangular", "regular_polygon"]},
                    "scale": {"type": "number"},
                    "rows": {"type": "integer", "minimum": 1},
                    "cols": {"type": "integer", "minimum": 1},
                    "spacing": {"type": "number"},
                    "radius": {"type": "number"}
                },
                "required": ["seed", "n"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
        json!({
            "name": "export_dimacs",
            "description": "Exporta el grafo unit-distance de puntos a DIMACS CNF para k-coloración (verificación externa con kissat/cadical vía sat_check). Guarda el CNF por su sha256.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                    "k": {"type": "integer", "minimum": 1, "maximum": 16}
                },
                "required": ["points", "k"]
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "verify_search_run",
            "description": "Re-ejecuta un run del ledger por run_id y compara métricas (doble puerta anti-alucinación).",
            "inputSchema": {
                "type": "object",
                "properties": {"run_id": {"type": "string"}},
                "required": ["run_id"]
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "topp39_best_of",
            "description": "Barre seeds con los mismos (n, family, scale), re-verifica cada corrida y devuelve la mejor por pares unitarios. Las no verificadas se descartan sin romper el loop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "seeds": {"type": "array", "items": {"type": "integer", "minimum": 0}},
                    "n": {"type": "integer", "minimum": 1},
                    "family": {"type": "string"},
                    "scale": {"type": "number"}
                },
                "required": ["seeds", "n"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
        json!({
            "name": "sat_check",
            "description": "Corre kissat/cadical sobre un CNF guardado (por cnf_hash) con timeout. Error honesto si el solver no está instalado. 0 = sin timeout (explícito).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "cnf_hash": {"type": "string"},
                    "solver": {"type": "string", "enum": ["kissat", "cadical"]},
                    "timeout_ms": {"type": "integer", "minimum": 0}
                },
                "required": ["cnf_hash"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
        json!({
            "name": "chromatic_solve",
            "description": "Resuelve la k-coloración del grafo unit-distance de puntos con kissat/cadical y verifica el modelo arista por arista (model_checked). UNSAT = no k-coloreable según el solver (sin proof-checking).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                    "k": {"type": "integer", "minimum": 1, "maximum": 16},
                    "solver": {"type": "string", "enum": ["kissat", "cadical"]},
                    "timeout_ms": {"type": "integer", "minimum": 0}
                },
                "required": ["points", "k"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
        json!({
            "name": "verify_coloring",
            "description": "Doble puerta sin solver: verifica un coloreo candidato del grafo unit-distance arista por arista (valid, violations, first_violation).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                    "coloring": {"type": "array", "items": {"type": "integer", "minimum": 0}},
                    "k": {"type": "integer", "minimum": 1, "maximum": 16}
                },
                "required": ["points", "coloring"]
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "check_bounds",
            "description": "Cotas conocidas y topes efectivos del laboratorio (con guía honesta de lo que no escala).",
            "inputSchema": {
                "type": "object",
                "properties": {"problem": {"type": "string"}},
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "execute_command",
            "description": "Ejecuta de verdad 1 comando o una secuencia (hasta 32 pasos) sobre un documento efímero en memoria y devuelve por paso ok/message/error más el inventario final de etiquetas. Cubre los 650 comandos de Grafito con sus errores honestos. Sin I/O de archivos (imágenes/sonido/export/grabación se rechazan con guía).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Un comando, ej. Punto[(1, 2)] o A = (1, 2)"},
                    "steps": {"type": "array", "items": {"type": "string"}, "description": "Secuencia sobre el mismo doc efímero, ej. [A = (1, 2), Recta[A, B]]"}
                },
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
    ]
}

/// Todas las tools: 8 lab + execute + 37 proxedas + 2 Lean + 2 policy + 1 GPU + 2 Colab.
pub fn all_tool_defs() -> Vec<Value> {
    let mut defs = tool_defs();
    defs.extend(crate::bridge::proxied_tool_defs());
    defs.extend(crate::lean::lean_tool_defs());
    defs.extend(crate::policy::policy_tool_defs());
    defs.extend(crate::gpu::gpu_tool_defs());
    defs.extend(crate::colab::colab_tool_defs());
    defs
}

// ── Resources ────────────────────────────────────────────────────────

fn resource_defs() -> Vec<Value> {
    vec![
        json!({
            "uri": "grafito://ledger",
            "name": "ledger",
            "description": "JSONL append-only con todos los runs verificados",
            "mimeType": "application/jsonl"
        }),
        json!({
            "uriTemplate": "grafito://run/{run_id}",
            "name": "run",
            "description": "Metadatos de un run por run_id",
            "mimeType": "application/json"
        }),
        json!({
            "uriTemplate": "grafito://run/{run_id}/cnf/{k}",
            "name": "run-cnf",
            "description": "DIMACS regenerado del run (solo seeded/grid/triangular/polygon con params default)",
            "mimeType": "text/plain"
        }),
        json!({
            "uriTemplate": "grafito://run/{run_id}/result",
            "name": "run-result",
            "description": "Último resultado SAT asociado (si existe)",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "grafito://bounds/known",
            "name": "bounds",
            "description": "Cotas conocidas y topes efectivos",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "grafito://catalog/commands",
            "name": "catalog",
            "description": "Catálogo compacto de los 650 comandos (conteo + categorías + guía de búsqueda)",
            "mimeType": "application/json"
        }),
        json!({
            "uriTemplate": "lean://proofs/{proof_id}",
            "name": "lean-proof",
            "description": "Artefacto Lean verificado por proof_id (statement + proof + resultado del kernel)",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "policy://archive",
            "name": "policy-archive",
            "description": "Historial de políticas de exploración con su replay score (dream loop)",
            "mimeType": "application/json"
        }),
        json!({
            "uriTemplate": "colab://jobs/{job_id}",
            "name": "colab-job",
            "description": "Manifiesto de un job de offload a Colab (kind, params, tamaño del script)",
            "mimeType": "application/json"
        }),
    ]
}

fn read_resource(uri: &str, limits: &LabLimits) -> Result<Value, String> {
    if uri == "grafito://ledger" {
        let (all, corrupt) = ledger::read_all();
        let lines: Vec<String> = all.iter().take(10_000).map(|e| e.to_jsonl()).collect();
        return Ok(json!({
            "entries": lines.len(),
            "corrupt_skipped": corrupt,
            "tail": lines.into_iter().rev().take(200).rev().collect::<Vec<_>>(),
        }));
    }
    if uri == "grafito://bounds/known" {
        return tools::check_bounds(&json!({}), limits);
    }
    if uri == "policy://archive" {
        return Ok(crate::policy::read_policy_archive());
    }
    if let Some(job_id) = uri.strip_prefix("colab://jobs/") {
        return crate::colab::read_colab_job(job_id);
    }
    if let Some(proof_id) = uri.strip_prefix("lean://proofs/") {
        return crate::lean::read_lean_proof(proof_id);
    }
    if uri == "grafito://catalog/commands" {
        let mut cats: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        let mut total = 0usize;
        for spec in grafito_command::command_registry::all() {
            total += 1;
            *cats.entry(spec.category).or_insert(0) += 1;
        }
        return Ok(json!({
            "commands": total,
            "categories": cats,
            "note": "usá grafito_docs(query) para buscar comandos por tema y execute_command para correrlos de verdad sobre un doc efímero",
        }));
    }
    if let Some(rest) = uri.strip_prefix("grafito://run/") {
        // Formas: {run_id} | {run_id}/cnf/{k} | {run_id}/result
        if let Some((run_id, tail)) = rest.split_once('/') {
            if !crate::is_valid_run_id(run_id) {
                return Err("run_id inválido".into());
            }
            let entry = ledger::find_run(run_id).ok_or("run_id desconocido en el ledger")?;
            if tail == "result" {
                // Busca el CNF asociado re-derivando? El result vive por cnf_hash;
                // acá se devuelve guía honesta si no hay link directo.
                return Ok(json!({
                    "run_id": run_id,
                    "note": "el resultado SAT vive por cnf_hash (ver export_dimacs + sat_check); este run es geométrico puro",
                    "run": entry.to_jsonl(),
                }));
            }
            if let Some(kraw) = tail.strip_prefix("cnf/") {
                let k: usize = kraw.parse().map_err(|_| "k debe ser entero [1, 16]")?;
                if !(1..=16).contains(&k) {
                    return Err("k fuera de [1, 16]".into());
                }
                let family = crate::Family::parse(&entry.family)
                    .map_err(|e| format!("familia del run: {e}"))?;
                let pts = crate::generate_points(
                    crate::GenParams {
                        family,
                        n: entry.n,
                        seed: entry.seed,
                        scale: entry.scale,
                        rows: None,
                        cols: None,
                        spacing: None,
                        radius: None,
                    },
                    limits,
                )
                .map_err(|e| format!("no se pudo regenerar el run: {e}"))?;
                let edges = crate::unit_edges_spatial(&pts, 1e-9, limits.max_edges)
                    .map_err(|e| format!("aristas: {e}"))?;
                let cnf = grafito_geometry::search::export_dimacs_kcoloring(pts.len(), &edges, k)
                    .map_err(|e| e.to_string())?;
                return Ok(json!({"run_id": run_id, "k": k, "cnf": cnf}));
            }
            return Err(format!("recurso '{uri}' no reconocido; formas: run/{{id}}, run/{{id}}/cnf/{{k}}, run/{{id}}/result"));
        }
        // Solo run_id.
        if !crate::is_valid_run_id(rest) {
            return Err("run_id inválido".into());
        }
        let entry = ledger::find_run(rest).ok_or("run_id desconocido en el ledger")?;
        return Ok(json!({
            "run_id": entry.run_id,
            "family": entry.family,
            "seed": entry.seed,
            "n": entry.n,
            "scale": entry.scale,
            "unit": entry.unit,
            "distinct": entry.distinct,
            "hash": entry.hash,
            "ts": entry.ts,
            "jsonl": entry.to_jsonl(),
        }));
    }
    Err(format!("recurso '{uri}' desconocido"))
}

// ── Dispatch JSON-RPC ────────────────────────────────────────────────

/// Despacha un mensaje ya parseado. Devuelve `None` para notificaciones.
pub fn dispatch(msg: &Value, limits: &LabLimits) -> Option<Value> {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    // Notificaciones: sin respuesta.
    if method.starts_with("notifications/") {
        return None;
    }
    // Métodos que devuelven resultado directo.
    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": "Grafito Lab: el modelo solo propone números (seeds, n, k). El motor mide todo. Solo el JSONL con verified:true cuenta como evidencia. Prefijos epistémicos obligatorios: [CONJETURA]/[EVIDENCIA]/[PRUEBA]/[DESCARTADO].",
            }
        })),
        "ping" => Some(json!({"jsonrpc": "2.0", "id": id, "result": {}})),
        "tools/list" => {
            Some(json!({"jsonrpc": "2.0", "id": id, "result": {"tools": all_tool_defs()}}))
        }
        "resources/list" => {
            Some(json!({"jsonrpc": "2.0", "id": id, "result": {"resources": resource_defs()}}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args, limits) {
                Ok(payload) => Some(json!({
                    "jsonrpc": "2.0", "id": id, "result": {
                        "content": [{"type": "text", "text": payload.to_string()}],
                        "structuredContent": payload,
                    }
                })),
                Err(message) => Some(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "content": [{"type": "text", "text": message}],
                        "isError": true,
                    }
                })),
            }
        }
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
            match read_resource(uri, limits) {
                Ok(payload) => Some(json!({
                    "jsonrpc": "2.0", "id": id, "result": {
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": payload.to_string(),
                        }]
                    }
                })),
                Err(message) => Some(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "content": [{"type": "text", "text": message}],
                        "isError": true,
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0", "id": id,
            "error": {"code": -32601, "message": format!("método '{method}' no encontrado")}
        })),
    }
}

fn call_tool(name: &str, args: &Value, limits: &LabLimits) -> Result<Value, String> {
    match name {
        "search_topp39" => tools::search_topp39(args, limits),
        "export_dimacs" => tools::export_dimacs(args, limits),
        "verify_search_run" => tools::verify_search_run(args, limits),
        "topp39_best_of" => tools::topp39_best_of(args, limits),
        "sat_check" => tools::sat_check(args),
        "chromatic_solve" => tools::chromatic_solve(args, limits),
        "verify_coloring" => tools::verify_coloring(args, limits),
        "check_bounds" => tools::check_bounds(args, limits),
        "execute_command" => crate::bridge::execute_command(args),
        "lean_check" => crate::lean::lean_check(args),
        "lean_submit" => crate::lean::lean_submit(args),
        "policy_suggest" => crate::policy::policy_suggest(args, limits),
        "replay_score" => crate::policy::policy_replay(args, limits),
        "gpu_probe" => crate::gpu::gpu_probe(),
        "export_colab_job" => crate::colab::export_colab_job(args, limits),
        "import_colab_result" => crate::colab::import_colab_result(args, limits),
        _ if crate::bridge::is_proxied(name) => crate::bridge::dispatch_proxied(name, args),
        _ => Err(format!("tool '{name}' desconocida")),
    }
}

/// Error de protocolo (para parse errors / requests inválidos).
pub fn protocol_error(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_y_tools_list_responden() {
        let limits = LabLimits::default();
        let init = dispatch(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            &limits,
        )
        .unwrap();
        assert_eq!(
            init.get("result")
                .and_then(|r| r.get("serverInfo"))
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str()),
            Some("grafito-mcp")
        );
        let list = dispatch(
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
            &limits,
        )
        .unwrap();
        let tools = list
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(Value::as_array)
            .unwrap();
        // 8 lab + execute + 37 proxedas + 2 Lean + 2 policy + 1 GPU + 2 Colab.
        assert_eq!(tools.len(), 53);
        // Notificaciones no responden.
        assert!(dispatch(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &limits
        )
        .is_none());
        // Método desconocido → -32601.
        let bad = dispatch(
            &json!({"jsonrpc": "2.0", "id": 3, "method": "nope", "params": {}}),
            &limits,
        )
        .unwrap();
        assert_eq!(
            bad.get("error")
                .and_then(|e| e.get("code"))
                .and_then(Value::as_i64),
            Some(-32601)
        );
    }

    #[test]
    fn tool_desconocida_da_iserror_no_protocolo() {
        let limits = LabLimits::default();
        let r = dispatch(
            &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "inventada", "arguments": {}}}),
            &limits,
        )
        .unwrap();
        assert_eq!(
            r.get("result")
                .and_then(|x| x.get("isError"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(r.get("error").is_none());
    }
}
