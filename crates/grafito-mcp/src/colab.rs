//! Offload pesado a Google Colab (vía `colab-mcp` del lado cliente).
//!
//! Arquitectura (ver `docs/COLAB_OFFLOAD.md`): este server NO habla con
//! Google — no tiene credenciales ni browser. Empaqueta trabajos pesados
//! como scripts Python autocontenidos (`export_colab_job`), el agente los
//! corre en la VM Pro del usuario con las tools proxedas de `colab-mcp`, y
//! el resultado vuelve acá para verificación (`import_colab_result`).
//!
//! Reglas duras:
//! - Solo matemática sale del box (puntos, CNFs, polinomios). Guardia PII:
//!   se rechaza cualquier payload con emails o rutas de home.
//! - Nada externo entra al ledger sin verificación: SAT se chequea
//!   cláusula por cláusula en local (fuerte); sweeps chicos se regeneran
//!   (fuerte); lo grande queda `unverified` (dato, no evidencia).
//! - Regen exacta: `seeded` usa SplitMix64 puro, bit-idéntico al motor
//!   (paridad pineada en test contra `seeded_point_set(42, 1, 5.0)`).

use serde_json::{json, Value};

// ── Límites ──────────────────────────────────────────────────────────

/// Puntos máximos por sweep en Colab (numpy por chunks; más = OOM en VM).
pub const MAX_COLAB_N: usize = 200_000;
/// Seeds máximas por job.
pub const MAX_COLAB_SEEDS: usize = 256;
/// CNFs máximos por job SAT.
pub const MAX_COLAB_CNFS: usize = 8;
/// Bytes totales de CNF por job (el texto viaja en el script).
pub const MAX_COLAB_CNF_BYTES: usize = 5 * 1024 * 1024;
/// Timeout SAT por instancia en Colab (s).
pub const MAX_COLAB_SAT_TIMEOUT_S: u64 = 600;
/// Distintas en Colab: mismo tope honesto que local.
pub const MAX_COLAB_DISTINCT_N: usize = 10_000;

// ── SplitMix64 (paridad con `search.rs::splitmix64`) ──────────────────

const SM_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const SM_M1: u64 = 0xBF58_476D_1CE4_E5B9;
const SM_M2: u64 = 0x94D0_49BB_1331_11EB;

fn splitmix_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(SM_GAMMA);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(SM_M1);
    z = (z ^ (z >> 27)).wrapping_mul(SM_M2);
    z ^ (z >> 31)
}

/// Regen local exacta de un set `seeded` (para verificación full-local).
pub fn regen_seeded(seed: u64, n: usize, scale: f64) -> Vec<(f64, f64)> {
    let mut state = seed;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let a = splitmix_next(&mut state);
        let b = splitmix_next(&mut state);
        // Orden EXACTO del motor (`search.rs`): el redondeo depende de él.
        #[allow(clippy::cast_precision_loss)]
        let x = (a as f64 / u64::MAX as f64) * 2.0 * scale - scale;
        #[allow(clippy::cast_precision_loss)]
        let y = (b as f64 / u64::MAX as f64) * 2.0 * scale - scale;
        out.push((x, y));
    }
    out
}

// ── Store de jobs ────────────────────────────────────────────────────

fn jobs_dir() -> std::path::PathBuf {
    // Hermano del ledger (mismo esquema que lab_cnfs/lab_proofs).
    let ledger = crate::ledger::ledger_path();
    if let Some(parent) = ledger.parent() {
        return parent.join("lab_jobs");
    }
    crate::ledger::cnf_dir().join("lab_jobs")
}

fn store_job(job_id: &str, manifest: &str, script: &str) -> Result<(), String> {
    if !is_valid_job_id(job_id) {
        return Err("job_id inválido".into());
    }
    let dir = jobs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("jobs: no se pudo crear {dir:?}: {e}"))?;
    std::fs::write(dir.join(format!("{job_id}.json")), manifest)
        .map_err(|e| format!("jobs: no se pudo guardar manifiesto: {e}"))?;
    std::fs::write(dir.join(format!("{job_id}.py")), script)
        .map_err(|e| format!("jobs: no se pudo guardar script: {e}"))?;
    Ok(())
}

