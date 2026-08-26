//! Memoria de trabajo — contexto episódico de la sesión actual.
//!
//! Guarda el tema actual, pasos intentados y conteo de misconceptions
//! para que el tutor socrático adapte preguntas y pistas sin tocar la
//! memoria a largo plazo. Pura, acotada y testeable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Memoria de trabajo de la sesión (RAM episódica).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingMemory {
    /// Tema actual en foco (ej. "derivada", "fracciones").
    pub current_topic: Option<String>,
    /// Pasos/intentos totales en la sesión.
    pub steps_tried: u32,
    /// Conteo por tipo de misconception (ej. "sign" -> 2).
    pub misconception_counts: HashMap<String, u8>,
    /// Epoch de inicio/actualización de la sesión.
    pub session_epoch: u64,
    /// Último concepto mencionado/intentado.
    pub last_concept: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for WorkingMemory {
    fn default() -> Self {
        Self {
            current_topic: None,
            steps_tried: 0,
            misconception_counts: HashMap::new(),
            session_epoch: 0,
            last_concept: None,
        }
    }
}

impl WorkingMemory {
    /// Crea una memoria vacía.
    pub fn new() -> Self {
        Self::default()
    }

    /// Crea con epoch inicial.
    pub fn with_epoch(session_epoch: u64) -> Self {
        Self {
            session_epoch,
            ..Self::default()
        }
    }

    /// Define el tema actual y actualiza `last_concept`.
    pub fn set_topic(&mut self, topic: impl Into<String>) {
        let t = topic.into();
        let trimmed = t.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        self.current_topic = Some(trimmed.clone());
        self.last_concept = Some(trimmed);
    }

    /// Actualiza el epoch de sesión.
    pub fn set_session_epoch(&mut self, epoch: u64) {
        self.session_epoch = epoch;
    }

    /// Registra un intento con posible misconception.
    ///
    /// - Incrementa `steps_tried` (saturating).
    /// - Si `misconception` no vacía, incrementa su contador (cap 255) y actualiza `last_concept`.
    /// - Si vacía o whitespace, solo cuenta el paso.
    pub fn record_attempt(&mut self, misconception: &str) {
        self.steps_tried = self.steps_tried.saturating_add(1);
        let m = misconception.trim().to_lowercase();
        if m.is_empty() {
            return;
        }
        // Normaliza clave: lowercase, sin espacios extra.
        let key = m.clone();
        let entry = self.misconception_counts.entry(key.clone()).or_insert(0);
        *entry = entry.saturating_add(1);
        self.last_concept = Some(key);
    }

    /// Limpia el estado episódico (pasos, conteos, tema actual y último concepto).
    /// Mantiene `session_epoch` para no perder referencia temporal.
    pub fn clear(&mut self) {
        self.current_topic = None;
        self.last_concept = None;
        self.steps_tried = 0;
        self.misconception_counts.clear();
    }

    /// Limpieza total incluyendo epoch.
    pub fn clear_all(&mut self) {
        self.clear();
        self.session_epoch = 0;
    }

    /// Resumen legible para prompt/debug (acotado).
    pub fn summary(&self) -> String {
        let topic = self.current_topic.as_deref().unwrap_or("sin tema");
        let last = self.last_concept.as_deref().unwrap_or("ninguno");
        let mut s = format!(
            "Tema: {topic}, pasos: {}, último: {last}, sesión: {}",
            self.steps_tried, self.session_epoch
        );
        if self.misconception_counts.is_empty() {
            s.push_str(", sin misconceptions");
        } else {
            s.push_str(", misconceptions: ");
            let mut pairs: Vec<(&String, &u8)> = self.misconception_counts.iter().collect();
            pairs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            let parts: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
            s.push_str(&parts.join(", "));
        }
        s
    }

    /// Conteo total de misconceptions registradas.
    pub fn total_misconceptions(&self) -> u32 {
        self.misconception_counts.values().map(|v| *v as u32).sum()
    }

