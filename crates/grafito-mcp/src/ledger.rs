//! Ledger append-only de runs verificados + almacén de CNFs.
//!
//! - Ruta: `GRAFITO_LAB_LEDGER` si está seteada, si no
//!   `$XDG_DATA_HOME/grafito/lab_ledger.jsonl` (fallback `~/.local/share`).
//!   Mismo esquema que `grafito-app::usage_log` (precedente del repo).
//! - El server es el ÚNICO escritor: solo entra lo verificado por el motor.
//!   El LLM jamás escribe acá (anti-falsificación).
//! - CNFs y resultados SAT viven en `$XDG_DATA_HOME/grafito/lab_cnfs/`
//!   nombrados por `cnf_hash` (64 hex). Sin path traversal: el hash se valida.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Entrada del ledger (una línea JSONL, claves fijas, orden estable).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LabEntry {
    /// `topp39` hoy; reservado para `topp57`/`halving`/etc.
    pub kind: String,
    pub run_id: String,
    pub family: String,
    pub seed: u64,
    pub n: usize,
    pub scale: f64,
    pub unit: usize,
    pub distinct: usize,
    pub hash: u64,
    /// Segundos epoch.
    pub ts: u64,
}

impl LabEntry {
    /// Serializa a JSONL con 6 decimales en `scale` (estable, diffriendly).
    pub fn to_jsonl(&self) -> String {
        serde_json::json!({
            "kind": self.kind,
            "run_id": self.run_id,
            "family": self.family,
            "seed": self.seed,
            "n": self.n,
            "scale": format!("{:.6}", self.scale),
            "unit": self.unit,
            "distinct": self.distinct,
            "hash": self.hash,
            "ts": self.ts,
        })
        .to_string()
    }
}

/// Resolución pura de la ruta (testeable sin entorno global).
pub fn ledger_path_from(
    explicit: Option<&str>,
    xdg_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> PathBuf {
    if let Some(raw) = explicit {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let base = xdg_data_home
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let mut h = home.unwrap_or_default();
            if h.as_os_str().is_empty() {
                h = PathBuf::from(".");
            }
            h.push(".local/share");
            h
        });
    base.join("grafito").join("lab_ledger.jsonl")
}

/// Ruta efectiva desde el entorno real.
pub fn ledger_path() -> PathBuf {
    ledger_path_from(
        std::env::var("GRAFITO_LAB_LEDGER").ok().as_deref(),
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// Directorio de CNFs y resultados (`.../grafito/lab_cnfs/`).
pub fn cnf_dir_from(xdg_data_home: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    let base = xdg_data_home
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let mut h = home.unwrap_or_default();
            if h.as_os_str().is_empty() {
                h = PathBuf::from(".");
            }
            h.push(".local/share");
            h
        });
    base.join("grafito").join("lab_cnfs")
}

/// Directorio efectivo desde el entorno.
pub fn cnf_dir() -> PathBuf {
    // Si el ledger es explícito, los CNFs van junto a él (carpeta hermana).
    if let Ok(raw) = std::env::var("GRAFITO_LAB_LEDGER") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if let Some(parent) = p.parent() {
                return parent.join("lab_cnfs");
            }
        }
    }
    cnf_dir_from(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// Agrega una entrada verificada (crea directorios padre). Solo el server llama.
pub fn append_entry(entry: &LabEntry) -> Result<(), String> {
    let path = ledger_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("ledger: no se pudo crear {parent:?}: {e}"))?;
        }
    }
    if entry.run_id.trim().is_empty() || !crate::is_valid_run_id(&entry.run_id) {
        return Err("ledger: run_id inválido, no se registra".into());
    }
    let line = entry.to_jsonl();
    append_line(&path, &line)
}

fn append_line(path: &std::path::Path, line: &str) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("ledger: no se pudo abrir {path:?}: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("ledger: no se pudo escribir: {e}"))?;
    Ok(())
}

/// Lee todas las entradas (ignora líneas corruptas, las cuenta).
pub fn read_all() -> (Vec<LabEntry>, usize) {
    let path = ledger_path();
    read_all_from(&path)
}

fn read_all_from(path: &std::path::Path) -> (Vec<LabEntry>, usize) {
    let mut out = Vec::new();
    let mut corrupt = 0usize;
    let Ok(text) = std::fs::read_to_string(path) else {
        return (out, 0);
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<LabEntryCompat>(line) {
            Ok(c) => out.push(c.into_entry()),
            Err(_) => corrupt += 1,
        }
    }
    (out, corrupt)
}