fn load_job(job_id: &str) -> Result<Value, String> {
    if !is_valid_job_id(job_id) {
        return Err("job_id inválido (64 hex)".into());
    }
    let path = jobs_dir().join(format!("{job_id}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|_| "job desconocido; regeneralo con export_colab_job".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("manifiesto corrupto: {e}"))
}

pub fn is_valid_job_id(raw: &str) -> bool {
    raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn read_colab_job(job_id: &str) -> Result<Value, String> {
    let manifest = load_job(job_id)?;
    let script_path = jobs_dir().join(format!("{job_id}.py"));
    let script = std::fs::read_to_string(&script_path).unwrap_or_default();
    Ok(json!({"job_id": job_id, "manifest": manifest, "script_chars": script.len()}))
}

// ── Guardia PII ──────────────────────────────────────────────────────

/// La nube solo recibe matemática: ni emails, ni rutas de home, ni claves.
fn reject_pii(canonical: &str) -> Result<(), String> {
    let low = canonical.to_lowercase();
    for needle in [
        "/home/",
        "/users/",
        "c:\\",
        "c:/",
        "@gmail.",
        "@hotmail.",
        "@yahoo.",
        "@outlook.",
        "ssh-rsa",
        "sk-ant-",
        "sk-",
    ] {
        if low.contains(needle) {
            return Err(format!(
                "colab: el payload contiene '{needle}', posible dato personal; la nube solo recibe matemática pura"
            ));
        }
    }
    // Email genérico user@host.tld (heurística simple, sin regex).
    if let Some(at) = low.find('@') {
        let after = &low[at + 1..];
        if after.contains('.')
            && after.len() >= 3
            && low[..at]
                .chars()
                .rev()
                .take(64)
                .any(|c| c.is_alphanumeric())
        {
            return Err(
                "colab: el payload parece contener un email; la nube solo recibe matemática pura"
                    .into(),
            );
        }
    }
    Ok(())
}

// ── export_colab_job ─────────────────────────────────────────────────

pub fn colab_tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "export_colab_job",
            "description": "Empaqueta un cálculo pesado como script Python autocontenido para correr en Google Colab (VM Pro con GPU) vía las tools de colab-mcp. Kinds: unit_sweep (barrido numpy con regen exacta), sat_sweep (batch python-sat), cas_crosscheck (sympy). Devuelve job_id + script + cómo verificar la vuelta.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["unit_sweep", "sat_sweep", "cas_crosscheck"]},
                    "params": {"type": "object"}
                },
                "required": ["kind", "params"]
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false}
        }),
        json!({
            "name": "import_colab_result",
            "description": "Verifica un resultado traído de Colab contra el manifiesto del job y lo registra. SAT se chequea cláusula por cláusula en local (fuerte); sweeps chicos se regeneran (fuerte); lo grande queda unverified (dato, no evidencia).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job_id": {"type": "string"},
                    "result": {"type": "object"}
                },
                "required": ["job_id", "result"]
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false}
        }),
    ]
}

fn canonical_json(value: &Value) -> String {
    // Orden de claves estable para job_id citable.
    let mut buf = String::new();
    write_canonical(value, &mut buf);
    buf
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

pub fn export_colab_job(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("export_colab_job: 'kind' requerido (unit_sweep|sat_sweep|cas_crosscheck)")?;
    let params = args
        .get("params")
        .and_then(Value::as_object)
        .ok_or("export_colab_job: 'params' debe ser objeto")?;
    let params_value = Value::Object(params.clone());
    reject_pii(&canonical_json(&params_value))?;
    let (script, verify_how) = match kind {
        "unit_sweep" => build_unit_sweep(params, limits)?,
        "sat_sweep" => build_sat_sweep(params)?,
        "cas_crosscheck" => build_cas_crosscheck(params)?,
        otra => {
            return Err(format!(
                "export_colab_job: kind '{otra}' desconocido (unit_sweep|sat_sweep|cas_crosscheck)"
            ));
        }
    };
    let manifest = json!({"kind": kind, "params": params_value, "script_chars": script.len()});
    let job_id = crate::sha256_hex(&canonical_json(&manifest));
    store_job(&job_id, &manifest.to_string(), &script)?;
    Ok(json!({
        "tool": "export_colab_job",
        "job_id": job_id,
        "kind": kind,
        "script": script,
        "verify_how": verify_how,
        "next": "corré el script en Colab (tools proxedas de colab-mcp) y traé el JSON impreso con import_colab_result",
        "pii": "payload solo-matemática verificado; nada personal sale del box",
    }))
}

use crate::LabLimits;

fn get_u64(params: &serde_json::Map<String, Value>, key: &str) -> Result<u64, String> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(format!("export_colab_job: '{key}' debe ser entero >= 0"))
}