    /// Misconception más frecuente, si existe.
    pub fn top_misconception(&self) -> Option<(&String, u8)> {
        self.misconception_counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(k, v)| (k, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let wm = WorkingMemory::new();
        assert_eq!(wm.steps_tried, 0);
        assert!(wm.current_topic.is_none());
        assert!(wm.misconception_counts.is_empty());
        assert_eq!(wm.session_epoch, 0);
        assert!(wm.last_concept.is_none());
    }

    #[test]
    fn with_epoch_sets_epoch() {
        let wm = WorkingMemory::with_epoch(999);
        assert_eq!(wm.session_epoch, 999);
    }

    #[test]
    fn set_topic_updates_both() {
        let mut wm = WorkingMemory::new();
        wm.set_topic("  derivada  ");
        assert_eq!(wm.current_topic.as_deref(), Some("derivada"));
        assert_eq!(wm.last_concept.as_deref(), Some("derivada"));
        wm.set_topic("   ");
        // vacío no debe sobrescribir
        assert_eq!(wm.current_topic.as_deref(), Some("derivada"));
    }

    #[test]
    fn record_attempt_increments() {
        let mut wm = WorkingMemory::new();
        wm.record_attempt("sign");
        assert_eq!(wm.steps_tried, 1);
        assert_eq!(wm.misconception_counts.get("sign"), Some(&1));
        wm.record_attempt("sign");
        assert_eq!(wm.misconception_counts.get("sign"), Some(&2));
        wm.record_attempt("fraction");
        assert_eq!(wm.steps_tried, 3);
        assert_eq!(wm.total_misconceptions(), 3);
    }

    #[test]
    fn record_attempt_empty_only_steps() {
        let mut wm = WorkingMemory::new();
        wm.record_attempt("   ");
        assert_eq!(wm.steps_tried, 1);
        assert!(wm.misconception_counts.is_empty());
        assert!(wm.last_concept.is_none());
    }

    #[test]
    fn record_attempt_normalizes_case() {
        let mut wm = WorkingMemory::new();
        wm.record_attempt("Sign");
        wm.record_attempt("SIGN");
        assert_eq!(wm.misconception_counts.get("sign"), Some(&2));
        assert_eq!(wm.misconception_counts.len(), 1);
    }

    #[test]
    fn clear_resets_episodic() {
        let mut wm = WorkingMemory::with_epoch(123);
        wm.set_topic("vectores");
        wm.record_attempt("concept");
        wm.record_attempt("sign");
        assert!(wm.steps_tried > 0);
        wm.clear();
        assert_eq!(wm.steps_tried, 0);
        assert!(wm.current_topic.is_none());
        assert!(wm.last_concept.is_none());
        assert!(wm.misconception_counts.is_empty());
        // epoch se mantiene
        assert_eq!(wm.session_epoch, 123);
        wm.clear_all();
        assert_eq!(wm.session_epoch, 0);
    }

    #[test]
    fn summary_contains_info() {
        let mut wm = WorkingMemory::new();
        wm.set_topic("fracciones");
        wm.record_attempt("fraction");
        let s = wm.summary();
        assert!(s.contains("fracciones"));
        assert!(s.contains("pasos: 1"));
        assert!(s.contains("fraction=1"));
    }

    #[test]
    fn summary_empty_topic() {
        let wm = WorkingMemory::new();
        let s = wm.summary();
        assert!(s.contains("sin tema"));
        assert!(s.contains("sin misconceptions"));
    }

    #[test]
    fn top_misconception() {
        let mut wm = WorkingMemory::new();
        wm.record_attempt("sign");
        wm.record_attempt("sign");
        wm.record_attempt("fraction");
        let top = wm.top_misconception().expect("debe haber top");
        assert_eq!(top.0, "sign");
        assert_eq!(top.1, 2);
    }

    #[test]
    fn saturating_u8() {
        let mut wm = WorkingMemory::new();
        for _ in 0..300 {
            wm.record_attempt("sign");
        }
        assert_eq!(wm.misconception_counts.get("sign"), Some(&255));
        assert_eq!(wm.steps_tried, 300);
    }

    #[test]
    fn serde_roundtrip() {
        let mut wm = WorkingMemory::new();
        wm.set_topic("matrices");
        wm.record_attempt("concept");
        wm.set_session_epoch(42);
        let json = serde_json::to_string(&wm).expect("serialize");
        let de: WorkingMemory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(wm, de);
    }
}
