//! Ledger de tarea J-Space: Goal / Core / Verified / Open / Next, acotado.
//!
//! El ledger es el estado vivo de una tarea larga: el loop lo muta por turno
//! (`record_tool_outcome`) y lo persiste (`save_to_file`) para sobrevivir
//! reinicios. Todos los métodos mantienen los presupuestos `MAX_LEDGER_*`.

use serde::{Deserialize, Serialize};

/// Límites del estado persistente de una tarea.
pub const MAX_LEDGER_GOAL_CHARS: usize = 240;
pub const MAX_LEDGER_CORE_ITEMS: usize = 8;
pub const MAX_LEDGER_VERIFIED_ITEMS: usize = 8;
pub const MAX_LEDGER_OPEN_ITEMS: usize = 5;
pub const MAX_LEDGER_ITEM_CHARS: usize = 200;
pub const MAX_LEDGER_RENDER_BYTES: usize = 2_048;

/// Estado compacto de una tarea larga, con los cinco campos canónicos.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct JSpaceLedger {
    pub goal: String,
    pub core: Vec<String>,
    pub verified: Vec<String>,
    pub open: Vec<String>,
    pub next: String,
}

impl JSpaceLedger {
    /// Crea un ledger con el objetivo y la siguiente acción anclados.
    pub fn with_task(goal: impl Into<String>, next: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            core: Vec::new(),
            verified: Vec::new(),
            open: Vec::new(),
            next: next.into(),
        }
    }

    /// Valida tamaños y acota cada campo a sus presupuestos.
    pub fn validate(&self) -> Result<(), String> {
        if self.goal.chars().count() > MAX_LEDGER_GOAL_CHARS {
            return Err("j-space ledger goal exceeds the character budget".into());
        }
        for (label, items, max) in [
            ("core", &self.core, MAX_LEDGER_CORE_ITEMS),
            ("verified", &self.verified, MAX_LEDGER_VERIFIED_ITEMS),
            ("open", &self.open, MAX_LEDGER_OPEN_ITEMS),
        ] {
            if items.len() > max {
                return Err(format!("j-space ledger {label} exceeds the item budget"));
            }
            for item in items {
                if item.chars().count() > MAX_LEDGER_ITEM_CHARS {
                    return Err(format!("j-space ledger {label} item exceeds its budget"));
                }
            }
        }
        if self.next.chars().count() > MAX_LEDGER_ITEM_CHARS {
            return Err("j-space ledger next exceeds its budget".into());
        }
        Ok(())
    }

    /// Renderiza el ledger en texto plano, truncado a un presupuesto de bytes.
    pub fn render_bounded(&self, max_bytes: usize) -> String {
        let max_bytes = max_bytes.min(MAX_LEDGER_RENDER_BYTES);
        let mut lines = Vec::new();
        if !self.goal.trim().is_empty() {
            lines.push(format!("Goal: {}", self.goal.trim()));
        }
        push_ledger_lines(&mut lines, "Core", &self.core);
        push_ledger_lines(&mut lines, "Verified", &self.verified);
        push_ledger_lines(&mut lines, "Open", &self.open);
        if !self.next.trim().is_empty() {
            lines.push(format!("Next: {}", self.next.trim()));
        }
        let mut render = lines.join("\n");
        if render.chars().count() > max_bytes {
            render = render
                .chars()
                .take(max_bytes.saturating_sub(1))
                .collect::<String>();
            render.push('…');
        }
        render
    }

    /// Devuelve si quedan problemas abiertos sin resolver.
    pub fn has_open_items(&self) -> bool {
        !self.open.is_empty() && self.open.iter().any(|item| !item.trim().is_empty())
    }

    /// La tarea está completa cuando no quedan items abiertos.
    /// Es el done-check real (el loop lo combina con el estado de las tools).
    pub fn is_complete(&self) -> bool {
        !self.has_open_items()
    }

    /// Registra el resultado de una herramienta: ok → Verified, error → Open.
    /// Acotado a presupuestos (expulsa el más viejo) y sin duplicar el último.
    pub fn record_tool_outcome(&mut self, tool_name: &str, ok: bool, detail: &str) {
        let tool_name = tool_name.trim();
        if tool_name.is_empty() {
            return;
        }
        let detail_budget = MAX_LEDGER_ITEM_CHARS
            .saturating_sub(tool_name.chars().count())
            .saturating_sub(2);
        let detail = truncate_chars(detail.trim(), detail_budget);
        let entry = if detail.is_empty() {
            tool_name.to_string()
        } else {
            format!("{tool_name}: {detail}")
        };
        let (list, max) = if ok {
            (&mut self.verified, MAX_LEDGER_VERIFIED_ITEMS)
        } else {
            (&mut self.open, MAX_LEDGER_OPEN_ITEMS)
        };
        if list.last().is_some_and(|last| last == &entry) {
            return;
        }
        if list.len() >= max {
            list.remove(0);
        }
        list.push(entry);
    }

    /// Avanza la siguiente acción (acotada al presupuesto del campo).
    pub fn advance_next(&mut self, next: impl Into<String>) {
        self.next = truncate_chars(next.into().trim(), MAX_LEDGER_ITEM_CHARS);
    }

    /// Huella estable del estado (goal + cardinalidades + next) para trazas.
    pub fn fingerprint(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.goal.hash(&mut hasher);
        self.core.len().hash(&mut hasher);
        self.verified.len().hash(&mut hasher);
        self.open.len().hash(&mut hasher);
        self.next.hash(&mut hasher);
        hasher.finish()
    }

    /// Persiste el ledger como JSON (p. ej. `jspace_state.json`) para sobrevivir reinicios.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("j-space ledger serialize failed: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("j-space ledger write failed: {error}"))?;
        Ok(())
    }

    /// Carga y valida un ledger persistido con `save_to_file`.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|error| format!("j-space ledger read failed: {error}"))?;
        let ledger: Self = serde_json::from_str(&json)
            .map_err(|error| format!("j-space ledger parse failed: {error}"))?;
        ledger.validate()?;
        Ok(ledger)
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn push_ledger_lines(lines: &mut Vec<String>, label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    for item in items.iter().filter(|item| !item.trim().is_empty()) {
        lines.push(format!("{label}: {}", item.trim()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_renders_fields_and_respects_byte_budget() {
        let ledger = JSpaceLedger {
            goal: "Graficar y analizar f(x)=x^2-1".into(),
            core: vec!["Dominio real".into()],
            verified: vec!["f(0) = -1".into()],
            open: vec!["Encontrar raíces".into()],
            next: "Evaluar en x=1".into(),
        };
        assert!(ledger.validate().is_ok());
        let render = ledger.render_bounded(512);
        assert!(render.contains("Goal:"));
        assert!(render.contains("Core: Dominio real"));
        assert!(render.contains("Open: Encontrar raíces"));
        assert!(render.contains("Next:"));
        assert!(render.chars().count() <= 512);
    }

    #[test]
    fn ledger_validation_enforces_budgets() {
        let mut ledger = JSpaceLedger::with_task("g", "n");
        ledger.open = vec!["x".repeat(MAX_LEDGER_ITEM_CHARS + 1)];
        assert!(ledger.validate().is_err());
    }

    #[test]
    fn ledger_records_tool_outcomes_bounded_without_duplicates() {
        let mut ledger = JSpaceLedger::with_task("g", "n");
        ledger.record_tool_outcome("evaluate_expr", true, "f(0) = -1");
        ledger.record_tool_outcome("evaluate_expr", true, "f(0) = -1");
        assert_eq!(ledger.verified.len(), 1);
        assert!(ledger.is_complete());
        ledger.record_tool_outcome("grafito_docs", false, "timeout");
        assert!(!ledger.is_complete());
        assert!(ledger.has_open_items());
        // Acota expulsando el más viejo.
        for i in 0..(MAX_LEDGER_OPEN_ITEMS + 3) {
            ledger.record_tool_outcome("t", false, &format!("err-{i}"));
        }
        assert_eq!(ledger.open.len(), MAX_LEDGER_OPEN_ITEMS);
        assert!(ledger.validate().is_ok());
        ledger.advance_next("reintentar docs");
        assert_eq!(ledger.next, "reintentar docs");
    }

    #[test]
    fn ledger_fingerprint_is_stable_and_state_sensitive() {
        let a = JSpaceLedger::with_task("g", "n");
        let mut b = a.clone();
        assert_eq!(a.fingerprint(), b.fingerprint());
        b.record_tool_outcome("t", true, "ok");
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn ledger_roundtrips_through_file() {
        let mut ledger = JSpaceLedger::with_task("analizar f", "evaluar g");
        ledger.record_tool_outcome("evaluate_expr", true, "f(0) = -1");
        let path =
            std::env::temp_dir().join(format!("grafito-ledger-test-{}.json", std::process::id()));
        ledger.save_to_file(&path).expect("save ledger");
        let loaded = JSpaceLedger::load_from_file(&path).expect("load ledger");
        assert_eq!(loaded, ledger);
        let _ = std::fs::remove_file(&path);
    }
}
