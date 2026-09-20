//! Sidecar SAT honesto (`kissat` / `cadical`).
//!
//! - Detecta el binario en PATH (`which`-like manual, sin deps).
//! - Si falta: error `SolverMissing` con guía de instalación (como
//!   `FfmpegMissing` del puente de animación). Jamás inventa SAT/UNSAT.
//! - Timeout con `try_wait` + polling (solo std, sin crates nuevos).
//! - Parsea stdout: `s SATISFIABLE` / `s UNSATISFIABLE` (kissat y cadical),
//!   `v ...` como modelo. Sin proof-checking (se guarda el stdout crudo).

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Solver soportado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Solver {
    Kissat,
    Cadical,
}

impl Solver {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_lowercase().as_str() {
            "kissat" => Ok(Self::Kissat),
            "cadical" => Ok(Self::Cadical),
            otra => Err(format!(
                "solver '{otra}' desconocido: elegí kissat o cadical"
            )),
        }
    }

    pub fn binary(self) -> &'static str {
        match self {
            Self::Kissat => "kissat",
            Self::Cadical => "cadical",
        }
    }
}

/// Resultado de un `sat_check`.
#[derive(Debug, Clone)]
pub struct SatOutcome {
    pub status: &'static str,
    pub model: Option<Vec<i64>>,
    pub time_ms: u64,
    pub solver: &'static str,
    pub stdout_tail: String,
}

/// Busca un binario en PATH (manual, sin `which`).
pub fn find_in_path(binary: &str) -> Option<PathBuf> {
    if binary.contains('/') || binary.contains('\0') {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(binary);
        if let Ok(meta) = std::fs::metadata(&cand) {
            if meta.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = meta.permissions().mode();
                    if mode & 0o111 != 0 {
                        return Some(cand);
                    }
                }
                #[cfg(not(unix))]
                {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// Guía de instalación honesta por solver.
pub fn install_hint(solver: Solver) -> String {
    match solver {
        Solver::Kissat => "instalá kissat (https://github.com/arminbiere/kissat: `git clone && ./configure && make`) y ponelo en tu PATH".to_string(),
        Solver::Cadical => "instalá cadical (https://github.com/arminbiere/cadical: `./configure && make`) y ponelo en tu PATH".to_string(),
    }
}

/// Corre el solver sobre un archivo CNF con timeout.
///
/// `timeout_ms == 0` = sin timeout (solo a pedido explícito; el ledger lo
/// anota). Devuelve `TIMEOUT` si se agota, matando al hijo.
pub fn run_solver(
    solver: Solver,
    cnf_path: &std::path::Path,
    timeout_ms: u64,
) -> Result<SatOutcome, String> {
    let binary = find_in_path(solver.binary()).ok_or_else(|| {
        format!(
            "solver ausente ({}): {}; Grafito no declara coloreabilidad sin SAT externo",
            solver.binary(),
            install_hint(solver)
        )
    })?;
    let start = std::time::Instant::now();
    let mut child = Command::new(&binary)
        .arg(cnf_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar {}: {e}", solver.binary()))?;

    // Espera con polling cada 25 ms (solo std).
    let timeout = if timeout_ms == 0 {
        None
    } else {
        Some(std::time::Duration::from_millis(timeout_ms))
    };
    let status_opt = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if let Some(t) = timeout {
                    if start.elapsed() >= t {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(format!("error esperando al solver: {e}")),
        }
    };
    let elapsed_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let Some(_status) = status_opt else {
        return Ok(SatOutcome {
            status: "TIMEOUT",
            model: None,
            time_ms: elapsed_ms,
            solver: solver.binary(),
            stdout_tail: String::new(),
        });
    };
    let output = child
        .wait_with_output()
        .map_err(|e| format!("error leyendo al solver: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (status, model) = parse_sat_output(&stdout);
    let tail: String = stdout
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let tail: String = tail.chars().take(4000).collect();
    Ok(SatOutcome {
        status,
        model,
        time_ms: elapsed_ms,
        solver: solver.binary(),
        stdout_tail: tail,
    })
}

/// Parsea `s SATISFIABLE` / `s UNSATISFIABLE` + líneas `v`.
fn parse_sat_output(stdout: &str) -> (&'static str, Option<Vec<i64>>) {
    let mut status: &'static str = "TIMEOUT";
    let mut model: Vec<i64> = Vec::new();
    for line in stdout.lines() {
        let t = line.trim();
        if t.starts_with('s') {
            let low = t.to_lowercase();
            if low.contains("unsat") {
                status = "UNSAT";
            } else if low.contains("sat") {
                status = "SAT";
            }
        } else if let Some(stripped) = t.strip_prefix('v') {
            for tok in stripped.split_whitespace() {
                if let Ok(lit) = tok.parse::<i64>() {
                    if lit == 0 {
                        continue;
                    }
                    if model.len() < 1_000_000 {
                        model.push(lit);
                    }
                }
            }
        }
    }
    if status == "SAT" {
        (status, Some(model))
    } else if status == "UNSAT" {
        (status, None)
    } else {
        ("TIMEOUT", None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_sat_unsat_timeout() {
        let (s, m) = parse_sat_output("c hola\ns SATISFIABLE\nv 1 -2 3 0\n");
        assert_eq!(s, "SAT");
        assert_eq!(m.unwrap(), vec![1, -2, 3]);
        let (s2, m2) = parse_sat_output("s UNSATISFIABLE\n");
        assert_eq!(s2, "UNSAT");
        assert!(m2.is_none());
        let (s3, _) = parse_sat_output("c nada\n");
        assert_eq!(s3, "TIMEOUT");
    }

    #[test]
    fn solver_desconocido_da_guia() {
        assert!(Solver::parse("minisat").is_err());
        assert_eq!(Solver::parse("KISSAT").unwrap(), Solver::Kissat);
    }

    #[test]
    fn binario_con_slash_se_rechaza() {
        assert!(find_in_path("../../bin/sh").is_none());
    }
}
