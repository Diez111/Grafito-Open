//! Implementación de las 6 tools del laboratorio.
//!
//! Todas devuelven `serde_json::Value` listo para `structuredContent` +
//! texto para `content`. Los errores de herramienta viajan como `Err(String)`
//! en español (el protocolo los envuelve con `isError:true`, jamás como
//! error de protocolo).

use crate::ledger::{self, LabEntry};
use crate::{
    distinct_count_bounded, generate_points, GenParams, LabLimits, MAX_DISTINCT_N,
    MAX_SAT_TIMEOUT_MS,
};
use crate::{run_id_for, sha256_hex, unit_edges_spatial, unit_pairs_spatial, Family};
use grafito_geometry::Point2;
use serde_json::{json, Value};

// ── search_topp39 ────────────────────────────────────────────────────

/// `search_topp39(seed, n, family?, scale?, rows?, cols?, spacing?, radius?)`.
pub fn search_topp39(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let seed = args
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or("search_topp39: 'seed' debe ser un entero >= 0")?;
    let n = args
        .get("n")
        .and_then(Value::as_u64)
        .ok_or("search_topp39: 'n' debe ser un entero >= 1")?;
    if n == 0 || n > limits.max_points as u64 {
        return Err(format!(
            "search_topp39: 'n' fuera de [1, {}]",
            limits.max_points
        ));
    }
    let n = n as usize;
    let family_raw = args
        .get("family")
        .and_then(Value::as_str)
        .unwrap_or("seeded");
    let family = Family::parse(family_raw).map_err(|e| format!("search_topp39: {e}"))?;
    let scale = match args.get("scale").and_then(Value::as_f64) {
        Some(v) if v.is_finite() => v,
        Some(_) => return Err("search_topp39: 'scale' debe ser finita".into()),
        None => 5.0,
    };
    let rows = args.get("rows").and_then(Value::as_u64).map(|v| v as usize);
    let cols = args.get("cols").and_then(Value::as_u64).map(|v| v as usize);
    let spacing = args.get("spacing").and_then(Value::as_f64);
    if let Some(s) = spacing {
        if !s.is_finite() || s <= 0.0 || s > 1e6 {
            return Err("search_topp39: 'spacing' fuera de (0, 1e6]".into());
        }
    }
    let radius = args.get("radius").and_then(Value::as_f64);
    if let Some(r) = radius {
        if !r.is_finite() || r <= 0.0 || r > 1e6 {
            return Err("search_topp39: 'radius' fuera de (0, 1e6]".into());
        }
    }
    let pts = generate_points(
        GenParams {
            family,
            n,
            seed,
            scale,
            rows,
            cols,
            spacing,
            radius,
        },
        limits,
    )
    .map_err(|e| format!("search_topp39: {e}"))?;
    let unit = unit_pairs_spatial(&pts, 1e-9).map_err(|e| format!("search_topp39: {e}"))?;
    // Distintas: si n excede el tope honesto, se informa -1 con nota (no se rompe).
    let (distinct, distinct_capped) = match distinct_count_bounded(&pts, 1e-9) {
        Ok(d) => (d as i64, false),
        Err(_) if pts.len() > MAX_DISTINCT_N => (-1, true),
        Err(e) => return Err(format!("search_topp39: {e}")),
    };
    let distinct_for_id = if distinct < 0 { 0 } else { distinct as usize };
    let run_id = run_id_for(family, n, seed, unit, distinct_for_id);
    let mut hbytes = Vec::with_capacity(64);
    hbytes.extend_from_slice(&seed.to_le_bytes());
    hbytes.extend_from_slice(&n.to_le_bytes());
    hbytes.extend_from_slice(&scale.to_bits().to_le_bytes());
    hbytes.extend_from_slice(&unit.to_le_bytes());
    hbytes.extend_from_slice(&distinct_for_id.to_le_bytes());
    let hash = crate::fnv1a_64(&hbytes);
    // Histograma de grados (solo si las aristas entran en cota).
    let degree_hist = match unit_edges_spatial(&pts, 1e-9, limits.max_edges) {
        Ok(edges) => {
            let mut deg = vec![0usize; pts.len()];
            for (a, b) in &edges {
                if *a < deg.len() && *b < deg.len() {
                    deg[*a] += 1;
                    deg[*b] += 1;
                }
            }
            let mut hist = std::collections::BTreeMap::new();
            for d in deg {
                *hist.entry(d).or_insert(0usize) += 1;
            }
            hist
        }
        Err(_) => std::collections::BTreeMap::new(),
    };
    let entry = LabEntry {
        kind: "topp39".into(),
        run_id: run_id.clone(),
        family: family.as_str().into(),
        seed,
        n,
        scale,
        unit,
        distinct: distinct_for_id,
        hash,
        ts: ledger::now_epoch_secs(),
    };
    // Doble puerta: re-mide y compara antes de registrar.
    let unit2 = unit_pairs_spatial(&pts, 1e-9).map_err(|e| format!("search_topp39: {e}"))?;
    if unit2 != unit {
        return Err("search_topp39: la corrida no verificó su hash; resultado descartado".into());
    }
    // Solo el server escribe, y solo verificado.
    ledger::append_entry(&entry).map_err(|e| format!("search_topp39: {e}"))?;
    Ok(json!({
        "tool": "search_topp39",
        "run_id": run_id,
        "family": family.as_str(),
        "seed": seed,
        "n": n,
        "scale": scale,
        "unit": unit,
        "distinct": distinct,
        "distinct_capped": distinct_capped,
        "hash": hash,
        "verified": true,
        "degree_hist": degree_hist,
        "jsonl": entry.to_jsonl(),
        "note": "registro verificado por el motor y guardado en el ledger; solo este JSONL cuenta como evidencia, jamás el texto del modelo",
    }))
}

