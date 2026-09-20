//! Dream-loop de Dream-RSI nativo (`policy.rs`).
//!
//! La política de exploración es código+datos versionados, los modelos
//! quedan fijos. Este módulo es puro cómputo determinista: lee el ledger
//! verificado, sugiere la próxima familia y re-ejecuta políticas candidatas
//! sin ninguna llamada LLM.
//!
//! Solo el server escribe (vía [`archive_policy`], llamada desde
//! [`policy_replay`]); el LLM jamás toca disco.

use crate::{
    distinct_count_bounded, generate_points, Family, GenParams, LabLimits, MAX_DISTINCT_N,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Seeds por defecto para replay (deterministas, recortables al largo pedido).
const DEFAULT_REPLAY_SEEDS: [u64; 4] = [7, 42, 99, 1234];

/// Nota honesta cuando el ledger está vacío.
const EMPTY_NOTE: &str = "sin historia: exploración inicial";
/// Nota obligatoria del replay (no promete generalización).
const REPLAY_NOTE: &str =
    "replay sobre historial determinista; no garantiza generalización fuera de lo visitado";

// ── Esquemas de tools ────────────────────────────────────────────────

/// Definiciones MCP de las 2 tools de política (el wire vive en otro agente).
pub fn policy_tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "policy_suggest",
            "description": "Sugiere la próxima política de exploración (familia, n, escala) a partir del historial verificado del ledger. Puro cómputo, sin LLM.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "problem": {"type": "string"}
                },
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "replay_score",
            "description": "Re-ejecuta políticas candidatas de forma determinista (generate_points + unit_pairs_spatial) y las puntúa por media de pares unitarios. Archiva la ganadora. Puro cómputo, cero ejecuciones LLM.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "policies": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 16,
                        "items": {
                            "type": "object",
                            "properties": {
                                "family": {"type": "string"},
                                "n": {"type": "integer", "minimum": 1},
                                "scale": {"type": "number"},
                                "seeds": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 1, "maxItems": 64}
                            },
                            "required": ["family", "n"]
                        }
                    },
                    "lab": {"type": "string"}
                },
                "required": ["policies"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
    ]
}

// ── policy_suggest ───────────────────────────────────────────────────

