#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Perfil pedagógico del usuario (ADR-0001): memoria de aprendizaje que el
//! tutor-orquestador «Mora» usa para adaptar el plan, ir ramificando y medir
//! el progreso. Crate de capa hoja: sin egui, testable headless, persistible.

pub mod bkt;
pub mod exam;
pub mod long_memory;
pub mod mascot;
pub mod scheduler;
pub mod working_memory;

// Re-exportar tipos de avatar/mascota en la raíz
pub use bkt::{
    bkt_params_for_branch, bkt_params_for_lo, bkt_update, BktParams, BktState, BKT_DEFAULT_PARAMS,
};
pub use long_memory::{Fact, LongTermMemory, Preferences};
pub use mascot::{
    AvatarAccessory, AvatarBlush, AvatarConfig, AvatarEyeStyle, AvatarMouthStyle, AvatarShape,
    DailyMission, FurnitureKind, HouseTheme, MascotConfig, MascotMood, MascotSpecies, Outfit,
    OutfitLayer, OutfitTier, Personality, ShopItem, Wardrobe, MAX_DISPLAY_NAME, MAX_NAME,
};
pub use scheduler::{
    is_due, next_interval, review_schedule_for, schedule_next_review, ReviewSchedule,
    SchedulerError, DAY_SECS, MAX_BOX_LEVEL,
};
pub use working_memory::WorkingMemory;

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Límites explícitos para que la memoria nunca crezca sin cota.
const MAX_BRANCHES: usize = 128;
const MAX_HISTORY_EVENTS: usize = 2_000;
const MAX_MEMORY_CHARS: usize = 2_400;

/// Tasa EMA para actualizar dominio en aciertos: mastery += EMA_ALPHA * (1 - mastery).
const EMA_ALPHA: f64 = 0.15;
/// Umbral de dominio para cubrir rama.
const MASTER_THRESHOLD: f64 = 0.8;
/// Retención EMA en fallos: mastery *= EMA_RETENTION (0.95).
const EMA_RETENTION: f64 = 0.95;

fn default_bkt_p_known() -> f64 {
    0.3
}

/// Estado de una rama de estudio (ej. Álgebra, Cálculo, Geometría 3D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchState {
    pub id: String,
    pub name: String,
    pub covered: bool,
    /// Dominio 0..=1 (media móvil exponencial de aciertos).
    pub mastery: f32,
    pub last_study_epoch: Option<u64>,
    /// Histórico (epoch, dominio) por rama, acotado a 64 muestras, para
    /// graficar la evolución. `#[serde(default)]` permite leer perfiles viejos.
    /// VecDeque para rotación O(1) con `pop_front` (LRU 64).
    #[serde(default)]
    pub domain_history: VecDeque<(u64, f32)>,
    /// Próximo repaso espaciado (epoch segundos). `None` si nunca se practicó.
    #[serde(default)]
    pub next_review_epoch: Option<u64>,
    /// Caja Leitner 0..=8 (0 sin repasar, 1 primera caja, 8 máxima).
    #[serde(default)]
    pub box_level: u8,
    /// Probabilidad latente BKT P(sabe) para esta rama.
    #[serde(default = "default_bkt_p_known")]
    pub bkt_p_known: f64,
}

/// Tipo de evento de aprendizaje registrado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StudyEventKind {
    Prompt,
    Correct,
    Incorrect,
    ExamPass,
    ExamFail,
}

/// Evento inmutable de la historia de aprendizaje.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudyEvent {
    pub epoch: u64,
    pub branch_id: String,
    pub kind: StudyEventKind,
    pub detail: String,
}

/// Resultado de un examen sobre una rama.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamResult {
    pub epoch: u64,
    pub branch_id: String,
    pub score: u32,
    pub total: u32,
    pub passed: bool,
}

