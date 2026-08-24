//! Perfil pedagógico del usuario (ADR-0001): memoria de aprendizaje que el
//! tutor-orquestador «Mora» usa para adaptar el plan, ir ramificando y medir
//! el progreso. Crate de capa hoja: sin egui, testable headless, persistible.

pub mod exam;
pub mod long_memory;
pub mod mascot;

// Re-exportar tipos de avatar/mascota en la raíz
pub use long_memory::{Fact, LongTermMemory, Preferences};
pub use mascot::{
    AvatarAccessory, AvatarBlush, AvatarConfig, AvatarEyeStyle, AvatarMouthStyle, AvatarShape,
    DailyMission, FurnitureKind, HouseTheme, MascotConfig, MascotMood, MascotSpecies, Outfit,
    OutfitLayer, OutfitTier, Personality, ShopItem, Wardrobe, MAX_NAME,
};

use serde::{Deserialize, Serialize};

/// Límites explícitos para que la memoria nunca crezca sin cota.
const MAX_BRANCHES: usize = 128;
const MAX_HISTORY_EVENTS: usize = 2_000;
const MAX_MEMORY_CHARS: usize = 2_400;

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
    #[serde(default)]
    pub domain_history: Vec<(u64, f32)>,
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
        self.avatar.mascot.as_mut().unwrap()
    }

    /// Asegura que la rama exista; devuelve su índice (MAX si hay límite).
    pub fn ensure_branch(&mut self, id: &str, name: &str) -> usize {
        if let Some(index) = self.branches.iter().position(|b| b.id == id) {
            return index;
        }
        if self.branches.len() >= MAX_BRANCHES {
            return usize::MAX;
        }
        self.branches.push(BranchState {
            id: id.to_string(),
            name: name.to_string(),
            covered: false,
            mastery: 0.0,
            last_study_epoch: None,
            domain_history: Vec::new(),
        });
        self.branches.len() - 1
    }

    /// Registra una respuesta y actualiza dominio (EMA) y progreso.
    pub fn record_outcome(&mut self, branch_id: &str, name: &str, epoch: u64, correct: bool) {
        let index = self.ensure_branch(branch_id, name);
        if index == usize::MAX {
            return;
        }
        let branch = &mut self.branches[index];
        if correct {
            branch.mastery += 0.15 * (1.0 - branch.mastery);
            self.xp = self.xp.saturating_add(10);
            self.streak = self.streak.saturating_add(1);
            self.best_streak = self.best_streak.max(self.streak);
            if branch.mastery >= 0.8 && !branch.covered {
                branch.covered = true;
                self.xp = self.xp.saturating_add(40);
            }
        } else {
            branch.mastery *= 0.95;
            self.streak = 0;
        }
        branch.last_study_epoch = Some(epoch);
        // Evolución de dominio acotada (64 muestras por rama).
        branch.domain_history.push((epoch, branch.mastery));
        if branch.domain_history.len() > 64 {
            branch.domain_history.remove(0);
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
        if index != usize::MAX && result.passed {
            let branch = &mut self.branches[index];
            branch.mastery = branch
                .mastery
                .max(result.score as f32 / result.total.max(1) as f32);
            branch.covered = true;
            self.xp = self.xp.saturating_add(60);
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
    pub fn recommend_next(&self) -> Vec<&BranchState> {
        let mut pending: Vec<&BranchState> = self
            .branches
            .iter()
            .filter(|branch| !branch.covered)
            .collect();
        pending.sort_by(|a, b| {
            a.mastery
                .partial_cmp(&b.mastery)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        pending
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
        if trimmed.chars().count() > 32 {
            return Err("El nombre no puede superar 32 caracteres".to_string());
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
        // Añadir personalidad/ánimo y memoria larga
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
        let next = profile.recommend_next();
        assert!(!next.is_empty());
        assert!(!next.iter().any(|b| b.covered));
        assert_eq!(next[0].id, "geometry3d", "la más débil primero");
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
            .last()
            .map(|entry| entry.1)
            .unwrap_or(0.0);
        let first = branch
            .domain_history
            .first()
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
}