/// Compat: acepta `scale` como número o string "x.xxxxxx".
#[derive(Debug, Deserialize)]
struct LabEntryCompat {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    family: String,
    #[serde(default)]
    seed: u64,
    #[serde(default)]
    n: usize,
    #[serde(default)]
    scale: serde_json::Value,
    #[serde(default)]
    unit: usize,
    #[serde(default)]
    distinct: usize,
    #[serde(default)]
    hash: u64,
    #[serde(default)]
    ts: u64,
}

impl LabEntryCompat {
    fn into_entry(self) -> LabEntry {
        let scale = match &self.scale {
            serde_json::Value::Number(v) => v.as_f64().unwrap_or(5.0),
            serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(5.0),
            _ => 5.0,
        };
        LabEntry {
            kind: if self.kind.is_empty() {
                "topp39".into()
            } else {
                self.kind
            },
            run_id: self.run_id,
            family: self.family,
            seed: self.seed,
            n: self.n,
            scale,
            unit: self.unit,
            distinct: self.distinct,
            hash: self.hash,
            ts: self.ts,
        }
    }
}

/// Busca una entrada por `run_id` (barrido lineal, el ledger es chico).
pub fn find_run(run_id: &str) -> Option<LabEntry> {
    let (all, _) = read_all();
    all.into_iter().find(|e| e.run_id == run_id)
}

/// Guarda un CNF por hash (valida el hash, crea el dir).
pub fn store_cnf(cnf_hash: &str, cnf: &str) -> Result<PathBuf, String> {
    if !crate::is_valid_cnf_hash(cnf_hash) {
        return Err("cnf_hash inválido (se esperan 64 hex)".into());
    }
    let dir = cnf_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cnf: no se pudo crear {dir:?}: {e}"))?;
    let path = dir.join(format!("{cnf_hash}.cnf"));
    std::fs::write(&path, cnf).map_err(|e| format!("cnf: no se pudo guardar: {e}"))?;
    Ok(path)
}

/// Lee un CNF por hash.
pub fn load_cnf(cnf_hash: &str) -> Result<String, String> {
    if !crate::is_valid_cnf_hash(cnf_hash) {
        return Err("cnf_hash inválido (se esperan 64 hex)".into());
    }
    let path = cnf_dir().join(format!("{cnf_hash}.cnf"));
    std::fs::read_to_string(&path)
        .map_err(|_| "cnf no encontrado en el almacén; regeneralo con export_dimacs".to_string())
}

/// Guarda el último resultado SAT de un CNF.
pub fn store_result(cnf_hash: &str, payload: &serde_json::Value) -> Result<(), String> {
    if !crate::is_valid_cnf_hash(cnf_hash) {
        return Err("cnf_hash inválido (se esperan 64 hex)".into());
    }
    let dir = cnf_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("result: no se pudo crear {dir:?}: {e}"))?;
    let path = dir.join(format!("{cnf_hash}.result.json"));
    let text = serde_json::to_string(payload)
        .map_err(|e| format!("result: no se pudo serializar: {e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("result: no se pudo guardar: {e}"))?;
    Ok(())
}

/// Lee el último resultado SAT de un CNF (si existe).
pub fn load_result(cnf_hash: &str) -> Option<serde_json::Value> {
    if !crate::is_valid_cnf_hash(cnf_hash) {
        return None;
    }
    let path = cnf_dir().join(format!("{cnf_hash}.result.json"));
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Candado global para tests que mutan `GRAFITO_LAB_LEDGER` (el env es global
/// por proceso y los tests corren en hilos; sin esto hay carreras).
#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_por_defecto_y_explicitas() {
        let p = ledger_path_from(None, Some(PathBuf::from("/tmp/xdg")), None);
        assert_eq!(p, PathBuf::from("/tmp/xdg/grafito/lab_ledger.jsonl"));
        let p2 = ledger_path_from(None, None, Some(PathBuf::from("/home/u")));
        assert_eq!(
            p2,
            PathBuf::from("/home/u/.local/share/grafito/lab_ledger.jsonl")
        );
        let p3 = ledger_path_from(Some("/tmp/mi.jsonl"), None, None);
        assert_eq!(p3, PathBuf::from("/tmp/mi.jsonl"));
        let d = cnf_dir_from(Some(PathBuf::from("/tmp/xdg")), None);
        assert_eq!(d, PathBuf::from("/tmp/xdg/grafito/lab_cnfs"));
    }

    #[test]
    fn compat_acepta_scale_numero_o_string() {
        let a: LabEntryCompat = serde_json::from_str(r#"{"run_id":"x","scale":5}"#).unwrap();
        assert_eq!(a.into_entry().scale, 5.0);
        let b: LabEntryCompat =
            serde_json::from_str(r#"{"run_id":"x","scale":"5.000000"}"#).unwrap();
        assert_eq!(b.into_entry().scale, 5.0);
    }
}