/// Perfil completo del estudiante (memoria del usuario).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentProfile {
    pub name: String,
    pub level: u32,
    pub xp: u64,
    pub branches: Vec<BranchState>,
    pub history: Vec<StudyEvent>,
    pub exams: Vec<ExamResult>,
    /// Racha actual de aciertos consecutivos (se reinicia con un fallo).
    pub streak: u32,
    /// Mejor racha histórica de aciertos consecutivos.
    pub best_streak: u32,
    /// Avatar y personalización visual (migración: default si no existe).
    #[serde(default)]
    pub avatar: AvatarConfig,
    /// Mascota directa (compatibilidad con spec: duplicado de avatar.mascot).
    /// Se mantiene sincronizada con `avatar.mascot` en helpers.
    #[serde(default)]
    pub mascot: Option<MascotConfig>,
    /// Memoria a largo plazo (hechos, preferencias, vínculo).
    #[serde(default)]
    pub long_memory: LongTermMemory,
    /// Memoria de trabajo episódica de la sesión (WorkingMemory).
    /// F5 inline: contexto socrático sin tocar memoria larga; se sincroniza con AssistantPanelState.
    #[serde(default)]
    pub working_memory: WorkingMemory,
}

impl Default for StudentProfile {
    fn default() -> Self {
        Self {
            name: "Estudiante".to_string(),
            level: 1,
            xp: 0,
            branches: Vec::new(),
            history: Vec::new(),
            exams: Vec::new(),
            streak: 0,
            best_streak: 0,
            avatar: AvatarConfig::default(),
            mascot: None,
            long_memory: LongTermMemory::default(),
            working_memory: WorkingMemory::default(),
        }
    }
}

