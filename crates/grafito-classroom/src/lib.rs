#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Grafito Classroom — aula colaborativa (Cerebro puro, sin egui/wgpu).
//!
//! Crate hoja del DAG: sin I/O, sin spawn, sin egui, testeable headless.
//! Provee [`TeacherDashboard`] para asistencia + dashboard de aprendizaje
//! accionable (Fase C2). No añade deps ni I/O: solo `serde`/`serde_json` ya
//! en workspace.
//!
//! # Roles
//! - [`TeacherDashboard`] (`lib.rs:659-698` original): asistencia
//!   (`code`, `present`, `hands`, `names`, `exercise`, `snapshot_digest`)
//!   extendida a métricas de aprendizaje accionables sin romper compat.
//! - [`LearnerSnapshot`] minimal `{ name, bkt_p_known, misconception_counts }`
//!   para desacoplar de `grafito-profile::StudentProfile` (crate distinta).
//!   Si el caller dispone de `StudentProfile`, mapea a `LearnerSnapshot`
//!   antes de llamar `from_live_with_profiles`.
//!
//! # Presupuestos (heredados)
//! - `MAX_DASHBOARD_NAMES = 5_000` (coherente con `MAX_OBJECT_COUNT`)
//! - `MAX_SNAPSHOT_DIGEST_LEN = 256`
//! - `MAX_MISCONCEPTION_KINDS = 32` (cap agregado)
//! - `MAX_BKT_SUMMARY_LEN = 128`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Límite de nombres en el dashboard (coherente con `MAX_OBJECT_COUNT 5000`).
pub const MAX_DASHBOARD_NAMES: usize = 5_000;
/// Límite de longitud para `snapshot_digest` (evita OOM en persistencia).
pub const MAX_SNAPSHOT_DIGEST_LEN: usize = 256;
/// Cap agregado de kinds de misconception en `top_misconceptions`.
pub const MAX_MISCONCEPTION_KINDS: usize = 32;
/// Cap de entradas en `bkt_summary` (evita Vec ilimitado).
pub const MAX_BKT_SUMMARY_LEN: usize = 128;

/// Snapshot mínimo de un aprendiz para alimentar el dashboard sin depender
/// de `grafito-profile` (evita ciclo de deps). Mapa 1:1 con `StudentProfile`
/// si el caller lo dispone: `bkt_p_known` es `BranchState::bkt_p_known` o
/// `mastery` medio; `misconception_counts` viene de `WorkingMemory`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnerSnapshot {
    /// Nombre display (trimmed, no vacío idealmente).
    pub name: String,
    /// Probabilidad latente BKT P(sabe) 0..=1 del alumno (media por rama/LO).
    pub bkt_p_known: f64,
    /// Conteo por misconception (`sign`, `fraction`, `chain_rule`...). Valores
    /// `usize` (no `u8`) para agregación sin saturar rápido.
    pub misconception_counts: HashMap<String, usize>,
    /// Ramas con repaso vencido (`next_review_epoch <= now`) para este alumno.
    /// `0` si no se dispone. Si hay dato, el dashboard lo suma; si no,
    /// lo infiere por `bkt_p_known < 0.6`.
    #[serde(default)]
    pub branches_due: usize,
}

impl LearnerSnapshot {
    /// Crea un snapshot mínimo con `misconception_counts` vacío y `branches_due = 0`.
    pub fn new(name: impl Into<String>, bkt_p_known: f64) -> Self {
        Self {
            name: name.into(),
            bkt_p_known: sanitize_bkt(bkt_p_known),
            misconception_counts: HashMap::new(),
            branches_due: 0,
        }
    }

    /// Builder: añade una misconception con conteo (se normaliza clave).
    #[must_use]
    pub fn with_misconception(mut self, kind: impl Into<String>, count: usize) -> Self {
        let key = normalize_misconception_key(&kind.into());
        if key.is_empty() || count == 0 {
            return self;
        }
        let entry = self.misconception_counts.entry(key).or_insert(0);
        *entry = entry.saturating_add(count);
        self
    }

    /// Builder: define `branches_due` explícito (sobrescribe inferencia).
    #[must_use]
    pub fn with_branches_due(mut self, due: usize) -> Self {
        self.branches_due = due;
        self
    }

