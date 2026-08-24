//! Memoria a largo plazo — hechos, preferencias y relación.
//! Persistente, acotada y migrable. Sin egui, 100% testable.

use serde::{Deserialize, Serialize};

const MAX_FACTS: usize = 50;
const MAX_FACT_LEN: usize = 160;
const MAX_PREF_LEN: usize = 80;
const MAX_CUSTOM_INSTRUCTIONS: usize = 800;

/// Hecho episódico recordado (qué dijo/hizo el usuario).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    /// Texto del hecho (ej. "Prefiere ejemplos con x²").
    pub text: String,
    /// Epoch segundos.
    pub epoch: u64,
    /// Importancia 0.0-1.0 (para poda).
    #[serde(default)]
    pub importance: f32,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Fact {
    pub fn new(text: impl Into<String>, epoch: u64, importance: f32) -> Self {
        let mut t: String = text.into().trim().chars().take(MAX_FACT_LEN).collect();
        if t.is_empty() {
            t = "recuerdo".to_string();
        }
        Self {
            text: t,
            epoch,
            importance: importance.clamp(0.0, 1.0),
            tags: Vec::new(),
        }
    }
}

/// Preferencias de aprendizaje y relación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub tone: String, // chill, curioso, energético, dulce, socrático...
    #[serde(default)]
    pub detail_level: String, // breve, medio, detallado
    #[serde(default)]
    pub language: String, // es, en, auto
    #[serde(default)]
    pub goal: String, // examen, olimpiada, hobby
    #[serde(default)]
    pub custom_instructions: String, // instrucciones libres 800c
    #[serde(default)]
    pub verbosity: String, // compat legacy
}

#[allow(clippy::derivable_impls)]
impl Default for Preferences {
    fn default() -> Self {
        Self {
            tone: String::new(),
            detail_level: String::new(),
            language: String::new(),
            goal: String::new(),
            custom_instructions: String::new(),
            verbosity: String::new(),
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.custom_instructions.chars().count() > MAX_CUSTOM_INSTRUCTIONS {
            return Err(format!(
                "Instrucciones no pueden superar {MAX_CUSTOM_INSTRUCTIONS} caracteres"
            ));
        }
        if self.tone.chars().count() > MAX_PREF_LEN
            || self.detail_level.chars().count() > MAX_PREF_LEN
            || self.language.chars().count() > MAX_PREF_LEN
            || self.goal.chars().count() > MAX_PREF_LEN
        {
            return Err("Preferencia demasiado larga".to_string());
        }
        Ok(())
    }
}

/// Memoria a largo plazo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongTermMemory {
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub relationship_stage: u8, // 0 nuevo, 1 conocido, 2 compañero, 3 cómplice
    #[serde(default)]
    pub first_seen_epoch: Option<u64>,
    #[serde(default)]
    pub last_summary_epoch: Option<u64>,
    #[serde(default)]
    pub summary: String, // resumen condensado de charla antigua
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[allow(clippy::derivable_impls)]
impl Default for LongTermMemory {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            preferences: Preferences::default(),
            relationship_stage: 0,
            first_seen_epoch: None,
            last_summary_epoch: None,
            summary: String::new(),
            enabled: true,
        }
    }
}

impl LongTermMemory {
    /// Añade hecho, poda por importancia y límite.
    pub fn push_fact(&mut self, fact: Fact) {
        // Evita duplicado exacto reciente
        if self.facts.iter().any(|f| f.text == fact.text) {
            return;
        }
        self.facts.push(fact);
        if self.facts.len() > MAX_FACTS {
            // Podar el menos importante más antiguo
            if let Some(pos) = self
                .facts
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.importance
                        .partial_cmp(&b.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.epoch.cmp(&b.epoch))
                })
                .map(|(i, _)| i)
            {
                self.facts.remove(pos);
            }
        }
        self.relationship_stage = (self.facts.len() as u8 / 12).min(3);
    }

    /// Condensa texto largo en summary (heurística local, sin LLM).
    pub fn summarize(&mut self, text: &str, epoch: u64) {
        if text.trim().is_empty() {
            return;
        }
        let snippet: String = text.chars().filter(|c| !c.is_control()).take(180).collect();
        let fact = Fact::new(snippet, epoch, 0.5);
        self.push_fact(fact);
        self.last_summary_epoch = Some(epoch);
        if self.facts.len() > 5 {
            let mut s = String::new();
            for f in self.facts.iter().rev().take(5) {
                if !s.is_empty() {
                    s.push_str(" | ");
                }
                s.push_str(&f.text);
            }
            self.summary = s.chars().take(400).collect();
        }
    }

    /// Render para prompt (acotado).
    pub fn render_for_prompt(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        if self.facts.is_empty()
            && self.summary.is_empty()
            && self.preferences.tone.is_empty()
            && self.preferences.goal.is_empty()
            && self.preferences.custom_instructions.is_empty()
        {
            return String::new();
        }
        let mut out = String::new();
        if !self.summary.is_empty() {
            out.push_str(&format!("Resumen charla previa: {}.\n", self.summary));
        }
        for fact in self.facts.iter().rev().take(5) {
            out.push_str(&format!("- Recuerdo: {}.\n", fact.text));
        }
        if !self.preferences.tone.is_empty() {
            out.push_str(&format!("Preferencia tono: {}.\n", self.preferences.tone));
        }
        if !self.preferences.detail_level.is_empty() {
            out.push_str(&format!(
                "Nivel detalle: {}.\n",
                self.preferences.detail_level
            ));
        }
        if !self.preferences.language.is_empty() {
            out.push_str(&format!("Idioma: {}.\n", self.preferences.language));
        }
        if !self.preferences.goal.is_empty() {
            out.push_str(&format!("Objetivo: {}.\n", self.preferences.goal));
        }
        if !self.preferences.custom_instructions.is_empty() {
            let trimmed: String = self
                .preferences
                .custom_instructions
                .chars()
                .take(MAX_CUSTOM_INSTRUCTIONS)
                .collect();
            out.push_str(&format!("Instrucciones: {}.\n", trimmed));
        }
        out.push_str(&format!("Vínculo: etapa {}.\n", self.relationship_stage));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_poda() {
        let mut m = LongTermMemory::default();
        for i in 0..60 {
            m.push_fact(Fact::new(format!("fact {i}"), i as u64, 0.1));
        }
        assert!(m.facts.len() <= MAX_FACTS);
    }

    #[test]
    fn dedup() {
        let mut m = LongTermMemory::default();
        m.push_fact(Fact::new("prefiere x²", 0, 0.8));
        m.push_fact(Fact::new("prefiere x²", 1, 0.8));
        assert_eq!(m.facts.len(), 1);
    }

    #[test]
    fn render() {
        let mut m = LongTermMemory::default();
        m.push_fact(Fact::new("odia anim automática", 0, 0.9));
        let r = m.render_for_prompt();
        assert!(r.contains("odia"));
    }

    #[test]
    fn render_includes_new_prefs() {
        let mut m = LongTermMemory::default();
        m.preferences.custom_instructions = "Sé breve".to_string();
        m.preferences.language = "es".to_string();
        let r = m.render_for_prompt();
        assert!(r.contains("Sé breve"));
        assert!(r.contains("es"));
        m.enabled = false;
        assert!(m.render_for_prompt().is_empty());
    }

    #[test]
    fn custom_instructions_truncated() {
        let mut p = Preferences {
            custom_instructions: "a".repeat(900),
            ..Default::default()
        };
        assert!(p.validate().is_err());
        p.custom_instructions = "a".repeat(800);
        assert!(p.validate().is_ok());
    }
}