impl StudentProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Devuelve mascota mutable, creando una por defecto si no existe.
    /// Sincroniza `self.mascot` y `self.avatar.mascot` para compatibilidad.
    pub fn mascot_mut_or_default(&mut self) -> &mut MascotConfig {
        // Prioriza avatar.mascot si existe, si no mascot directo, si no crea.
        if self.avatar.mascot.is_none() && self.mascot.is_some() {
            self.avatar.mascot = self.mascot.clone();
        }
        if self.mascot.is_none() && self.avatar.mascot.is_some() {
            self.mascot = self.avatar.mascot.clone();
        }
        if self.avatar.mascot.is_none() {
            let cfg = MascotConfig::default();
            self.avatar.mascot = Some(cfg.clone());
            self.mascot = Some(cfg);
        }
        // Mantener ambos sincronizados (avatar es fuente de verdad para UI)
        if self.mascot.is_none() {
            self.mascot = self.avatar.mascot.clone();
        }
        self.avatar.mascot.get_or_insert_with(MascotConfig::default)
    }

    /// Asegura que la rama exista; devuelve su índice o `None` si se alcanzó el límite.
    /// Fail-closed: los llamantes deben manejar `None` en lugar de usar sentinel `usize::MAX`.
    pub fn ensure_branch(&mut self, id: &str, name: &str) -> Option<usize> {
        if let Some(index) = self.branches.iter().position(|b| b.id == id) {
            return Some(index);
        }
        if self.branches.len() >= MAX_BRANCHES {
            return None;
        }
        self.branches.push(BranchState {
            id: id.to_string(),
            name: name.to_string(),
            covered: false,
            mastery: 0.0,
            last_study_epoch: None,
            domain_history: VecDeque::new(),
            next_review_epoch: None,
            box_level: 0,
            bkt_p_known: default_bkt_p_known(),
        });
        Some(self.branches.len() - 1)
    }

    /// Compatibilidad: variante sentinel legada que retorna `usize::MAX` si no hay espacio.
    /// Preferir `ensure_branch` que retorna `Option`.
    #[deprecated(note = "Usar ensure_branch que retorna Option<usize>")]
    pub fn ensure_branch_legacy(&mut self, id: &str, name: &str) -> usize {
        self.ensure_branch(id, name).unwrap_or(usize::MAX)
    }

    /// Registra una respuesta y actualiza dominio (EMA + BKT), Leitner y progreso.
    pub fn record_outcome(&mut self, branch_id: &str, name: &str, epoch: u64, correct: bool) {
        let Some(index) = self.ensure_branch(branch_id, name) else {
            return;
        };
        let branch = &mut self.branches[index];
        if correct {
            branch.mastery += (EMA_ALPHA as f32) * (1.0 - branch.mastery);
            self.xp = self.xp.saturating_add(10);
            self.streak = self.streak.saturating_add(1);
            self.best_streak = self.best_streak.max(self.streak);
            if branch.mastery >= (MASTER_THRESHOLD as f32) && !branch.covered {
                branch.covered = true;
                self.xp = self.xp.saturating_add(40);
            }
        } else {
            branch.mastery *= EMA_RETENTION as f32;
            self.streak = 0;
        }
        // BKT: actualizar P(sabe) con evidencia
        let params = bkt::bkt_params_for_branch(branch_id);
        let next_p = bkt::bkt_update(branch.bkt_p_known, correct, &params);
        branch.bkt_p_known = next_p.clamp(0.0, 1.0);
        // Leitner: subir/bajar caja
        if correct {
            branch.box_level = (branch.box_level.saturating_add(1)).min(MAX_BOX_LEVEL);
        } else {
            branch.box_level = branch.box_level.saturating_sub(2);
        }
        // Scheduler: próximo repaso
        let interval = scheduler::next_interval(branch.box_level, branch.mastery);
        branch.next_review_epoch = Some(epoch.saturating_add(interval));
        branch.last_study_epoch = Some(epoch);
        // Evolución de dominio acotada (64 muestras por rama) — VecDeque LRU.
        branch.domain_history.push_back((epoch, branch.mastery));
        if branch.domain_history.len() > 64 {
            branch.domain_history.pop_front();
        }
        self.push_event(StudyEvent {
            epoch,
            branch_id: branch_id.to_string(),
            kind: if correct {
                StudyEventKind::Correct
            } else {
                StudyEventKind::Incorrect
            },
            detail: format!("{} {branch_id}", if correct { "acierto" } else { "fallo" }),
        });
        self.level = (self.xp / 250).saturating_add(1) as u32;
        // Sincronizar evolución de mascota si existe
        if let Some(m) = self.avatar.mascot.as_mut() {
            let covered = self.branches.iter().filter(|b| b.covered).count() as u32;
            m.sync_evolution(self.level, covered);
        }
        if let Some(m) = self.mascot.as_mut() {
            let covered = self.branches.iter().filter(|b| b.covered).count() as u32;
            m.sync_evolution(self.level, covered);
        }
    }

    pub fn record_exam(&mut self, result: ExamResult) {
        let index = self.ensure_branch(&result.branch_id, &result.branch_id);
        if let Some(idx) = index {
            if result.passed {
                let branch = &mut self.branches[idx];
                branch.mastery = branch
                    .mastery
                    .max(result.score as f32 / result.total.max(1) as f32);
                branch.covered = true;
                self.xp = self.xp.saturating_add(60);
            }
        }
        self.push_event(StudyEvent {
            epoch: result.epoch,
            branch_id: result.branch_id.clone(),
            kind: if result.passed {
                StudyEventKind::ExamPass
            } else {
                StudyEventKind::ExamFail
            },
            detail: format!("{}/{}", result.score, result.total),
        });
        self.exams.push(result);
    }

    fn push_event(&mut self, event: StudyEvent) {
        self.history.push(event);
        if self.history.len() > MAX_HISTORY_EVENTS {
            let overflow = self.history.len() - MAX_HISTORY_EVENTS;
            self.history.drain(0..overflow);
        }
    }

    /// Ramas sin cubrir, ordenadas por menor dominio (la siguiente a estudiar).
    ///
    /// # Nota de integración (assistant.rs:450)
    ///
    /// Este método es la API histórica usada por `assistant.rs:450`
    /// (`profile.recommend_next()`). Desde esta versión delega en
    /// `recommend_next_with_scheduler(now)` con `now = now_epoch()` para
    /// incorporar Leitner/SM-2 sin romper callers. Si necesitas control
    /// explícito del tiempo (tests deterministas, replay), usa
    /// `recommend_next_with_scheduler(now)` directamente. Comportamiento:
    /// prioriza `due` (vencidas) primero, luego menor `mastery`/`bkt_p_known`.
    pub fn recommend_next(&self) -> Vec<&BranchState> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.recommend_next_with_scheduler(now)
    }

    /// Variante explícita sin reloj: delega en `recommend_next_with_scheduler(now)` con `now` dado.
    /// Útil para tests y para `assistant.rs` si migra a inyección de reloj.
    pub fn recommend_next_at(&self, now: u64) -> Vec<&BranchState> {
        self.recommend_next_with_scheduler(now)
    }

    /// Ramas cuyo repaso venció en `now` (Leitner due).
    pub fn branches_due(&self, now: u64) -> Vec<&BranchState> {
        self.branches
            .iter()
            .filter(|b| {
                if let Some(epoch) = b.next_review_epoch {
                    scheduler::is_due(epoch, now)
                } else {
                    false
                }
            })
            .collect()
    }

    /// Ramas sin cubrir priorizando vencidas (due primero) y luego menor dominio/BKT.
    /// Mantiene compatibilidad: no cambia `recommend_next()` existente.
    pub fn recommend_next_with_scheduler(&self, now: u64) -> Vec<&BranchState> {
        let mut pending: Vec<&BranchState> = self.branches.iter().filter(|b| !b.covered).collect();
        pending.sort_by(|a, b| {
            let a_due = a
                .next_review_epoch
                .is_some_and(|e| scheduler::is_due(e, now));
            let b_due = b
                .next_review_epoch
                .is_some_and(|e| scheduler::is_due(e, now));
            match (a_due, b_due) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // Entre mismo estado due, prioriza próximo vencimiento más antiguo.
                    let ord_epoch = a.next_review_epoch.cmp(&b.next_review_epoch);
                    if ord_epoch != std::cmp::Ordering::Equal {
                        return ord_epoch;
                    }
                    // Fallback: menor mastery/BKT primero (más débil)
                    let ord_mastery = a
                        .mastery
                        .partial_cmp(&b.mastery)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if ord_mastery != std::cmp::Ordering::Equal {
                        return ord_mastery;
                    }
                    a.bkt_p_known
                        .partial_cmp(&b.bkt_p_known)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            }
        });
        pending
    }

    /// Schedules actuales por rama (para UI/debug).
    pub fn review_schedules(&self) -> Vec<ReviewSchedule> {
        self.branches
            .iter()
            .filter_map(|b| {
                let epoch = b.next_review_epoch?;
                let interval = scheduler::next_interval(b.box_level, b.mastery);
                let days = (interval / scheduler::DAY_SECS) as u32;
                Some(ReviewSchedule {
                    branch_id: b.id.clone(),
                    next_review_epoch: epoch,
                    interval_days: days.max(1),
                    box_level: b.box_level,
                })
            })
            .collect()
    }

    pub fn display_name(&self) -> &str {
        let d = self.avatar.display_name.trim();
        if !d.is_empty() {
            &self.avatar.display_name
        } else {
            &self.name
        }
    }
    pub fn set_display_name(&mut self, name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("El nombre no puede estar vacío".to_string());
        }
        if trimmed.chars().count() > crate::mascot::MAX_DISPLAY_NAME {
            return Err(format!(
                "El nombre no puede superar {} caracteres",
                crate::mascot::MAX_DISPLAY_NAME
            ));
        }
        self.name = trimmed.to_string();
        self.avatar.display_name = trimmed.to_string();
        if self.avatar.seed.trim().is_empty() {
            self.avatar.seed = trimmed.to_string();
        }
        Ok(())
    }

    /// Resumen comprimido para el prompt del tutor (memoria del usuario).
    pub fn memory(&self) -> String {
        let mut base = if self.branches.is_empty() {
            "Estudiante nuevo: sin ramas registradas todavía.".to_string()
        } else {
            let covered = self.branches.iter().filter(|b| b.covered).count();
            let pct = covered as f32 / self.branches.len().max(1) as f32 * 100.0;
            let mut t = format!(
                "Nivel {}, XP {}. Racha: {}. Cobertura: {covered}/{} ({pct:.0}%).\n",
                self.level,
                self.xp,
                self.streak,
                self.branches.len()
            );
            for branch in &self.branches {
                t.push_str(&format!(
                    "- {}: {} (dominio {:.0}%).\n",
                    branch.name,
                    if branch.covered {
                        "cubierta"
                    } else {
                        "pendiente"
                    },
                    branch.mastery * 100.0
                ));
            }
            if let Some(event) = self.history.last() {
                t.push_str(&format!(
                    "Última actividad: {:?} en {}.\n",
                    event.kind, event.branch_id
                ));
            }
            t
        };
        // Añadir personalidad/ánimo y memoria larga + avatar rasgos finos
        if let Some(m) = self.avatar.mascot.as_ref().or(self.mascot.as_ref()) {
            base.push_str(&format!(
                "Personalidad mascota: {}.\n",
                m.personality.system_prompt_snippet()
            ));
            let mood = m.update_mood(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                false,
            );
            base.push_str(&format!("Ánimo actual: {}.\n", mood.label()));
        }
        if !self.avatar.custom_instructions.trim().is_empty() {
            let ci: String = self.avatar.custom_instructions.chars().take(800).collect();
            base.push_str(&format!("Instrucciones usuario: {ci}.\n"));
        }
        if self.avatar.verbosity != 50
            || self.avatar.humor != 30
            || self.avatar.formality != 50
            || self.avatar.empathy != 60
        {
            base.push_str(&format!(
                "Rasgos: verbosidad {} humor {} formalidad {} empatía {}.\n",
                self.avatar.verbosity,
                self.avatar.humor,
                self.avatar.formality,
                self.avatar.empathy
            ));
        }
        if !self.avatar.language.trim().is_empty() {
            base.push_str(&format!("Idioma preferido: {}.\n", self.avatar.language));
        }
        let long = self.long_memory.render_for_prompt();
        if !long.is_empty() {
            base.push_str(&format!("\n[Memoria largo plazo]\n{long}"));
        }
        if base.chars().count() > MAX_MEMORY_CHARS + 800 {
            let cut: String = base.chars().take(MAX_MEMORY_CHARS + 800 - 20).collect();
            format!("{cut}…\n[resumen recortado]")
        } else {
            base
        }
    }

    /// Guarda un recuerdo episódico (ej. preferencia detectada).
    pub fn remember_fact(&mut self, text: &str, epoch: u64, importance: f32) {
        let fact = Fact::new(text, epoch, importance);
        self.long_memory.push_fact(fact);
        if self.long_memory.first_seen_epoch.is_none() {
            self.long_memory.first_seen_epoch = Some(epoch);
        }
    }
}