    /// Builder: reemplaza `misconception_counts` completo (claves normalizadas).
    #[must_use]
    pub fn with_counts(mut self, counts: HashMap<String, usize>) -> Self {
        let mut normalized = HashMap::new();
        for (k, v) in counts {
            let key = normalize_misconception_key(&k);
            if key.is_empty() || v == 0 {
                continue;
            }
            let e: &mut usize = normalized.entry(key).or_insert(0_usize);
            *e = (*e).saturating_add(v);
        }
        self.misconception_counts = normalized;
        self
    }
}

impl Default for LearnerSnapshot {
    fn default() -> Self {
        Self {
            name: "Estudiante".to_string(),
            bkt_p_known: 0.3,
            misconception_counts: HashMap::new(),
            branches_due: 0,
        }
    }
}

/// Dashboard del docente: asistencia (compat) + métricas accionables (C2).
///
/// Campos originales (compat, `from_live`):
/// - `code`: código de aula (ej. `"GRAF-1234"`).
/// - `present`: cantidad de presentes.
/// - `hands`: manos levantadas.
/// - `names`: nombres display de los presentes (orden de `from_live`).
/// - `exercise`: ejercicio activo (prompt) si hay.
/// - `snapshot_digest`: hash/digest del snapshot del documento para auditoría.
///
/// Campos extendidos (C2, sin I/O):
/// - `bkt_summary`: BKT medio por LO del grupo (vec de `(lo_id|name, p)`).
///   Si el caller usa `LearnerSnapshot` por alumno, contiene `(name, bkt)`.
///   Orden alfabético estable.
/// - `top_misconceptions`: misconceptions agregadas del grupo, ordenadas
///   `count desc` + `key asc`, cap `MAX_MISCONCEPTION_KINDS`.
/// - `branches_due`: ramas vencidas totales del grupo (suma o inferencia).
/// - `avg_mastery`: dominio/BKT medio del grupo `0..=1` (NaN-safe).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeacherDashboard {
    /// Código de aula.
    pub code: String,
    /// Presentes (conteo).
    pub present: usize,
    /// Manos levantadas.
    pub hands: usize,
    /// Nombres de los presentes.
    pub names: Vec<String>,
    /// Ejercicio activo (si hay).
    pub exercise: Option<String>,
    /// Digest/hash del snapshot (hex o similar).
    pub snapshot_digest: String,
    /// BKT medio por LO (o por alumno) del grupo.
    pub bkt_summary: Vec<(String, f64)>,
    /// Misconceptions agregadas (top) del grupo.
    pub top_misconceptions: Vec<(String, usize)>,
    /// Ramas vencidas totales del grupo.
    pub branches_due: usize,
    /// Dominio medio del grupo 0..=1.
    pub avg_mastery: f64,
}