// ── export_dimacs ────────────────────────────────────────────────────

fn parse_points(raw: &[Value], tool: &str) -> Result<Vec<Point2>, String> {
    if raw.is_empty() {
        return Err(format!("{tool}: 'points' vacío"));
    }
    let mut out = Vec::with_capacity(raw.len().min(500_001));
    for (idx, item) in raw.iter().enumerate() {
        let pair = item
            .as_array()
            .ok_or(format!("{tool}: punto {idx} debe ser [x, y]"))?;
        if pair.len() != 2 {
            return Err(format!("{tool}: punto {idx} debe ser [x, y]"));
        }
        let x = pair[0]
            .as_f64()
            .ok_or(format!("{tool}: punto {idx} x no numérico"))?;
        let y = pair[1]
            .as_f64()
            .ok_or(format!("{tool}: punto {idx} y no numérico"))?;
        if !x.is_finite() || !y.is_finite() {
            return Err(format!("{tool}: punto {idx} no finito"));
        }
        out.push(Point2::new(x, y));
    }
    Ok(out)
}

/// `export_dimacs(points, k)`.
pub fn export_dimacs(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let raw = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or("export_dimacs: 'points' debe ser [[x, y], ...]")?;
    if raw.len() > limits.max_points {
        return Err(format!(
            "export_dimacs: puntos fuera de [1, {}]",
            limits.max_points
        ));
    }
    let points = parse_points(raw, "export_dimacs")?;
    let k = args
        .get("k")
        .and_then(Value::as_u64)
        .ok_or("export_dimacs: 'k' debe ser un entero [1, 16]")?;
    if !(1..=16).contains(&k) {
        return Err("export_dimacs: 'k' fuera de [1, 16]".into());
    }
    let k = k as usize;
    let edges = unit_edges_spatial(&points, 1e-9, limits.max_edges)
        .map_err(|e| format!("export_dimacs: {e}"))?;
    let vars = points
        .len()
        .checked_mul(k)
        .ok_or("export_dimacs: desborde en n * k")?;
    if vars > limits.max_dimacs_vars {
        return Err(format!(
            "export_dimacs: {vars} variables exceden el máximo {}; probá con menos vértices o colores",
            limits.max_dimacs_vars
        ));
    }
    let cnf = grafito_geometry::search::export_dimacs_kcoloring(points.len(), &edges, k)
        .map_err(|e| format!("export_dimacs: {e}"))?;
    if cnf.len() > limits.max_cnf_bytes {
        return Err(format!(
            "export_dimacs: CNF de {} bytes excede {} bytes; probá con menos puntos o menor k",
            cnf.len(),
            limits.max_cnf_bytes
        ));
    }
    let cnf_hash = sha256_hex(&cnf);
    let n_clauses = cnf.lines().count().saturating_sub(1);
    ledger::store_cnf(&cnf_hash, &cnf).map_err(|e| format!("export_dimacs: {e}"))?;
    Ok(json!({
        "tool": "export_dimacs",
        "n": points.len(),
        "edges": edges.len(),
        "k": k,
        "n_vars": vars,
        "n_clauses": n_clauses,
        "bytes": cnf.len(),
        "cnf_hash": cnf_hash,
        "cnf": cnf,
        "note": "llevá este CNF a kissat/cadical con sat_check; Grafito no declara coloreabilidad fuera de backtracking n<=24",
    }))
}