/// `policy_suggest({problem?})`: agrega el ledger por familia y recomienda.
pub fn policy_suggest(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let _ = limits;
    let problem = args
        .get("problem")
        .and_then(Value::as_str)
        .unwrap_or("topp39");
    let (entries, corrupt) = crate::ledger::read_all();
    if entries.is_empty() {
        return Ok(json!({
            "tool": "policy_suggest",
            "problem": problem,
            "ranking": [],
            "recommended": {"family": "seeded", "n": 12, "scale": 5.0},
            "rationale": "sin historia: exploración inicial con seeded n=12 escala 5.0; el azar puro da unit≈0, las estructuradas (triangular/grid) suelen dominar",
            "entries": 0,
            "corrupt_skipped": corrupt,
            "note": EMPTY_NOTE,
        }));
    }
    #[derive(Debug, Clone)]
    struct Agg {
        runs: usize,
        best_unit: usize,
        sum_unit: u64,
        best_n: usize,
        best_scale: f64,
    }
    let mut agg: BTreeMap<String, Agg> = BTreeMap::new();
    for e in &entries {
        let slot = agg.entry(e.family.clone()).or_insert(Agg {
            runs: 0,
            best_unit: 0,
            sum_unit: 0,
            best_n: e.n,
            best_scale: e.scale,
        });
        slot.runs += 1;
        slot.sum_unit += e.unit as u64;
        if e.unit > slot.best_unit {
            slot.best_unit = e.unit;
            slot.best_n = e.n;
            slot.best_scale = e.scale;
        }
    }
    let mut ranking: Vec<Value> = agg
        .iter()
        .map(|(family, a)| {
            let mean = if a.runs == 0 {
                0.0
            } else {
                a.sum_unit as f64 / a.runs as f64
            };
            json!({
                "family": family,
                "runs": a.runs,
                "best_unit": a.best_unit,
                "mean_unit": mean,
            })
        })
        .collect();
    ranking.sort_by(|a, b| {
        let ba = a.get("best_unit").and_then(Value::as_u64).unwrap_or(0);
        let bb = b.get("best_unit").and_then(Value::as_u64).unwrap_or(0);
        bb.cmp(&ba).then_with(|| {
            let ma = a.get("mean_unit").and_then(Value::as_f64).unwrap_or(0.0);
            let mb = b.get("mean_unit").and_then(Value::as_f64).unwrap_or(0.0);
            mb.partial_cmp(&ma).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let top_family = ranking
        .first()
        .and_then(|r| r.get("family"))
        .and_then(Value::as_str)
        .unwrap_or("seeded")
        .to_string();
    let top_agg = agg.get(&top_family);
    let (rec_n, rec_scale, rec_best, rec_runs) = match top_agg {
        Some(a) => (a.best_n, a.best_scale, a.best_unit, a.runs),
        None => (12usize, 5.0f64, 0usize, 0usize),
    };
    let mean_top = ranking
        .first()
        .and_then(|r| r.get("mean_unit"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let rationale = format!(
        "la familia '{top_family}' lidera con best_unit={rec_best} en {rec_runs} runs (media {mean_top:.1}); se recomienda repetirla con n={rec_n} escala={rec_scale}"
    );
    Ok(json!({
        "tool": "policy_suggest",
        "problem": problem,
        "ranking": ranking,
        "recommended": {"family": top_family, "n": rec_n, "scale": rec_scale},
        "rationale": rationale,
        "entries": entries.len(),
        "corrupt_skipped": corrupt,
        "note": "agregado sobre el ledger verificado; el texto del modelo no es evidencia",
    }))
}

// ── policy_replay ────────────────────────────────────────────────────

fn parse_replay_seeds(policy: &Value, idx: usize) -> Result<Vec<u64>, String> {
    match policy.get("seeds") {
        None => Ok(DEFAULT_REPLAY_SEEDS.to_vec()),
        Some(Value::Array(raw)) => {
            if raw.is_empty() || raw.len() > 64 {
                return Err(format!(
                    "replay_score: policies[{idx}].seeds fuera de [1, 64] (llegaron {})",
                    raw.len()
                ));
            }
            let mut out = Vec::with_capacity(raw.len());
            for (j, v) in raw.iter().enumerate() {
                let s = v.as_u64().ok_or(format!(
                    "replay_score: policies[{idx}].seeds[{j}] debe ser entero >= 0"
                ))?;
                out.push(s);
            }
            Ok(out)
        }
        // Forma compacta: {"seeds": 2} = recorte de las defaults al largo pedido.
        Some(v) => {
            let want = v.as_u64().ok_or(format!(
                "replay_score: policies[{idx}].seeds debe ser arreglo o conteo 1..=64"
            ))?;
            if want == 0 || want > 64 {
                return Err(format!(
                    "replay_score: policies[{idx}].seeds fuera de [1, 64] (llegó {want})"
                ));
            }
            let want = want as usize;
            if want <= DEFAULT_REPLAY_SEEDS.len() {
                Ok(DEFAULT_REPLAY_SEEDS[..want].to_vec())
            } else {
                // Extensión determinista más allá de las 4 defaults.
                let mut out = DEFAULT_REPLAY_SEEDS.to_vec();
                let mut k: u64 = 10_000;
                while out.len() < want {
                    out.push(k);
                    k += 101;
                }
                Ok(out)
            }
        }
    }
}

/// `replay_score({policies, lab?})`: regenera y puntúa; archiva la ganadora.
pub fn policy_replay(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let arr = args
        .get("policies")
        .and_then(Value::as_array)
        .ok_or("replay_score: 'policies' debe ser un arreglo no vacío (1..=16)")?;
    if arr.is_empty() || arr.len() > 16 {
        return Err(format!(
            "replay_score: 'policies' fuera de [1, 16] (llegaron {})",
            arr.len()
        ));
    }
    let lab = args.get("lab").and_then(Value::as_str).unwrap_or("topp39");
    let mut results: Vec<Value> = Vec::with_capacity(arr.len());
    let mut best_idx: usize = 0;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_max = 0usize;
    for (idx, p) in arr.iter().enumerate() {
        if !p.is_object() {
            return Err(format!("replay_score: policies[{idx}] debe ser objeto"));
        }
        let family_raw = p.get("family").and_then(Value::as_str).unwrap_or("seeded");
        let family =
            Family::parse(family_raw).map_err(|e| format!("replay_score: policies[{idx}]: {e}"))?;
        let n_raw = p.get("n").and_then(Value::as_u64).ok_or(format!(
            "replay_score: policies[{idx}].n requerido (entero >= 1)"
        ))?;
        if n_raw == 0 || n_raw > limits.max_points as u64 {
            return Err(format!(
                "replay_score: policies[{idx}].n fuera de [1, {}]",
                limits.max_points
            ));
        }
        let n = n_raw as usize;
        let scale = match p.get("scale") {
            None => 5.0,
            Some(v) => v.as_f64().ok_or(format!(
                "replay_score: policies[{idx}].scale debe ser número finito"
            ))?,
        };
        if !scale.is_finite() || scale <= 0.0 || scale > 1e6 {
            return Err(format!(
                "replay_score: policies[{idx}].scale fuera de (0, 1e6]"
            ));
        }
        let seeds = parse_replay_seeds(p, idx)?;
        let mut run_objs: Vec<Value> = Vec::with_capacity(seeds.len());
        let mut sum_unit: u64 = 0;
        let mut max_unit: usize = 0;
        for seed in &seeds {
            let pts = generate_points(
                GenParams {
                    family,
                    n,
                    seed: *seed,
                    scale,
                    rows: None,
                    cols: None,
                    spacing: None,
                    radius: None,
                },
                limits,
            )
            .map_err(|e| format!("replay_score: policies[{idx}] seed {seed}: {e}"))?;
            let unit = crate::unit_pairs_spatial(&pts, 1e-9)
                .map_err(|e| format!("replay_score: policies[{idx}] seed {seed}: {e}"))?;
            let (distinct, capped) = if pts.len() > MAX_DISTINCT_N {
                (-1i64, true)
            } else {
                match distinct_count_bounded(&pts, 1e-9) {
                    Ok(d) => (d as i64, false),
                    Err(e) => {
                        return Err(format!("replay_score: policies[{idx}] seed {seed}: {e}"));
                    }
                }
            };
            sum_unit += unit as u64;
            if unit > max_unit {
                max_unit = unit;
            }
            run_objs.push(json!({
                "seed": seed,
                "unit": unit,
                "distinct": distinct,
                "distinct_capped": capped,
            }));
        }
        let mean_unit = if seeds.is_empty() {
            0.0
        } else {
            sum_unit as f64 / seeds.len() as f64
        };
        let score = mean_unit;
        results.push(json!({
            "policy": p,
            "runs": run_objs,
            "mean_unit": mean_unit,
            "max_unit": max_unit,
            "score": score,
        }));
        let better = if idx == 0 {
            true
        } else if (score - best_score).abs() <= f64::EPSILON {
            max_unit > best_max
        } else {
            score > best_score
        };
        if better {
            best_idx = idx;
            best_score = score;
            best_max = max_unit;
        }
    }
    // La ganadora se archiva automáticamente (solo el server escribe).
    if let Some(winner_policy) = arr.get(best_idx) {
        archive_policy(winner_policy, best_score)
            .map_err(|e| format!("replay_score: no se pudo archivar la ganadora: {e}"))?;
    }
    Ok(json!({
        "tool": "replay_score",
        "lab": lab,
        "results": results,
        "winner": best_idx,
        "winner_score": best_score,
        "note": REPLAY_NOTE,
    }))
}

// ── Archivo de políticas ─────────────────────────────────────────────

fn policy_archive_path() -> PathBuf {
    let ledger = crate::ledger::ledger_path();
    match ledger.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join("lab_policies.jsonl"),
        _ => PathBuf::from("lab_policies.jsonl"),
    }
}

/// Lee `lab_policies.jsonl` (dir hermano del ledger) para `policy://archive`.
pub fn read_policy_archive() -> Value {
    let path = policy_archive_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            return json!({
                "policies": [],
                "count": 0,
                "note": "sin archivo de políticas; aún no hay ganadoras archivadas",
            });
        }
    };
    let mut out = Vec::new();
    let mut corrupt = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) => out.push(v),
            Err(_) => corrupt += 1,
        }
    }
    let count = out.len();
    json!({
        "policies": out,
        "count": count,
        "corrupt_skipped": corrupt,
        "note": "archivo versionado de políticas ganadoras (código+datos); los modelos quedan fijos",
    })
}