impl TeacherDashboard {
    /// Constructor de asistencia (compat). No toca `LearnerSnapshot`; los
    /// campos extendidos quedan en defaults vacíos/cero.
    ///
    /// Presupuesto: `names` se trunca a `MAX_DASHBOARD_NAMES`; `snapshot_digest`
    /// a `MAX_SNAPSHOT_DIGEST_LEN`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_live(
        code: String,
        present: usize,
        hands: usize,
        names: Vec<String>,
        exercise: Option<String>,
        snapshot_digest: String,
    ) -> Self {
        let code = sanitize_code(&code);
        let snapshot_digest = truncate_digest(&snapshot_digest);
        let names = sanitize_names(names);
        let exercise = exercise.and_then(|e| {
            let t = e.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t.chars().take(2000).collect())
            }
        });
        Self {
            code,
            present,
            hands,
            names,
            exercise,
            snapshot_digest,
            bkt_summary: Vec::new(),
            top_misconceptions: Vec::new(),
            branches_due: 0,
            avg_mastery: 0.0,
        }
    }

    /// Constructor extendido: asistencia + métricas accionables desde un slice
    /// de [`LearnerSnapshot`] (sin I/O, sin deps extra). Mantiene compat con
    /// `from_live` (delega y luego computa).
    ///
    /// Cómputo (puro, determinista, NaN-safe, O(n log n)):
    /// - `avg_mastery = mean(bkt_p_known.clamp 0..1)` (solo finitos; 0 si vacío/NaN).
    /// - `bkt_summary = [(name, bkt) ...]` orden alfabético, cap 128.
    /// - `branches_due = sum(snapshot.branches_due)` si alguno >0, si no
    ///   `count(bkt < 0.6)` como inferencia de “vencidas”.
    /// - `top_misconceptions = agregación HashMap<String,usize>` por
    ///   `misconception_counts` (claves normalizadas lowercase) → sort
    ///   `count desc, key asc` → cap 32.
    #[allow(clippy::too_many_arguments)]
    pub fn from_live_with_profiles(
        code: String,
        present: usize,
        hands: usize,
        names: Vec<String>,
        exercise: Option<String>,
        snapshot_digest: String,
        profiles: &[LearnerSnapshot],
    ) -> Self {
        let mut base = Self::from_live(code, present, hands, names, exercise, snapshot_digest);
        if profiles.is_empty() {
            return base;
        }

        // avg_mastery
        let mut sum = 0.0_f64;
        let mut valid = 0_usize;
        for p in profiles {
            let v = sanitize_bkt(p.bkt_p_known);
            if v.is_finite() {
                sum += v;
                valid = valid.saturating_add(1);
            }
        }
        let avg = if valid > 0 { sum / valid as f64 } else { 0.0 };
        base.avg_mastery = sanitize_bkt(avg);

        // bkt_summary: per-learner, orden estable alfabético.
        let mut summary: Vec<(String, f64)> = profiles
            .iter()
            .map(|p| {
                let v = sanitize_bkt(p.bkt_p_known);
                (sanitize_name(&p.name), v)
            })
            .collect();
        summary.sort_by(|a, b| a.0.cmp(&b.0));
        if summary.len() > MAX_BKT_SUMMARY_LEN {
            summary.truncate(MAX_BKT_SUMMARY_LEN);
        }
        base.bkt_summary = summary;

        // branches_due: suma explícita si hay dato, si no inferencia por BKT.
        let sum_due: usize = profiles.iter().map(|p| p.branches_due).sum();
        if sum_due > 0 {
            base.branches_due = sum_due;
        } else {
            let inferred = profiles
                .iter()
                .filter(|p| {
                    let v = sanitize_bkt(p.bkt_p_known);
                    v.is_finite() && v < 0.6
                })
                .count();
            base.branches_due = inferred;
        }

        // top_misconceptions: agregación.
        let mut agg: HashMap<String, usize> = HashMap::new();
        for p in profiles {
            for (k, v) in &p.misconception_counts {
                let key = normalize_misconception_key(k);
                if key.is_empty() || *v == 0 {
                    continue;
                }
                let entry = agg.entry(key).or_insert(0);
                *entry = entry.saturating_add(*v);
            }
        }
        let mut top: Vec<(String, usize)> = agg.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        if top.len() > MAX_MISCONCEPTION_KINDS {
            top.truncate(MAX_MISCONCEPTION_KINDS);
        }
        base.top_misconceptions = top;

        base
    }

    /// Presentes según `present` vs `names.len()` (útil para UI).
    #[must_use]
    pub fn effective_present(&self) -> usize {
        self.present.min(self.names.len()).min(MAX_DASHBOARD_NAMES)
    }

    /// Si el digest es vacío.
    #[must_use]
    pub fn has_snapshot(&self) -> bool {
        !self.snapshot_digest.is_empty()
    }
}

// ── helpers puros (sin I/O, sin unwrap) ─────────────────────────────────────

fn sanitize_bkt(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 1.0)
}

fn sanitize_code(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return "GRAF-0000".to_string();
    }
    // Mantener solo alfanumérico + '-' '_' , recortar a 32
    let filtered: String = t
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if filtered.is_empty() {
        "GRAF-0000".to_string()
    } else {
        filtered
    }
}

fn truncate_digest(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    t.chars().take(MAX_SNAPSHOT_DIGEST_LEN).collect()
}

fn sanitize_names(names: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for n in names {
        let t = n.trim().to_string();
        if t.is_empty() {
            continue;
        }
        // Cap por nombre 64 chars, evita OOM en UI
        let capped: String = t.chars().take(64).collect();
        out.push(capped);
        if out.len() >= MAX_DASHBOARD_NAMES {
            break;
        }
    }
    out
}

