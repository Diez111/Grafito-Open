//! Ledger de tarea J-Space: Goal / Core / Verified / Open / Next, acotado.

/// Límites del estado persistente de una tarea.
pub const MAX_LEDGER_GOAL_CHARS: usize = 240;
pub const MAX_LEDGER_CORE_ITEMS: usize = 8;
pub const MAX_LEDGER_VERIFIED_ITEMS: usize = 8;
pub const MAX_LEDGER_OPEN_ITEMS: usize = 5;
pub const MAX_LEDGER_ITEM_CHARS: usize = 200;
pub const MAX_LEDGER_RENDER_BYTES: usize = 2_048;

/// Estado compacto de una tarea larga, con los cinco campos canónicos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
        !self.open.iter().any(|item| item.trim().is_empty())
    }
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
}