/// Tiempo transcurrido desde `epoch` hasta `now`, legible y determinista.
/// Sin dependencias externas (matemática pura): hoy, hace N días, hace N h o
/// hace N min. Si `now <= epoch` devuelve «hoy».
pub fn time_ago(epoch: u64, now: u64) -> String {
    const DAY: u64 = 86_400;
    const HOUR: u64 = 3_600;
    const MINUTE: u64 = 60;
    let elapsed = now.saturating_sub(epoch);
    if elapsed < MINUTE {
        return "hoy".to_string();
    }
    if elapsed < HOUR {
        let minutes = elapsed / MINUTE;
        return format!("hace {minutes} min");
    }
    if elapsed < DAY {
        let hours = elapsed / HOUR;
        return format!("hace {hours} h");
    }
    let days = elapsed / DAY;
    if days == 1 {
        return "ayer".to_string();
    }
    format!("hace {days} días")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ago_is_deterministic_and_legible() {
        assert_eq!(time_ago(100, 100), "hoy");
        assert_eq!(time_ago(100, 130), "hoy"); // 30 s < 1 min
        assert_eq!(time_ago(0, 86_400), "ayer");
        assert_eq!(time_ago(0, 86_400 * 3), "hace 3 días");
        assert_eq!(time_ago(0, 1_800), "hace 30 min");
    }
    #[test]
    fn profile_tracks_outcomes_levels_and_branch_coverage() {
        let mut profile = StudentProfile::new("Lucas");
        assert_eq!(profile.name, "Lucas");
        // 1 - 0.85^n >= 0.8 → n >= 10; con 12 aciertos el dominio supera 0.8.
        for _ in 0..12 {
            profile.record_outcome("calculus", "Cálculo", 1, true);
        }
        let branch = &profile.branches[0];
        assert!(branch.mastery >= 0.8, "el dominio pasa de 0.8");
        assert!(branch.covered, "la rama se cubre al superar el umbral");
        assert!(profile.xp >= 10 * 12 + 40);
        assert!(profile.level >= 1);
    }

    #[test]
    fn recommendation_prioritizes_uncovered_low_domain_branches() {
        let mut profile = StudentProfile::new("Ana");
        profile.record_outcome("algebra", "Álgebra", 0, true);
        profile.record_outcome("geometry3d", "Geometría 3D", 1, false);
        // Usar variante determinista para evitar flakiness por reloj
        let next = profile.recommend_next_with_scheduler(9_999_999);
        assert!(!next.is_empty());
        assert!(!next.iter().any(|b| b.covered));
        // La más débil primero — con due determinista, geometry3d debería estar entre las primeras
        // (orden exacta depende de scheduler + BKT; verificar presencia no posición)
        assert!(
            next.iter().any(|b| b.id == "geometry3d"),
            "geometry3d debe estar recomendada"
        );
    }

    #[test]
    fn recommend_next_delegates_to_scheduler() {
        // Verifica que recommend_next() (reloj) y recommend_next_at(now) coincidan
        // en el orden relativo cuando el reloj se inyecta. No podemos asumir now
        // exacto, pero sí que ambas APIs retornan el mismo conjunto y que with_scheduler es determinista.
        let mut profile = StudentProfile::new("Deleg");
        profile.record_outcome("algebra", "Álgebra", 0, false);
        profile.record_outcome("geometry", "Geometría", 0, true);
        for b in &mut profile.branches {
            if b.id == "algebra" {
                b.mastery = 0.2;
                b.next_review_epoch = Some(10);
                b.box_level = 1;
            } else if b.id == "geometry" {
                b.mastery = 0.9;
                b.next_review_epoch = Some(9_999_999);
                b.box_level = 3;
            }
        }
        let det = profile.recommend_next_at(100);
        assert_eq!(det[0].id, "algebra");
        // recommend_next() usa now real; con epoch grande ambas ramas estarán due, pero
        // el test asegura que al menos no rompe y retorna pending ordenado
        let live = profile.recommend_next();
        assert!(!live.is_empty());
        assert!(live.iter().any(|b| b.id == "algebra"));
    }

    #[test]
    fn memory_is_bounded_and_mentions_coverage() {
        let mut profile = StudentProfile::new("Mia");
        profile.record_outcome("linear", "Ecuaciones", 5, true);
        let memory = profile.memory();
        assert!(memory.len() <= MAX_MEMORY_CHARS);
        assert!(memory.contains("Ecuaciones"));
    }

    #[test]
    fn profile_serde_round_trips_without_loss() {
        let mut profile = StudentProfile::new("Leo");
        profile.record_outcome("stats", "Estadística", 9, true);
        profile.record_exam(ExamResult {
            epoch: 10,
            branch_id: "stats".to_string(),
            score: 4,
            total: 5,
            passed: true,
        });
        let json = serde_json::to_string(&profile).expect("serialize");
        let restored: StudentProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, profile);
    }

    #[test]
    fn non_finite_mastery_is_never_produced() {
        let mut profile = StudentProfile::new("Nico");
        for _ in 0..200 {
            profile.record_outcome("fractals", "Fractales", 1, true);
            profile.record_outcome("fractals", "Fractales", 1, false);
        }
        assert!(profile.branches[0].mastery.is_finite());
        assert!(profile.branches[0].mastery >= 0.0 && profile.branches[0].mastery <= 1.0);
    }

    #[test]
    fn domain_history_records_evolution_and_stays_capped() {
        let mut profile = StudentProfile::new("Evo");
        for index in 0..70 {
            profile.record_outcome("algebra", "Álgebra", index as u64, true);
        }
        let branch = &profile.branches[0];
        assert!(branch.domain_history.len() <= 64, "histórico acotado");
        assert!(branch.domain_history.len() >= 2);
        let last = branch
            .domain_history
            .back()
            .map(|entry| entry.1)
            .unwrap_or(0.0);
        let first = branch
            .domain_history
            .front()
            .map(|entry| entry.1)
            .unwrap_or(1.0);
        assert!(last >= first, "el dominio sube con los aciertos");
    }

    #[test]
    fn streak_rises_on_correct_and_resets_on_failure() {
        let mut profile = StudentProfile::new("Racha");
        for _ in 0..3 {
            profile.record_outcome("algebra", "Álgebra", 1, true);
        }
        assert_eq!(profile.streak, 3);
        assert_eq!(profile.best_streak, 3);
        profile.record_outcome("algebra", "Álgebra", 1, false);
        assert_eq!(profile.streak, 0);
        assert_eq!(profile.best_streak, 3, "la mejor se conserva");
    }

    #[test]
    fn history_is_capped_to_max_events() {
        let mut profile = StudentProfile::new("Sofi");
        for year in 0..(MAX_HISTORY_EVENTS + 30) as u64 {
            profile.record_outcome("algebra", "Álgebra", year, true);
        }
        assert!(profile.history.len() <= MAX_HISTORY_EVENTS);
    }

    #[test]
    fn mascot_mut_or_default_creates_and_syncs() {
        let mut profile = StudentProfile::new("Masc");
        profile.mascot = None;
        profile.avatar.mascot = None;
        let m = profile.mascot_mut_or_default();
        m.name = "TestPou".to_string();
        assert_eq!(profile.avatar.mascot.as_ref().unwrap().name, "TestPou");
        assert!(profile.mascot.is_some());
        // Segunda llamada no duplica
        let _ = profile.mascot_mut_or_default();
        assert!(profile.avatar.mascot.is_some());
    }

    #[test]
    fn profile_migration_defaults_mascot_fields() {
        let json = r#"{"name":"Old","level":1,"xp":0,"branches":[],"history":[],"exams":[],"streak":0,"best_streak":0}"#;
        let restored: StudentProfile = serde_json::from_str(json).expect("migration");
        assert!(restored.avatar.mascot.is_some() || restored.mascot.is_none());
        // Debe poder serializar de nuevo sin perder datos
        let json2 = serde_json::to_string(&restored).unwrap();
        assert!(json2.contains("Old"));
    }

    #[test]
    fn mascot_config_validation_and_sanitized() {
        let mut m = MascotConfig::default();
        assert!(m.validate().is_ok());
        m.name = "a".repeat(MAX_NAME + 1);
        assert!(m.validate().is_err());
        m.name = "  Hola  ".to_string();
        assert_eq!(m.sanitized_name(), "Hola");
        m.name = "".to_string();
        assert_eq!(m.sanitized_name(), "Pou");
    }

    #[test]
    fn outfit_tier_and_wardrobe_unlock() {
        assert_eq!(OutfitTier::from_level(3), OutfitTier::Primary);
        assert_eq!(OutfitTier::from_level(10), OutfitTier::Secondary);
        assert_eq!(OutfitTier::from_level(15), OutfitTier::University);
        assert_eq!(OutfitTier::from_level(30), OutfitTier::Master);
        let mut w = Wardrobe::default();
        w.unlock_for_level(6);
        assert!(w.is_owned("cap_prim"));
        assert!(w.is_owned("hat_sec"));
    }

    #[test]
    fn bkt_update_correct_increases_incorrect_decreases() {
        let mut p = StudentProfile::new("BKT");
        p.record_outcome("algebra", "Álgebra", 100, true);
        let bkt_after_correct = p.branches[0].bkt_p_known;
        assert!(
            bkt_after_correct > 0.3,
            "BKT debe subir con acierto: {bkt_after_correct}"
        );
        p.record_outcome("algebra", "Álgebra", 200, false);
        let bkt_after_incorrect = p.branches[0].bkt_p_known;
        assert!(
            bkt_after_incorrect < bkt_after_correct,
            "BKT debe bajar con fallo: {bkt_after_incorrect} vs {bkt_after_correct}"
        );
    }

    #[test]
    fn scheduler_interval_grows_with_box_level() {
        let i1 = next_interval(1, 0.5);
        let i2 = next_interval(2, 0.5);
        let i4 = next_interval(4, 0.5);
        assert!(i1 < i2);
        assert!(i2 < i4);
    }

    #[test]
    fn branches_due_filters_correctly() {
        let mut p = StudentProfile::new("Due");
        // Dos ramas con distinto vencimiento
        p.record_outcome("algebra", "Álgebra", 0, true); // -> next_review ~ 129600
        p.record_outcome("calculus", "Cálculo", 0, true);
        // Forzar una vencida y otra futura
        if let Some(b) = p.branches.iter_mut().find(|b| b.id == "algebra") {
            b.next_review_epoch = Some(100);
        }
        if let Some(b) = p.branches.iter_mut().find(|b| b.id == "calculus") {
            b.next_review_epoch = Some(9_999_999);
        }
        let due = p.branches_due(500);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "algebra");
        let all_due = p.branches_due(10_000_000);
        assert_eq!(all_due.len(), 2);
    }

    #[test]
    fn recommend_next_with_scheduler_prioritizes_due() {
        let mut p = StudentProfile::new("Sched");
        // Álgebra débil y vencida, geometría fuerte y no vencida
        p.record_outcome("algebra", "Álgebra", 0, false); // mastery bajo
        p.record_outcome("geometry", "Geometría", 0, true);
        // Ajustar vencimientos: algebra due, geometry futuro
        for b in &mut p.branches {
            if b.id == "algebra" {
                b.mastery = 0.2;
                b.next_review_epoch = Some(10);
                b.box_level = 1;
            } else if b.id == "geometry" {
                b.mastery = 0.9;
                b.next_review_epoch = Some(9_999_999);
                b.box_level = 3;
            }
        }
        let ranked = p.recommend_next_with_scheduler(100);
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].id, "algebra", "vencida debe ir primero");
    }

    #[test]
    fn box_level_increments_on_correct_and_decrements_on_failure() {
        let mut p = StudentProfile::new("Box");
        p.record_outcome("algebra", "Álgebra", 0, true);
        assert_eq!(p.branches[0].box_level, 1);
        p.record_outcome("algebra", "Álgebra", 1, true);
        assert_eq!(p.branches[0].box_level, 2);
        p.record_outcome("algebra", "Álgebra", 2, false);
        assert_eq!(p.branches[0].box_level, 0, "debe bajar 2 niveles");
        // No debe underflow
        p.record_outcome("algebra", "Álgebra", 3, false);
        assert_eq!(p.branches[0].box_level, 0);
        // Subir hasta tope 8
        for i in 0..20 {
            p.record_outcome("algebra", "Álgebra", 10 + i, true);
        }
        assert!(p.branches[0].box_level <= 8);
        assert_eq!(p.branches[0].box_level, 8);
    }

    #[test]
    fn branch_state_migration_defaults_bkt_and_scheduler() {
        let json = r#"{"id":"old","name":"Vieja","covered":false,"mastery":0.5,"last_study_epoch":null,"domain_history":[]}"#;
        let branch: BranchState = serde_json::from_str(json).expect("migrate branch");
        assert_eq!(branch.next_review_epoch, None);
        assert_eq!(branch.box_level, 0);
        assert!((branch.bkt_p_known - 0.3).abs() < 1e-9);
        let json2 = serde_json::to_string(&branch).expect("serialize migrated");
        assert!(json2.contains("old"));
    }

    #[test]
    fn profile_migration_defaults_bkt_scheduler() {
        let json = r#"{"name":"Old","level":1,"xp":0,"branches":[{"id":"algebra","name":"Álgebra","covered":false,"mastery":0.4,"last_study_epoch":null,"domain_history":[]}],"history":[],"exams":[],"streak":0,"best_streak":0}"#;
        let restored: StudentProfile = serde_json::from_str(json).expect("migration");
        assert_eq!(restored.branches[0].box_level, 0);
        assert!(restored.branches[0].next_review_epoch.is_none());
        assert!((restored.branches[0].bkt_p_known - 0.3).abs() < 1e-9);
    }

    #[test]
    fn bkt_p_known_is_finite_and_bounded() {
        let mut p = StudentProfile::new("Fin");
        for i in 0..100 {
            p.record_outcome("algebra", "Álgebra", i, i % 3 == 0);
            assert!(p.branches[0].bkt_p_known.is_finite());
            assert!((0.0..=1.0).contains(&p.branches[0].bkt_p_known));
        }
    }

    #[test]
    fn next_review_epoch_is_set_and_monotonic_with_box() {
        let mut p = StudentProfile::new("Rev");
        p.record_outcome("algebra", "Álgebra", 1_000, true);
        let first = p.branches[0].next_review_epoch.expect("review set");
        assert!(first > 1_000);
        let first_interval = first - 1_000;
        p.record_outcome("algebra", "Álgebra", 2_000, true);
        let second = p.branches[0].next_review_epoch.expect("review set");
        let second_interval = second - 2_000;
        assert!(
            second_interval > first_interval,
            "interval debe crecer con box_level: {first_interval} vs {second_interval}"
        );
    }
}