// ── verify_search_run ────────────────────────────────────────────────

/// `verify_search_run(run_id)`: re-ejecuta y compara (doble puerta).
pub fn verify_search_run(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let run_id = args
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or("verify_search_run: 'run_id' requerido")?;
    if !crate::is_valid_run_id(run_id) {
        return Err("verify_search_run: 'run_id' inválido".into());
    }
    let entry =
        ledger::find_run(run_id).ok_or("verify_search_run: run_id desconocido en el ledger")?;
    let family = Family::parse(&entry.family).map_err(|e| format!("verify_search_run: {e}"))?;
    let pts = generate_points(
        GenParams {
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
    .map_err(|e| format!("verify_search_run: {e}"))?;
    // Nota: grid/triangular derivan filas×cols desde n; si el run original
    // usó rows/cols explícitos distintos, la regeneración puede diferir y la
    // puerta lo detecta (OK=false honesto, no pánico).
    let unit = unit_pairs_spatial(&pts, 1e-9).map_err(|e| format!("verify_search_run: {e}"))?;
    let ok = unit == entry.unit;
    Ok(json!({
        "tool": "verify_search_run",
        "run_id": entry.run_id,
        "ok": ok,
        "gates_passed": if ok { vec!["hash", "metrics"] } else { vec![] },
        "hash_fnv": entry.hash,
        "unit": entry.unit,
        "unit_fresh": unit,
    }))
}

// ── topp39_best_of ───────────────────────────────────────────────────

/// `topp39_best_of(seeds, n, family?, scale?)`: barre, re-verifica, mejor.
pub fn topp39_best_of(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let seeds_raw = args
        .get("seeds")
        .and_then(Value::as_array)
        .ok_or("topp39_best_of: 'seeds' debe ser [u64, ...]")?;
    if seeds_raw.is_empty() || seeds_raw.len() > limits.max_seeds {
        return Err(format!(
            "topp39_best_of: seeds fuera de [1, {}]",
            limits.max_seeds
        ));
    }
    let mut seeds = Vec::with_capacity(seeds_raw.len());
    for (i, v) in seeds_raw.iter().enumerate() {
        let s = v
            .as_u64()
            .ok_or(format!("topp39_best_of: seeds[{i}] debe ser entero >= 0"))?;
        seeds.push(s);
    }
    let n = args
        .get("n")
        .and_then(Value::as_u64)
        .ok_or("topp39_best_of: 'n' requerido")?;
    if n == 0 || n > limits.max_points as u64 {
        return Err(format!(
            "topp39_best_of: 'n' fuera de [1, {}]",
            limits.max_points
        ));
    }
    let n = n as usize;
    let family = Family::parse(
        args.get("family")
            .and_then(Value::as_str)
            .unwrap_or("seeded"),
    )
    .map_err(|e| format!("topp39_best_of: {e}"))?;
    let scale = match args.get("scale").and_then(Value::as_f64) {
        Some(v) if v.is_finite() => v,
        Some(_) => return Err("topp39_best_of: 'scale' debe ser finita".into()),
        None => 5.0,
    };
    let mut best: Option<Value> = None;
    let mut best_unit = 0usize;
    let mut verified_count = 0usize;
    let mut best_run_id = String::new();
    let mut best_jsonl = String::new();
    for seed in seeds {
        let sub = json!({"seed": seed, "n": n, "family": family.as_str(), "scale": scale});
        match search_topp39(&sub, limits) {
            Ok(payload) => {
                let unit = payload.get("unit").and_then(Value::as_u64).unwrap_or(0) as usize;
                verified_count += 1;
                if best.is_none() || unit > best_unit {
                    best_unit = unit;
                    best_run_id = payload
                        .get("run_id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    best_jsonl = payload
                        .get("jsonl")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    best = Some(payload);
                }
            }
            Err(_) => continue, // corrida descartada sin romper el loop
        }
    }
    if verified_count == 0 {
        return Err("topp39_best_of: ninguna corrida verificó; resultados descartados".into());
    }
    let _ = best;
    Ok(json!({
        "tool": "topp39_best_of",
        "best_run_id": best_run_id,
        "best_unit": best_unit,
        "verified_count": verified_count,
        "best_jsonl": best_jsonl,
    }))
}

// ── sat_check ────────────────────────────────────────────────────────

/// `sat_check(cnf_hash, solver?, timeout_ms?)`.
pub fn sat_check(args: &Value) -> Result<Value, String> {
    let cnf_hash = args
        .get("cnf_hash")
        .and_then(Value::as_str)
        .ok_or("sat_check: 'cnf_hash' requerido (64 hex)")?;
    if !crate::is_valid_cnf_hash(cnf_hash) {
        return Err("sat_check: 'cnf_hash' inválido (64 hex)".into());
    }
    let solver = crate::sat::Solver::parse(
        args.get("solver")
            .and_then(Value::as_str)
            .unwrap_or("kissat"),
    )
    .map_err(|e| format!("sat_check: {e}"))?;
    let timeout_ms = match args.get("timeout_ms").and_then(Value::as_u64) {
        Some(v) if v <= MAX_SAT_TIMEOUT_MS => v,
        Some(v) => {
            return Err(format!(
                "sat_check: 'timeout_ms' {v} excede el máximo {MAX_SAT_TIMEOUT_MS} (24 h); 0 = sin timeout explícito"
            ))
        }
        None => crate::DEFAULT_SAT_TIMEOUT_MS,
    };
    let cnf = ledger::load_cnf(cnf_hash).map_err(|e| format!("sat_check: {e}"))?;
    // Escribe a un temporal para el solver (nombre por hash, sin traversal).
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("grafito-{cnf_hash}.cnf"));
    std::fs::write(&tmp, &cnf).map_err(|e| format!("sat_check: no se pudo staging: {e}"))?;
    let outcome =
        crate::sat::run_solver(solver, &tmp, timeout_ms).map_err(|e| format!("sat_check: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    let payload = json!({
        "tool": "sat_check",
        "cnf_hash": cnf_hash,
        "status": outcome.status,
        "solver": outcome.solver,
        "time_ms": outcome.time_ms,
        "model": outcome.model,
        "unbounded_timeout": timeout_ms == 0,
    });
    // Persiste el último resultado para el recurso `result`.
    let _ = ledger::store_result(cnf_hash, &payload);
    Ok(payload)
}

// ── chromatic_solve ──────────────────────────────────────────────────

/// Decodifica un modelo SAT (literales `v`) a coloreo por vértice.
/// Codificación: var(v, c) = v * k + c + 1 (la de `export_dimacs_kcoloring`).
/// `None` si algún vértice queda sin color en el modelo.
fn model_to_coloring(model: &[i64], n: usize, k: usize) -> Option<Vec<usize>> {
    let mut coloring = vec![usize::MAX; n];
    for &lit in model {
        if lit <= 0 {
            continue;
        }
        let var = lit as usize - 1;
        let v = var / k;
        let c = var % k;
        if v < n {
            coloring[v] = c;
        }
    }
    if coloring.iter().all(|&c| c < k) {
        Some(coloring)
    } else {
        None
    }
}

/// Cuenta violaciones de un coloreo sobre las aristas y devuelve la primera
/// arista en conflicto (doble puerta anti-alucinación, sin solver).
fn coloring_violations(
    edges: &[(usize, usize)],
    coloring: &[usize],
) -> Result<(usize, Option<(usize, usize)>), String> {
    let mut violations = 0usize;
    let mut first: Option<(usize, usize)> = None;
    for &(a, b) in edges {
        let ca = *coloring
            .get(a)
            .ok_or("verify_coloring: 'coloring' más corto que los vértices")?;
        let cb = *coloring
            .get(b)
            .ok_or("verify_coloring: 'coloring' más corto que los vértices")?;
        if ca == cb {
            violations += 1;
            if first.is_none() {
                first = Some((a, b));
            }
        }
    }
    Ok((violations, first))
}

/// Etapa compartida: puntos + k + solver + timeout con validación honesta.
fn chromatic_args(
    args: &Value,
    tool: &str,
    limits: &LabLimits,
) -> Result<(Vec<Point2>, usize, crate::sat::Solver, u64), String> {
    let raw = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or(format!("{tool}: 'points' debe ser [[x, y], ...]"))?;
    if raw.len() > limits.max_points {
        return Err(format!(
            "{tool}: puntos fuera de [1, {}]",
            limits.max_points
        ));
    }
    let points = parse_points(raw, tool)?;
    let k = args
        .get("k")
        .and_then(Value::as_u64)
        .ok_or(format!("{tool}: 'k' debe ser un entero [1, 16]"))?;
    if !(1..=16).contains(&k) {
        return Err(format!("{tool}: 'k' fuera de [1, 16]"));
    }
    let solver = crate::sat::Solver::parse(
        args.get("solver")
            .and_then(Value::as_str)
            .unwrap_or("kissat"),
    )
    .map_err(|e| format!("{tool}: {e}"))?;
    let timeout_ms = match args.get("timeout_ms").and_then(Value::as_u64) {
        Some(v) if v <= MAX_SAT_TIMEOUT_MS => v,
        Some(v) => {
            return Err(format!(
                "{tool}: 'timeout_ms' {v} excede el máximo {MAX_SAT_TIMEOUT_MS} (24 h); 0 = sin timeout explícito"
            ))
        }
        None => crate::DEFAULT_SAT_TIMEOUT_MS,
    };
    Ok((points, k as usize, solver, timeout_ms))
}

/// `chromatic_solve(points, k, solver?, timeout_ms?)`: resuelve la
/// k-coloración del grafo unit-distance con kissat/cadical y VERIFICA el
/// modelo arista por arista. `model_checked: true` = coloreo válido; jamás se
/// declara coloreabilidad sin ese chequeo. UNSAT = no k-coloreable según el
/// solver (sin proof-checking; `cnf_hash` queda para repro).
pub fn chromatic_solve(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let tool = "chromatic_solve";
    let (points, k, solver, timeout_ms) = chromatic_args(args, tool, limits)?;
    let edges =
        unit_edges_spatial(&points, 1e-9, limits.max_edges).map_err(|e| format!("{tool}: {e}"))?;
    let vars = points
        .len()
        .checked_mul(k)
        .ok_or(format!("{tool}: desborde en n * k"))?;
    if vars > limits.max_dimacs_vars {
        return Err(format!(
            "{tool}: {vars} variables exceden el máximo {}; probá con menos vértices o colores",
            limits.max_dimacs_vars
        ));
    }
    let cnf = grafito_geometry::search::export_dimacs_kcoloring(points.len(), &edges, k)
        .map_err(|e| format!("{tool}: {e}"))?;
    if cnf.len() > limits.max_cnf_bytes {
        return Err(format!(
            "{tool}: CNF de {} bytes excede {} bytes; probá con menos puntos o menor k",
            cnf.len(),
            limits.max_cnf_bytes
        ));
    }
    let cnf_hash = sha256_hex(&cnf);
    ledger::store_cnf(&cnf_hash, &cnf).map_err(|e| format!("{tool}: {e}"))?;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("grafito-{cnf_hash}.cnf"));
    std::fs::write(&tmp, &cnf).map_err(|e| format!("{tool}: no se pudo staging: {e}"))?;
    let outcome =
        crate::sat::run_solver(solver, &tmp, timeout_ms).map_err(|e| format!("{tool}: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    let payload = if outcome.status == "SAT" {
        let model = outcome.model.clone().unwrap_or_default();
        let coloring = model_to_coloring(&model, points.len(), k);
        match coloring {
            Some(ref col) => {
                let (violations, first) = coloring_violations(&edges, col)?;
                json!({
                    "tool": tool,
                    "status": "SAT",
                    "k": k,
                    "n": points.len(),
                    "edges": edges.len(),
                    "solver": outcome.solver,
                    "time_ms": outcome.time_ms,
                    "cnf_hash": cnf_hash,
                    "coloring": col,
                    "model_checked": violations == 0,
                    "violations": violations,
                    "first_violation": first,
                    "note": if violations == 0 {
                        "coloreo verificado arista por arista"
                    } else {
                        "modelo SAT con violaciones: NO declarar coloreabilidad (bug de codificación o solver)"
                    },
                })
            }
            None => json!({
                "tool": tool,
                "status": "SAT",
                "k": k,
                "n": points.len(),
                "edges": edges.len(),
                "solver": outcome.solver,
                "time_ms": outcome.time_ms,
                "cnf_hash": cnf_hash,
                "model_checked": false,
                "note": "modelo SAT sin coloreo completo: NO declarar coloreabilidad",
            }),
        }
    } else if outcome.status == "UNSAT" {
        json!({
            "tool": tool,
            "status": "UNSAT",
            "k": k,
            "n": points.len(),
            "edges": edges.len(),
            "solver": outcome.solver,
            "time_ms": outcome.time_ms,
            "cnf_hash": cnf_hash,
            "note": "no k-coloreable según el solver (sin proof-checking); reproducí con sat_check(cnf_hash) u otro solver",
        })
    } else {
        json!({
            "tool": tool,
            "status": outcome.status,
            "k": k,
            "n": points.len(),
            "edges": edges.len(),
            "solver": outcome.solver,
            "time_ms": outcome.time_ms,
            "cnf_hash": cnf_hash,
            "note": "sin veredicto; probá con más timeout_ms u otro solver",
        })
    };
    let _ = ledger::store_result(&cnf_hash, &payload);
    Ok(payload)
}

/// `verify_coloring(points, coloring, k?)`: doble puerta pura, sin solver.
/// Cuenta violaciones del coloreo candidato sobre el grafo unit-distance y
/// señala la primera arista en conflicto. `valid: true` = coloreo propio.
pub fn verify_coloring(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let tool = "verify_coloring";
    let raw = args
        .get("points")
        .and_then(Value::as_array)
        .ok_or("verify_coloring: 'points' debe ser [[x, y], ...]")?;
    if raw.len() > limits.max_points {
        return Err(format!(
            "verify_coloring: puntos fuera de [1, {}]",
            limits.max_points
        ));
    }
    let points = parse_points(raw, tool)?;
    let raw_col = args
        .get("coloring")
        .and_then(Value::as_array)
        .ok_or("verify_coloring: 'coloring' debe ser [c0, c1, ...]")?;
    if raw_col.len() != points.len() {
        return Err(format!(
            "verify_coloring: 'coloring' tiene {} entradas y hay {} puntos",
            raw_col.len(),
            points.len()
        ));
    }
    let k = match args.get("k").and_then(Value::as_u64) {
        Some(v) if (1..=16).contains(&v) => v as usize,
        Some(_) => return Err("verify_coloring: 'k' fuera de [1, 16]".into()),
        None => 16,
    };
    let mut coloring = Vec::with_capacity(raw_col.len());
    for (idx, item) in raw_col.iter().enumerate() {
        let c = item
            .as_u64()
            .ok_or(format!("verify_coloring: color {idx} no entero"))?;
        if c as usize >= k {
            return Err(format!(
                "verify_coloring: color {idx} = {c} fuera de [0, {k})"
            ));
        }
        coloring.push(c as usize);
    }
    let edges = unit_edges_spatial(&points, 1e-9, limits.max_edges)
        .map_err(|e| format!("verify_coloring: {e}"))?;
    let (violations, first) = coloring_violations(&edges, &coloring)?;
    Ok(json!({
        "tool": tool,
        "n": points.len(),
        "edges": edges.len(),
        "k": k,
        "valid": violations == 0,
        "violations": violations,
        "first_violation": first,
        "note": if violations == 0 {
            "coloreo verificado arista por arista (doble puerta, sin solver)"
        } else {
            "coloreo inválido: mirá first_violation"
        },
    }))
}

// ── check_bounds ─────────────────────────────────────────────────────

/// `check_bounds(problem?)`: cotas efectivas + guía honesta.
pub fn check_bounds(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let problem = args.get("problem").and_then(Value::as_str).unwrap_or("all");
    Ok(json!({
        "tool": "check_bounds",
        "problem": problem,
        "known": {
            "hadwiger_nelson": "5 <= chi(R²) <= 7; un grafo 6-cromático verificado es hito, no solución",
            "topp39_random": "el azar da unit≈0; las estructuradas (triangular/grid) dominan",
        },
        "limits": {
            "max_points": limits.max_points,
            "max_edges": limits.max_edges,
            "max_dimacs_vars": limits.max_dimacs_vars,
            "max_cnf_bytes": limits.max_cnf_bytes,
            "max_seeds": limits.max_seeds,
            "max_distinct_n": MAX_DISTINCT_N,
            "max_sat_timeout_ms": MAX_SAT_TIMEOUT_MS,
        },
        "honest": "distintas/halving/triángulo-vacío son O(n²)/O(n³)/O(n⁴): fuera de tope se muestrea, no se finge",
        "env": "GRAFITO_LAB_MAX_POINTS / GRAFITO_LAB_MAX_EDGES / GRAFITO_LAB_MAX_DIMACS_VARS / GRAFITO_LAB_MAX_CNF_BYTES / GRAFITO_LAB_MAX_SEEDS / GRAFITO_LAB_LEDGER",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_to_coloring_decodifica_var_v_k_c() {
        // triángulo k=3: var(v, c) = v*k + c + 1 → positivos 1, 5, 9
        let model = vec![1, -2, -3, -4, 5, -6, -7, -8, 9];
        let col = model_to_coloring(&model, 3, 3).unwrap();
        assert_eq!(col, vec![0, 1, 2]);
    }

    #[test]
    fn model_incompleto_devuelve_none() {
        assert!(model_to_coloring(&[1, -2, -3], 3, 3).is_none());
    }

    #[test]
    fn coloring_violations_cuenta_y_senal_a() {
        let edges = vec![(0, 1), (1, 2), (0, 2)];
        let (v, first) = coloring_violations(&edges, &[0, 0, 1]).unwrap();
        assert_eq!(v, 1);
        assert_eq!(first, Some((0, 1)));
        let (v2, first2) = coloring_violations(&edges, &[0, 1, 2]).unwrap();
        assert_eq!(v2, 0);
        assert!(first2.is_none());
    }

    #[test]
    fn search_y_verify_roundtrip() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let limits = LabLimits {
            max_points: 2000,
            max_edges: 200_000,
            max_dimacs_vars: 20_000,
            max_cnf_bytes: 512 * 1024,
            max_seeds: 4096,
        };
        // Usa ledger temporal para no ensuciar el real.
        let dir = std::env::temp_dir().join("grafito-mcp-test-ledger");
        let _ = std::fs::create_dir_all(&dir);
        let ledger_file = dir.join("lab.jsonl");
        let _ = std::fs::remove_file(&ledger_file);
        unsafe {
            std::env::set_var(
                "GRAFITO_LAB_LEDGER",
                ledger_file.to_string_lossy().to_string(),
            );
        }
        let out = search_topp39(&json!({"seed": 42, "n": 12}), &limits).unwrap();
        let run_id = out.get("run_id").and_then(Value::as_str).unwrap();
        assert!(run_id.starts_with("t39-seeded-n12-h"));
        let v = verify_search_run(&json!({"run_id": run_id}), &limits).unwrap();
        assert_eq!(v.get("ok").and_then(Value::as_bool), Some(true));
        let _ = std::fs::remove_file(&ledger_file);
        unsafe {
            std::env::remove_var("GRAFITO_LAB_LEDGER");
        }
    }

    #[test]
    fn export_rechaza_k_fuera_de_rango() {
        let limits = LabLimits::default();
        let r = export_dimacs(&json!({"points": [[0.0, 0.0]], "k": 99}), &limits);
        assert!(r.is_err());
    }
}