fn get_f64(params: &serde_json::Map<String, Value>, key: &str, def: f64) -> Result<f64, String> {
    match params.get(key) {
        None => Ok(def),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .ok_or(format!("export_colab_job: '{key}' debe ser número finito")),
    }
}

fn build_unit_sweep(
    params: &serde_json::Map<String, Value>,
    limits: &LabLimits,
) -> Result<(String, String), String> {
    let family_raw = params
        .get("family")
        .and_then(Value::as_str)
        .unwrap_or("seeded");
    let family = crate::Family::parse(family_raw).map_err(|e| format!("unit_sweep: {e}"))?;
    let n = get_u64(params, "n")? as usize;
    if n == 0 || n > MAX_COLAB_N {
        return Err(format!("unit_sweep: n fuera de [1, {MAX_COLAB_N}]"));
    }
    let _ = limits;
    let seeds_raw = params
        .get("seeds")
        .and_then(Value::as_array)
        .ok_or("unit_sweep: 'seeds' debe ser [u64, ...]")?;
    if seeds_raw.is_empty() || seeds_raw.len() > MAX_COLAB_SEEDS {
        return Err(format!("unit_sweep: seeds fuera de [1, {MAX_COLAB_SEEDS}]"));
    }
    let mut seeds = Vec::with_capacity(seeds_raw.len());
    for (i, v) in seeds_raw.iter().enumerate() {
        seeds.push(
            v.as_u64()
                .ok_or(format!("unit_sweep: seeds[{i}] debe ser entero"))?,
        );
    }
    let scale = get_f64(params, "scale", 5.0)?;
    if !(0.0 < scale && scale <= 1e6) {
        return Err("unit_sweep: scale fuera de (0, 1e6]".into());
    }
    let tol = get_f64(params, "tol", 1e-9)?;
    if !(1e-12..=1e-3).contains(&tol) {
        return Err("unit_sweep: tol fuera de [1e-12, 1e-3]".into());
    }
    // Retículas: filas×cols explícitas (sin derivación ambigua entre lenguajes).
    let grid_extra = match family {
        crate::Family::Grid | crate::Family::Triangular => {
            let rows = get_u64(params, "rows")? as usize;
            let cols = get_u64(params, "cols")? as usize;
            if rows == 0 || cols == 0 {
                return Err("unit_sweep: rows/cols >= 1".into());
            }
            let spacing = get_f64(params, "spacing", 1.0)?;
            if !(0.0 < spacing && spacing <= 1e6) {
                return Err("unit_sweep: spacing fuera de (0, 1e6]".into());
            }
            format!("ROWS={rows}\nCOLS={cols}\nSPACING={spacing:?}\n")
        }
        crate::Family::RegularPolygon => {
            let radius = get_f64(params, "radius", 1.0)?;
            if !(0.0 < radius && radius <= 1e6) {
                return Err("unit_sweep: radius fuera de (0, 1e6]".into());
            }
            format!("RADIUS={radius:?}\n")
        }
        crate::Family::Seeded => String::new(),
    };
    let seeds_list = seeds
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#""""Barrido TOPP 39 en Colab (numpy). Regen exacta bit a bit con Grafito.
Imprime UNA línea JSON a stdout: {{"job_id": "...", "runs": [...]}}.
"""
import json, struct
import numpy as np

FAMILY = "{family}"
N = {n}
SEEDS = [{seeds_list}]
SCALE = {scale:?}
TOL = {tol:?}
{grid_extra}
MASK = (1 << 64) - 1
SM_GAMMA = 0x9E3779B97F4A7C15
SM_M1 = 0xBF58476D1CE4E5B9
SM_M2 = 0x94D049BB133111EB

def splitmix(state):
    state = (state + SM_GAMMA) & MASK
    z = state
    z = ((z ^ (z >> 30)) * SM_M1) & MASK
    z = ((z ^ (z >> 27)) * SM_M2) & MASK
    return state, z ^ (z >> 31)

def gen_seeded(seed, n, scale):
    st = seed
    pts = np.empty((n, 2))
    for i in range(n):
        st, a = splitmix(st)
        st, b = splitmix(st)
        pts[i, 0] = a / 18446744073709551615 * 2.0 * scale - scale
        pts[i, 1] = b / 18446744073709551615 * 2.0 * scale - scale
    return pts

def gen_grid(rows, cols, spacing):
    yy, xx = np.mgrid[0:rows, 0:cols]
    return np.stack([xx.ravel() * spacing, yy.ravel() * spacing], axis=1)

def gen_triangular(rows, cols, spacing):
    dy = spacing * np.sqrt(3.0) / 2.0
    pts = np.empty((rows * cols, 2))
    k = 0
    for r in range(rows):
        off = spacing / 2.0 if r % 2 == 1 else 0.0
        for c in range(cols):
            pts[k] = (c * spacing + off, r * dy)
            k += 1
    return pts

def gen_polygon(n, radius):
    th = 2.0 * np.pi * np.arange(n) / n
    return np.stack([radius * np.cos(th), radius * np.sin(th)], axis=1)

def unit_count(pts, tol):
    n = len(pts)
    lo, hi = max(0.0, 1.0 - tol), 1.0 + tol
    lo2, hi2 = lo * lo, hi * hi
    total = 0
    CH = 4096
    for s in range(0, n, CH):
        d = pts[s:s + CH, None, :] - pts[None, :, :]
        d2 = np.einsum("ijk,ijk->ij", d, d)
        total += int(np.sum((d2 >= lo2) & (d2 <= hi2))) // 1
    # cada par contado 2 veces (i,j)+(j,i); diagonal nunca unitaria
    return total // 2

def fnv(data: bytes):
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h

runs = []
for seed in SEEDS:
    if FAMILY == "seeded":
        pts = gen_seeded(seed, N, SCALE)
    elif FAMILY == "grid":
        pts = gen_grid(ROWS, COLS, SPACING)[:N]
    elif FAMILY == "triangular":
        pts = gen_triangular(ROWS, COLS, SPACING)[:N]
    else:
        pts = gen_polygon(N, RADIUS)
    unit = unit_count(pts, TOL)
    blob = struct.pack("<%dd" % (2 * len(pts)), *pts.ravel())
    runs.append({{"seed": seed, "n": len(pts), "unit": unit,
                  "points_hash": format(fnv(blob), "016x")}})
print(json.dumps({{"runs": runs}}))
"#,
        family = family.as_str(),
    );
    Ok((
        script,
        "la vuelta se re-mide en local si n entra en topes (full-local) o se coteja points_hash + spot-check en seeded".into(),
    ))
}

fn build_sat_sweep(params: &serde_json::Map<String, Value>) -> Result<(String, String), String> {
    let cnfs = params
        .get("cnfs")
        .and_then(Value::as_array)
        .ok_or("sat_sweep: 'cnfs' debe ser [{name, text}, ...]")?;
    if cnfs.is_empty() || cnfs.len() > MAX_COLAB_CNFS {
        return Err(format!("sat_sweep: cnfs fuera de [1, {MAX_COLAB_CNFS}]"));
    }
    let mut total = 0usize;
    let mut items = Vec::with_capacity(cnfs.len());
    for (i, c) in cnfs.iter().enumerate() {
        let name = c
            .get("name")
            .and_then(Value::as_str)
            .ok_or(format!("sat_sweep: cnfs[{i}].name requerido"))?;
        if name.len() > 64
            || !name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(format!("sat_sweep: cnfs[{i}].name inválido"));
        }
        let text = c
            .get("text")
            .and_then(Value::as_str)
            .ok_or(format!("sat_sweep: cnfs[{i}].text requerido"))?;
        total += text.len();
        items.push((name.to_string(), text.to_string()));
    }
    if total > MAX_COLAB_CNF_BYTES {
        return Err(format!(
            "sat_sweep: {total} bytes exceden {MAX_COLAB_CNF_BYTES}"
        ));
    }
    if items.iter().any(|(_, t)| t.contains('\0')) {
        return Err("sat_sweep: CNF con NUL".into());
    }
    let timeout_s = params
        .get("timeout_s")
        .and_then(Value::as_u64)
        .unwrap_or(60);
    if timeout_s == 0 || timeout_s > MAX_COLAB_SAT_TIMEOUT_S {
        return Err(format!(
            "sat_sweep: timeout_s fuera de [1, {MAX_COLAB_SAT_TIMEOUT_S}]"
        ));
    }
    let mut cases = String::new();
    for (name, text) in &items {
        cases.push_str(&format!("CASES.append(({name:?}, {text:?}))\n"));
    }
    let script = format!(
        r#""""Batch SAT en Colab (GPU/Pro). Imprime UNA línea JSON.
Requiere: pip install python-sat (una vez por VM).
"""
import json, time, subprocess, sys
from concurrent.futures import ThreadPoolExecutor, TimeoutError
subprocess.check_call([sys.executable, "-m", "pip", "-q", "install", "python-sat"])
from pysat.solvers import Glucose3

TIMEOUT_S = {timeout_s}
CASES = []
{cases}
def parse_dimacs(text):
    clauses = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("c") or line.startswith("p"):
            continue
        lits = [int(x) for x in line.split()]
        if lits and lits[-1] == 0:
            lits.pop()
        if lits:
            clauses.append(lits)
    return clauses

def solve_one(clauses):
    with Glucose3() as solver:
        for cl in clauses:
            solver.add_clause(cl)
        sat = solver.solve()
        return sat, solver.get_model() if sat else None

out = []
pool = ThreadPoolExecutor(max_workers=1)
for name, text in CASES:
    clauses = parse_dimacs(text)
    t0 = time.time()
    fut = pool.submit(solve_one, clauses)
    try:
        sat, model = fut.result(timeout=TIMEOUT_S)
        status = "SAT" if sat else "UNSAT"
    except TimeoutError:
        sat, model, status = None, None, "TIMEOUT"
    out.append({{"name": name, "status": status, "model": model,
                 "time_s": round(time.time() - t0, 3)}})
print(json.dumps({{"results": out}}))
"#
    );
    Ok((
        script,
        "los modelos SAT se chequean cláusula por cláusula en local (fuerte); UNSAT/TIMEOUT quedan a confianza del solver".into(),
    ))
}

fn build_cas_crosscheck(
    params: &serde_json::Map<String, Value>,
) -> Result<(String, String), String> {
    let expression = params
        .get("expression")
        .and_then(Value::as_str)
        .ok_or("cas_crosscheck: 'expression' requerida")?;
    let claim = params
        .get("claim")
        .and_then(Value::as_str)
        .ok_or("cas_crosscheck: 'claim' requerido")?;
    let check = params
        .get("check")
        .and_then(Value::as_str)
        .ok_or("cas_crosscheck: 'check' requerido (derivative_of|integral_of|identity)")?;
    if !matches!(check, "derivative_of" | "integral_of" | "identity") {
        return Err("cas_crosscheck: check inválido".into());
    }
    for (k, v) in [("expression", expression), ("claim", claim)] {
        if v.len() > 2000 || v.contains('\n') || v.contains('\0') {
            return Err(format!(
                "cas_crosscheck: '{k}' inválida (≤2000 chars, 1 línea)"
            ));
        }
    }
    let script = format!(
        r#""""Cross-check simbólico en Colab (sympy preinstalado). Imprime UNA línea JSON.
"""
import json
from sympy import symbols, sympify, diff, integrate, simplify
x = symbols("x")
expr = sympify({expression:?})
claim = sympify({claim:?})
check = {check:?}
if check == "derivative_of":
    verdict = simplify(diff(expr, x) - claim) == 0
elif check == "integral_of":
    verdict = simplify(diff(claim, x) - expr) == 0
else:
    verdict = simplify(expr - claim) == 0
print(json.dumps({{"verdict": bool(verdict), "check": check}}))
"#
    );
    Ok((
        script,
        "veredicto sympy = evidencia cruzada (débil): vale como segunda opinión, jamás como prueba"
            .into(),
    ))
}

// ── import_colab_result ──────────────────────────────────────────────

/// Chequea un modelo contra un DIMACS (puro, barato, fuerte).
fn check_model(cnf_text: &str, model: &[i64]) -> Result<bool, String> {
    let mut assign = std::collections::BTreeMap::new();
    for lit in model {
        if *lit == 0 {
            continue;
        }
        assign.insert(lit.abs(), *lit > 0);
    }
    for line in cnf_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('c') || line.starts_with('p') {
            continue;
        }
        let mut ok = false;
        for tok in line.split_whitespace() {
            let lit: i64 = tok
                .parse()
                .map_err(|_| "import: CNF corrupto en el manifiesto".to_string())?;
            if lit == 0 {
                break;
            }
            if assign.get(&lit.abs()) == Some(&(lit > 0)) {
                ok = true;
                break;
            }
        }
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

fn colab_ledger_path() -> std::path::PathBuf {
    let ledger = crate::ledger::ledger_path();
    if let Some(parent) = ledger.parent() {
        return parent.join("lab_colab.jsonl");
    }
    crate::ledger::cnf_dir().join("lab_colab.jsonl")
}

fn append_colab(entry: &Value) -> Result<(), String> {
    use std::io::Write;
    let path = colab_ledger_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("colab: no se pudo crear {parent:?}: {e}"))?;
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("colab: no se pudo abrir {path:?}: {e}"))?;
    writeln!(file, "{entry}").map_err(|e| format!("colab: no se pudo escribir: {e}"))?;
    Ok(())
}

pub fn import_colab_result(args: &Value, limits: &LabLimits) -> Result<Value, String> {
    let job_id = args
        .get("job_id")
        .and_then(Value::as_str)
        .ok_or("import_colab_result: 'job_id' requerido")?;
    let result = args
        .get("result")
        .and_then(Value::as_object)
        .ok_or("import_colab_result: 'result' debe ser objeto")?;
    let manifest = load_job(job_id)?;
    let kind = manifest.get("kind").and_then(Value::as_str).unwrap_or("?");
    let params = manifest.get("params").cloned().unwrap_or(Value::Null);
    let (ok, verification, detail) = match kind {
        "unit_sweep" => verify_unit_sweep(&params, result, limits)?,
        "sat_sweep" => verify_sat_sweep(&params, result)?,
        "cas_crosscheck" => {
            let verdict = result
                .get("verdict")
                .and_then(Value::as_bool)
                .ok_or("import: result.verdict booleano requerido")?;
            (
                verdict,
                "cross",
                if verdict {
                    "sympy confirma en Colab (segunda opinión, no prueba)".to_string()
                } else {
                    "sympy refuta en Colab: claim descartado".to_string()
                },
            )
        }
        _ => return Err("import: manifiesto con kind desconocido".into()),
    };
    let entry = json!({
        "kind": "colab",
        "job_id": job_id,
        "job_kind": kind,
        "origin": "colab",
        "verification": verification,
        "ok": ok,
        "detail": detail,
        "ts": crate::ledger::now_epoch_secs(),
    });
    append_colab(&entry)?;
    Ok(json!({
        "tool": "import_colab_result",
        "job_id": job_id,
        "ok": ok,
        "verification": verification,
        "detail": detail,
        "note": if ok {
            "registrado en lab_colab.jsonl con origen y nivel de verificación explícitos"
        } else {
            "NO es evidencia: registrado como refutación/dato, no como resultado"
        },
    }))
}

fn verify_unit_sweep(
    params: &Value,
    result: &serde_json::Map<String, Value>,
    limits: &LabLimits,
) -> Result<(bool, &'static str, String), String> {
    let runs = result
        .get("runs")
        .and_then(Value::as_array)
        .ok_or("import: result.runs requerido")?;
    if runs.is_empty() || runs.len() > MAX_COLAB_SEEDS {
        return Err("import: runs fuera de [1, 256]".into());
    }
    let family = crate::Family::parse(
        params
            .get("family")
            .and_then(Value::as_str)
            .unwrap_or("seeded"),
    )
    .map_err(|e| format!("import: {e}"))?;
    let n = params
        .get("n")
        .and_then(Value::as_u64)
        .ok_or("import: manifiesto sin n")? as usize;
    let scale = params.get("scale").and_then(Value::as_f64).unwrap_or(5.0);
    // Full-local solo si entra en topes Y es seeded (prefijo-compatible).
    if family == crate::Family::Seeded && n <= limits.max_points {
        let mut checked = 0usize;
        for run in runs {
            let seed = run
                .get("seed")
                .and_then(Value::as_u64)
                .ok_or("import: run sin seed")?;
            let claimed = run
                .get("unit")
                .and_then(Value::as_u64)
                .ok_or("import: run sin unit")? as usize;
            let pts: Vec<grafito_geometry::Point2> = regen_seeded(seed, n, scale)
                .into_iter()
                .map(|(x, y)| grafito_geometry::Point2::new(x, y))
                .collect();
            let fresh =
                crate::unit_pairs_spatial(&pts, 1e-9).map_err(|e| format!("import: {e}"))?;
            if fresh != claimed {
                return Ok((
                    false,
                    "mismatch",
                    format!("seed {seed}: Colab dice {claimed}, local mide {fresh} — refutado"),
                ));
            }
            if let Some(h) = run.get("points_hash").and_then(Value::as_str) {
                let mut bytes = Vec::with_capacity(pts.len() * 16);
                for p in &pts {
                    bytes.extend_from_slice(&p.x.to_le_bytes());
                    bytes.extend_from_slice(&p.y.to_le_bytes());
                }
                let local_h = format!("{:016x}", crate::fnv1a_64(&bytes));
                if local_h != h {
                    return Ok((
                        false,
                        "mismatch",
                        format!("seed {seed}: points_hash difiere — regen no idéntica"),
                    ));
                }
            }
            checked += 1;
        }
        return Ok((
            true,
            "full-local",
            format!("{checked} runs re-medidos en local, idénticos (incl. hash si vino)"),
        ));
    }
    Ok((
        true,
        "unverified",
        "n/familia fuera de re-medición local: dato archivado, NO es evidencia hasta verificar"
            .into(),
    ))
}

fn verify_sat_sweep(
    params: &Value,
    result: &serde_json::Map<String, Value>,
) -> Result<(bool, &'static str, String), String> {
    let cnfs = params
        .get("cnfs")
        .and_then(Value::as_array)
        .ok_or("import: manifiesto sin cnfs")?;
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .ok_or("import: result.results requerido")?;
    let mut by_name = std::collections::BTreeMap::new();
    for c in cnfs {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
        let text = c.get("text").and_then(Value::as_str).unwrap_or("");
        by_name.insert(name.to_string(), text.to_string());
    }
    let mut checked = 0usize;
    let mut trusted = 0usize;
    for r in results {
        let name = r.get("name").and_then(Value::as_str).unwrap_or("");
        let status = r.get("status").and_then(Value::as_str).unwrap_or("?");
        let text = by_name
            .get(name)
            .ok_or(format!("import: resultado para CNF desconocido '{name}'"))?;
        match status {
            "SAT" => {
                let model = r
                    .get("model")
                    .and_then(Value::as_array)
                    .ok_or(format!("import: '{name}' SAT sin model"))?;
                let mut lits = Vec::with_capacity(model.len());
                for m in model {
                    lits.push(
                        m.as_i64()
                            .ok_or(format!("import: '{name}' model no entero"))?,
                    );
                }
                if lits.len() > 2_000_000 {
                    return Err(format!("import: '{name}' model gigante, rechazado"));
                }
                if !check_model(text, &lits).map_err(|e| format!("import '{name}': {e}"))? {
                    return Ok((
                        false,
                        "mismatch",
                        format!("'{name}': el modelo de Colab NO satisface el CNF — refutado"),
                    ));
                }
                checked += 1;
            }
            "UNSAT" | "TIMEOUT" => trusted += 1,
            _ => return Err(format!("import: '{name}' status '{status}' inválido")),
        }
    }
    if checked > 0 && trusted == 0 {
        Ok((
            true,
            "model-checked",
            format!("{checked} modelos SAT verificados cláusula por cláusula en local"),
        ))
    } else if checked == 0 {
        Ok((
            true,
            "unverified",
            format!("{trusted} resultados a confianza del solver (UNSAT/TIMEOUT no re-chequeables sin prueba); dato, no evidencia"),
        ))
    } else {
        Ok((
            true,
            "model-checked",
            format!("{checked} SAT verificados en local + {trusted} a confianza del solver"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paridad_splitmix_pineada() {
        // golden 2026-09-20: seeded_point_set(42, 1, 5.0) del motor.
        let pts = regen_seeded(42, 1, 5.0);
        assert_eq!(pts.len(), 1);
        assert_eq!(
            format!("{:.16} {:.16}", pts[0].0, pts[0].1),
            "2.4156487877182347 -3.4008960712307985"
        );
    }

    #[test]
    fn defs_y_job_id_estable() {
        assert_eq!(colab_tool_defs().len(), 2);
        assert!(is_valid_job_id(&"b".repeat(64)));
        assert!(!is_valid_job_id("xyz"));
    }

    #[test]
    fn pii_se_rechaza() {
        assert!(reject_pii(r#"{"x": "a@b.com"}"#).is_err());
        assert!(reject_pii(r#"{"p": "/home/u/x"}"#).is_err());
        assert!(reject_pii(r#"{"e": "1+1"}"#).is_ok());
        // "a || b" style math con @ aislado no dispara
        assert!(reject_pii(r#"{"e": "f@g"}"#).is_ok());
    }

    #[test]
    fn unit_sweep_roundtrip_full_local() {
        let _guard = crate::ledger::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("grafito-colab-test");
        let _ = std::fs::create_dir_all(&dir);
        unsafe {
            std::env::set_var(
                "GRAFITO_LAB_LEDGER",
                dir.join("lab.jsonl").to_string_lossy().to_string(),
            );
        }
        let limits = LabLimits::default();
        let out = export_colab_job(
            &json!({"kind": "unit_sweep", "params": {"family": "seeded", "n": 10, "seeds": [7, 42]}}),
            &limits,
        )
        .unwrap();
        let job_id = out["job_id"].as_str().unwrap().to_string();
        assert!(out["script"].as_str().unwrap().contains("splitmix"));
        // Simula la vuelta de Colab calculando lo real en local.
        let mut runs = Vec::new();
        for seed in [7u64, 42] {
            let pts: Vec<grafito_geometry::Point2> = regen_seeded(seed, 10, 5.0)
                .into_iter()
                .map(|(x, y)| grafito_geometry::Point2::new(x, y))
                .collect();
            let unit = crate::unit_pairs_spatial(&pts, 1e-9).unwrap();
            runs.push(json!({"seed": seed, "n": 10, "unit": unit}));
        }
        let back = import_colab_result(
            &json!({"job_id": job_id, "result": {"runs": runs}}),
            &limits,
        )
        .unwrap();
        assert_eq!(back["ok"], json!(true));
        assert_eq!(back["verification"], json!("full-local"));
        // Mismatch se detecta.
        let bad = import_colab_result(
            &json!({"job_id": job_id, "result": {"runs": [{"seed": 7, "n": 10, "unit": 999999}]}}),
            &limits,
        )
        .unwrap();
        assert_eq!(bad["ok"], json!(false));
        unsafe {
            std::env::remove_var("GRAFITO_LAB_LEDGER");
        }
    }

    #[test]
    fn sat_model_se_chequea_en_local() {
        let cnf = "p cnf 2 2\n1 0\n-1 2 0\n";
        assert!(check_model(cnf, &[1, 2]).unwrap());
        assert!(!check_model(cnf, &[-1, -2]).unwrap());
        let out = export_colab_job(
            &json!({"kind": "sat_sweep", "params": {"cnfs": [{"name": "t", "text": cnf}], "timeout_s": 5}}),
            &LabLimits::default(),
        )
        .unwrap();
        assert!(out["script"].as_str().unwrap().contains("python-sat"));
    }

    #[test]
    fn cas_crosscheck_y_kind_invalido() {
        let out = export_colab_job(
            &json!({"kind": "cas_crosscheck", "params": {"expression": "x**2", "claim": "2*x", "check": "derivative_of"}}),
            &LabLimits::default(),
        )
        .unwrap();
        assert!(out["script"].as_str().unwrap().contains("sympy"));
        assert!(export_colab_job(
            &json!({"kind": "nope", "params": {}}),
            &LabLimits::default()
        )
        .is_err());
    }
}
