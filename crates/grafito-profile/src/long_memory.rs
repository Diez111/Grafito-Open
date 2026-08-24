//! Memoria a largo plazo — hechos, preferencias y relación.
//! Persistente, acotada y migrable. Sin egui, 100% testable.

use serde::{Deserialize, Serialize};

const MAX_FACTS: usize = 50;
const MAX_FACT_LEN: usize = 120;
#[allow(dead_code)]
const MAX_PREF_LEN: usize = 80;

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
    pub tone: String, // chill, curioso, energético, dulce
    #[serde(default)]
    pub detail_level: String, // breve, medio, detallado
    #[serde(default)]
    pub language: String, // es, en
    #[serde(default)]
    pub goal: String, // examen, olimpiada, hobby
}

#[allow(clippy::derivable_impls)]
impl Default for Preferences {
    fn default() -> Self {
        Self {
            tone: String::new(),
            detail_level: String::new(),
            language: String::new(),
            goal: String::new(),
        }
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
        if self.facts.is_empty() && self.summary.is_empty() {
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
        if !self.preferences.goal.is_empty() {
            out.push_str(&format!("Objetivo: {}.\n", self.preferences.goal));
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
}