/// Agrega {policy, score, ts} al archivo. Solo el server la llama.
pub fn archive_policy(policy: &Value, score: f64) -> Result<(), String> {
    if !score.is_finite() {
        return Err("policy: 'score' debe ser finito".into());
    }
    if !policy.is_object() {
        return Err("policy: la política debe ser un objeto {family, n, ...}".into());
    }
    let path = policy_archive_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("policy: no se pudo crear {parent:?}: {e}"))?;
        }
    }
    let entry = json!({
        "policy": policy,
        "score": score,
        "ts": crate::ledger::now_epoch_secs(),
    });
    let line =
        serde_json::to_string(&entry).map_err(|e| format!("policy: no se pudo serializar: {e}"))?;
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("policy: no se pudo abrir {path:?}: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("policy: no se pudo escribir: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ledger::LabEntry;
    use serde_json::json;

    fn test_limits() -> LabLimits {
        LabLimits {
            max_points: 2000,
            max_edges: 200_000,
            max_dimacs_vars: 20_000,
            max_cnf_bytes: 512 * 1024,
            max_seeds: 4096,
        }
    }

    fn unique_ledger(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        p.push(format!("grafito-mcp-policy-{name}-{pid}.jsonl"));
        p
    }

    fn set_ledger(path: &std::path::Path) {
        unsafe {
            std::env::set_var("GRAFITO_LAB_LEDGER", path.to_string_lossy().to_string());
        }
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_file(parent.join("lab_policies.jsonl"));
        }
        unsafe {
            std::env::remove_var("GRAFITO_LAB_LEDGER");
        }
    }

    #[test]
    fn suggest_ledger_vacio_da_default_honesto() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = unique_ledger("empty");
        let _ = std::fs::remove_file(&path);
        set_ledger(&path);
        let out = policy_suggest(&json!({}), &test_limits()).unwrap();
        assert_eq!(
            out.get("recommended")
                .and_then(|r| r.get("family"))
                .and_then(Value::as_str),
            Some("seeded")
        );
        assert_eq!(
            out.get("recommended")
                .and_then(|r| r.get("n"))
                .and_then(Value::as_u64),
            Some(12)
        );
        let text = out.to_string();
        assert!(
            text.contains("sin historia: exploración inicial"),
            "nota honesta ausente: {text}"
        );
        cleanup(&path);
    }

    #[test]
    fn suggest_agrega_tres_entradas_y_rankea() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = unique_ledger("ranking");
        let _ = std::fs::remove_file(&path);
        set_ledger(&path);
        let mk = |family: &str, seed: u64, n: usize, scale: f64, unit: usize| LabEntry {
            kind: "topp39".into(),
            run_id: crate::run_id_for(Family::parse(family).unwrap(), n, seed, unit, unit + 1),
            family: family.into(),
            seed,
            n,
            scale,
            unit,
            distinct: unit + 1,
            hash: 1,
            ts: crate::ledger::now_epoch_secs(),
        };
        crate::ledger::append_entry(&mk("seeded", 1, 12, 5.0, 5)).unwrap();
        crate::ledger::append_entry(&mk("grid", 2, 16, 1.0, 20)).unwrap();
        crate::ledger::append_entry(&mk("triangular", 3, 16, 1.0, 10)).unwrap();
        let out = policy_suggest(&json!({"problem": "topp39"}), &test_limits()).unwrap();
        let ranking = out.get("ranking").and_then(Value::as_array).unwrap();
        assert_eq!(ranking.len(), 3);
        assert_eq!(
            ranking[0].get("family").and_then(Value::as_str),
            Some("grid")
        );
        assert_eq!(
            ranking[0].get("best_unit").and_then(Value::as_u64),
            Some(20)
        );
        assert_eq!(
            out.get("recommended")
                .and_then(|r| r.get("family"))
                .and_then(Value::as_str),
            Some("grid")
        );
        assert_eq!(
            out.get("recommended")
                .and_then(|r| r.get("n"))
                .and_then(Value::as_u64),
            Some(16)
        );
        cleanup(&path);
    }

    #[test]
    fn replay_es_determinista() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = unique_ledger("determinista");
        let _ = std::fs::remove_file(&path);
        set_ledger(&path);
        let args = json!({"policies": [
            {"family": "seeded", "n": 8, "scale": 5.0, "seeds": [7, 42]},
            {"family": "grid", "n": 8, "seeds": [7, 42]},
        ]});
        let a = policy_replay(&args, &test_limits()).unwrap();
        let b = policy_replay(&args, &test_limits()).unwrap();
        assert_eq!(
            a.get("winner_score").and_then(Value::as_f64),
            b.get("winner_score").and_then(Value::as_f64)
        );
        assert_eq!(
            a.get("results"),
            b.get("results"),
            "el replay debe ser determinista"
        );
        assert!(a.to_string().contains(
            "replay sobre historial determinista; no garantiza generalización fuera de lo visitado"
        ));
        cleanup(&path);
    }

    #[test]
    fn replay_rechaza_policies_y_seeds_fuera_de_cota() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = unique_ledger("rechazos");
        set_ledger(&path);
        let limits = test_limits();
        assert!(policy_replay(&json!({"policies": []}), &limits).is_err());
        let many: Vec<Value> = (0..17)
            .map(|_| json!({"family": "seeded", "n": 8}))
            .collect();
        assert!(policy_replay(&json!({"policies": many}), &limits).is_err());
        let seeds65: Vec<Value> = (0..65).map(|i| json!(i)).collect();
        assert!(policy_replay(
            &json!({"policies": [{"family": "seeded", "n": 8, "seeds": seeds65}]}),
            &limits
        )
        .is_err());
        cleanup(&path);
    }

    #[test]
    fn archive_roundtrip() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path = unique_ledger("archive");
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_file(parent.join("lab_policies.jsonl"));
        }
        set_ledger(&path);
        archive_policy(&json!({"family": "seeded", "n": 12}), 3.5).unwrap();
        let got = read_policy_archive();
        let list = got.get("policies").and_then(Value::as_array).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get("score").and_then(Value::as_f64), Some(3.5));
        cleanup(&path);
    }
}