fn sanitize_name(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return "Estudiante".to_string();
    }
    t.chars().take(64).collect()
}

fn normalize_misconception_key(k: &str) -> String {
    let t = k.trim().to_lowercase();
    if t.is_empty() {
        return String::new();
    }
    // Solo letras, dígitos, '_' '-' ; colapsa espacios a '_'
    let mut out = String::new();
    for c in t.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('_');
        }
        if out.len() >= 64 {
            break;
        }
    }
    // Normaliza múltiplos '_' y recorta bordes
    let mut dedup = String::new();
    let mut prev_us = false;
    for ch in out.chars() {
        if ch == '_' {
            if !prev_us {
                dedup.push('_');
            }
            prev_us = true;
        } else {
            dedup.push(ch);
            prev_us = false;
        }
    }
    dedup.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── 8 tests de compat (asistencia) ─────────────────────────────────────

    #[test]
    fn from_live_basic_sets_fields() {
        let d = TeacherDashboard::from_live(
            "GRAF-1234".to_string(),
            2,
            1,
            vec!["Ana".to_string(), "Luis".to_string()],
            None,
            "abc123".to_string(),
        );
        assert_eq!(d.code, "GRAF-1234");
        assert_eq!(d.present, 2);
        assert_eq!(d.hands, 1);
        assert_eq!(d.names, vec!["Ana", "Luis"]);
        assert_eq!(d.exercise, None);
        assert_eq!(d.snapshot_digest, "abc123");
        // extendidos en default
        assert!(d.bkt_summary.is_empty());
        assert!(d.top_misconceptions.is_empty());
        assert_eq!(d.branches_due, 0);
        assert!((d.avg_mastery - 0.0).abs() < 1e-9);
    }

    #[test]
    fn from_live_empty_names_and_code_sanitize() {
        let d = TeacherDashboard::from_live(
            "   ".to_string(),
            0,
            0,
            vec!["   ".to_string(), "".to_string()],
            None,
            "".to_string(),
        );
        assert_eq!(d.code, "GRAF-0000");
        assert!(d.names.is_empty());
        assert!(!d.has_snapshot());
        assert_eq!(d.effective_present(), 0);
    }

    #[test]
    fn from_live_with_exercise_some() {
        let d = TeacherDashboard::from_live(
            "GRAF-A1".to_string(),
            1,
            0,
            vec!["Mia".to_string()],
            Some("  Resolver x+2=5  ".to_string()),
            "digest".to_string(),
        );
        assert_eq!(d.exercise.as_deref(), Some("Resolver x+2=5"));
    }

    #[test]
    fn from_live_without_exercise_trims_empty() {
        let d = TeacherDashboard::from_live(
            "GRAF-A1".to_string(),
            1,
            0,
            vec!["Mia".to_string()],
            Some("   ".to_string()),
            "digest".to_string(),
        );
        assert_eq!(d.exercise, None);
    }

    #[test]
    fn from_live_snapshot_digest_preserved_and_truncated() {
        let long = "a".repeat(500);
        let d = TeacherDashboard::from_live(
            "GRAF-1".to_string(),
            1,
            0,
            vec!["A".to_string()],
            None,
            long.clone(),
        );
        assert_eq!(d.snapshot_digest.len(), MAX_SNAPSHOT_DIGEST_LEN);
        assert!(d.has_snapshot());

        let d2 = TeacherDashboard::from_live(
            "GRAF-1".to_string(),
            1,
            0,
            vec!["A".to_string()],
            None,
            "  deadbeef  ".to_string(),
        );
        assert_eq!(d2.snapshot_digest, "deadbeef");
    }

    #[test]
    fn from_live_names_preserved_and_capped() {
        let names = vec![" Ana ".to_string(), "Luis".to_string(), "  ".to_string()];
        let d =
            TeacherDashboard::from_live("GRAF-1".to_string(), 3, 0, names, None, "d".to_string());
        assert_eq!(d.names, vec!["Ana", "Luis"]);
        assert_eq!(d.effective_present(), 2);
    }

    #[test]
    fn from_live_hands_count() {
        let d = TeacherDashboard::from_live(
            "GRAF-1".to_string(),
            5,
            3,
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            None,
            "x".to_string(),
        );
        assert_eq!(d.hands, 3);
        assert_eq!(d.present, 5);
    }

    #[test]
    fn from_live_serde_roundtrip() {
        let d = TeacherDashboard::from_live(
            "GRAF-XYZ".to_string(),
            1,
            0,
            vec!["Sol".to_string()],
            Some("ex".to_string()),
            "snap".to_string(),
        );
        let json = serde_json::to_string(&d).expect("serialize");
        let back: TeacherDashboard = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d, back);
        // extendidos también deben roundtrip aunque vacíos
        assert!(back.bkt_summary.is_empty());
    }

    // ── extendidos (C2) ────────────────────────────────────────────────────

    #[test]
    fn dashboard_extended_avg_mastery_tres_alumnos() {
        let profiles = vec![
            LearnerSnapshot::new("Ana", 0.2),
            LearnerSnapshot::new("Luis", 0.5),
            LearnerSnapshot::new("Mia", 0.9),
        ];
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            3,
            0,
            vec!["Ana".to_string(), "Luis".to_string(), "Mia".to_string()],
            None,
            "dig".to_string(),
            &profiles,
        );
        let expected = (0.2 + 0.5 + 0.9) / 3.0;
        assert!(
            (d.avg_mastery - expected).abs() < 1e-9,
            "avg {} vs expected {}",
            d.avg_mastery,
            expected
        );
        // bkt_summary debe contener los 3, orden alfabético
        assert_eq!(d.bkt_summary.len(), 3);
        assert_eq!(d.bkt_summary[0].0, "Ana");
        assert!((d.bkt_summary[0].1 - 0.2).abs() < 1e-9);
        assert_eq!(d.bkt_summary[1].0, "Luis");
        assert_eq!(d.bkt_summary[2].0, "Mia");
    }

    #[test]
    fn dashboard_extended_branches_due_inferido_por_bkt() {
        // Sin branches_due explícito, se infiere por bkt < 0.6
        let profiles = vec![
            LearnerSnapshot::new("A", 0.2),
            LearnerSnapshot::new("B", 0.5),
            LearnerSnapshot::new("C", 0.9),
        ];
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            3,
            0,
            vec![],
            None,
            "dig".to_string(),
            &profiles,
        );
        // 0.2 y 0.5 < 0.6 => 2 vencidas
        assert_eq!(d.branches_due, 2);

        // Si hay branches_due explícito, suma.
        let profiles2 = vec![
            LearnerSnapshot::new("A", 0.9).with_branches_due(2),
            LearnerSnapshot::new("B", 0.9).with_branches_due(1),
            LearnerSnapshot::new("C", 0.9).with_branches_due(0),
        ];
        let d2 = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            3,
            0,
            vec![],
            None,
            "dig".to_string(),
            &profiles2,
        );
        assert_eq!(d2.branches_due, 3);
    }

    #[test]
    fn dashboard_extended_top_misconception_agregado() {
        let mut c1 = HashMap::new();
        c1.insert("sign".to_string(), 2);
        c1.insert("fraction".to_string(), 1);
        let mut c2 = HashMap::new();
        c2.insert("sign".to_string(), 1);
        c2.insert("Sign".to_string(), 1); // debe normalizar a "sign"
        c2.insert("chain_rule".to_string(), 3);
        let mut c3 = HashMap::new();
        c3.insert("fraction".to_string(), 2);
        c3.insert("distributive".to_string(), 1);

        let profiles = vec![
            LearnerSnapshot::new("A", 0.4).with_counts(c1),
            LearnerSnapshot::new("B", 0.5).with_counts(c2),
            LearnerSnapshot::new("C", 0.6).with_counts(c3),
        ];
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            3,
            0,
            vec![],
            None,
            "dig".to_string(),
            &profiles,
        );
        // sign: 2 + 1+1 =4, fraction:1+2=3, chain_rule:3, distributive:1
        // top desc: sign 4, chain_rule 3, fraction 3 (alfabetico desempate)
        assert!(!d.top_misconceptions.is_empty());
        assert_eq!(d.top_misconceptions[0], ("sign".to_string(), 4));
        // Los siguientes pueden ser chain_rule y fraction empatados en 3; chain_rule < fraction alfabético
        assert!(d
            .top_misconceptions
            .iter()
            .any(|(k, v)| k == "chain_rule" && *v == 3));
        assert!(d
            .top_misconceptions
            .iter()
            .any(|(k, v)| k == "fraction" && *v == 3));
        assert!(d
            .top_misconceptions
            .iter()
            .any(|(k, v)| k == "distributive" && *v == 1));
    }

    #[test]
    fn dashboard_extended_bkt_summary_orden_y_clamp() {
        let profiles = vec![
            LearnerSnapshot::new("Zoe", f64::NAN),
            LearnerSnapshot::new("Ana", 1.5),   // clamp 1.0
            LearnerSnapshot::new("Luis", -0.3), // clamp 0.0
        ];
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            3,
            0,
            vec![],
            None,
            "dig".to_string(),
            &profiles,
        );
        // NaN -> 0.0
        assert!((d.avg_mastery - (0.0 + 1.0 + 0.0) / 3.0).abs() < 1e-9);
        // orden alfabético
        assert_eq!(d.bkt_summary[0].0, "Ana");
        assert!((d.bkt_summary[0].1 - 1.0).abs() < 1e-9);
        assert_eq!(d.bkt_summary[1].0, "Luis");
        assert!((d.bkt_summary[1].1 - 0.0).abs() < 1e-9);
        assert_eq!(d.bkt_summary[2].0, "Zoe");
        assert!((d.bkt_summary[2].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn dashboard_extended_empty_profiles_defaults() {
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-1".to_string(),
            0,
            0,
            vec![],
            None,
            "dig".to_string(),
            &[],
        );
        assert!(d.bkt_summary.is_empty());
        assert!(d.top_misconceptions.is_empty());
        assert_eq!(d.branches_due, 0);
        assert!((d.avg_mastery - 0.0).abs() < 1e-9);
    }

    #[test]
    fn dashboard_extended_serde_roundtrip_con_métricas() {
        let profiles = vec![
            LearnerSnapshot::new("Ana", 0.2).with_misconception("sign", 2),
            LearnerSnapshot::new("Luis", 0.5).with_misconception("fraction", 1),
            LearnerSnapshot::new("Mia", 0.9).with_misconception("sign", 1),
        ];
        let d = TeacherDashboard::from_live_with_profiles(
            "GRAF-99".to_string(),
            3,
            1,
            vec!["Ana".to_string()],
            Some("ejercicio 1".to_string()),
            "abc".to_string(),
            &profiles,
        );
        let json = serde_json::to_string(&d).expect("serialize");
        let back: TeacherDashboard = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(d, back);
        assert!((back.avg_mastery - 0.5333333333333333).abs() < 1e-9);
        assert_eq!(back.top_misconceptions[0], ("sign".to_string(), 3));
    }

    #[test]
    fn learner_snapshot_builders_normalizan() {
        let s = LearnerSnapshot::new("  Ana  ", 0.5)
            .with_misconception("  Sign  ", 1)
            .with_misconception("SIGN", 1)
            .with_misconception("   ", 5)
            .with_misconception("fraction", 0);
        assert_eq!(s.name, "  Ana  ");
        assert_eq!(s.misconception_counts.get("sign"), Some(&2));
        assert_eq!(s.misconception_counts.len(), 1);

        let s2 = LearnerSnapshot::new("Bob", 0.4).with_branches_due(3);
        assert_eq!(s2.branches_due, 3);
    }

    #[test]
    fn normalize_misconception_key_dedup() {
        assert_eq!(normalize_misconception_key("  Chain Rule  "), "chain_rule");
        assert_eq!(normalize_misconception_key("sign"), "sign");
        assert_eq!(normalize_misconception_key("  "), "");
        assert_eq!(normalize_misconception_key("a  b   c"), "a_b_c");
    }

    #[test]
    fn sanitize_code_filters() {
        assert_eq!(sanitize_code("GRAF-1234"), "GRAF-1234");
        assert_eq!(sanitize_code(" $$$ "), "GRAF-0000");
        assert_eq!(sanitize_code("a b c"), "abc");
    }
}
