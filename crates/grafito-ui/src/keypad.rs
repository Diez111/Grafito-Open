//! Keypad matemático — símbolos insertables sin botones mudos.
//!
//! Piel pura: `fn render(&Estado) -> Frame`. Cada botón inserta texto vía
//! callback (I/O real ocurre en el llamador, nunca aquí). Sin tamaños
//! hardcodeados: sólo tokens TYPE/SPACE/RADIUS.

use crate::tokens::{RADIUS_SM, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_SM, TYPE_XS};

/// Una tecla: símbolo visible, texto a insertar y por qué existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeypadEntry {
    /// Glifo visible en la tecla.
    pub symbol: &'static str,
    /// Texto que se inserta en la entrada (ASCII honesto, no LaTeX).
    pub insertion: &'static str,
    /// Tooltip rico: qué hace + ejemplo.
    pub tooltip: &'static str,
}

/// Nivel inicial: 8 símbolos frecuentes (constantes y comparación).
pub const KEYPAD_BASIC: &[KeypadEntry] = &[
    KeypadEntry {
        symbol: "π",
        insertion: "pi",
        tooltip: "Pi — inserta «pi». Ej.: pi*r^2",
    },
    KeypadEntry {
        symbol: "√",
        insertion: "sqrt()",
        tooltip: "Raíz — inserta «sqrt()» y deja el cursor adentro",
    },
    KeypadEntry {
        symbol: "∞",
        insertion: "inf",
        tooltip: "Infinito — inserta «inf». Ej.: límite en inf",
    },
    KeypadEntry {
        symbol: "≤",
        insertion: "<=",
        tooltip: "Menor o igual — inserta «<=»",
    },
    KeypadEntry {
        symbol: "≥",
        insertion: ">=",
        tooltip: "Mayor o igual — inserta «>=»",
    },
    KeypadEntry {
        symbol: "≠",
        insertion: "!=",
        tooltip: "Distinto — inserta «!=»",
    },
    KeypadEntry {
        symbol: "±",
        insertion: "+-",
        tooltip: "Más/menos — inserta «+-»",
    },
    KeypadEntry {
        symbol: "θ",
        insertion: "theta",
        tooltip: "Theta — inserta «theta». Ej.: r(theta)",
    },
];

/// Nivel medio: +6 operadores y conjuntos (14 visibles).
pub const KEYPAD_EXTENDED: &[KeypadEntry] = &[
    KeypadEntry {
        symbol: "∑",
        insertion: "sum()",
        tooltip: "Suma — inserta «sum()». Ej.: sum(1/n^2)",
    },
    KeypadEntry {
        symbol: "∫",
        insertion: "integral()",
        tooltip: "Integral — inserta «integral()». Revisá la ayuda de Integral[...]",
    },
    KeypadEntry {
        symbol: "∂",
        insertion: "d/dx",
        tooltip: "Derivada — inserta «d/dx»",
    },
    KeypadEntry {
        symbol: "≈",
        insertion: "~",
        tooltip: "Aproximado — inserta «~»",
    },
    KeypadEntry {
        symbol: "×",
        insertion: "*",
        tooltip: "Producto — inserta «*»",
    },
    KeypadEntry {
        symbol: "÷",
        insertion: "/",
        tooltip: "División — inserta «/»",
    },
];

/// Nivel universidad: +6 griegas y lógica (20 visibles).
pub const KEYPAD_FULL: &[KeypadEntry] = &[
    KeypadEntry {
        symbol: "α",
        insertion: "alpha",
        tooltip: "Alfa — inserta «alpha»",
    },
    KeypadEntry {
        symbol: "β",
        insertion: "beta",
        tooltip: "Beta — inserta «beta»",
    },
    KeypadEntry {
        symbol: "λ",
        insertion: "lambda",
        tooltip: "Lambda — inserta «lambda»",
    },
    KeypadEntry {
        symbol: "Δ",
        insertion: "delta",
        tooltip: "Delta — inserta «delta»",
    },
    KeypadEntry {
        symbol: "∧",
        insertion: "&&",
        tooltip: "Y lógico — inserta «&&»",
    },
    KeypadEntry {
        symbol: "∨",
        insertion: "||",
        tooltip: "O lógico — inserta «||»",
    },
];

/// Máximo de recientes recordados (MRU local, sin I/O).
pub const KEYPAD_MRU_CAP: usize = 8;

/// Cuántas teclas muestra cada nivel Scandinavian (8 / 14 / 20).
pub fn keypad_visible_count(level_value: u32) -> usize {
    if level_value < 8 {
        KEYPAD_BASIC.len()
    } else if level_value < 12 {
        KEYPAD_BASIC.len() + KEYPAD_EXTENDED.len()
    } else {
        KEYPAD_BASIC.len() + KEYPAD_EXTENDED.len() + KEYPAD_FULL.len()
    }
}

/// Itera las teclas visibles para el nivel dado (básico → extendido → full).
pub fn keypad_visible_entries(level_value: u32) -> impl Iterator<Item = &'static KeypadEntry> {
    let count = keypad_visible_count(level_value);
    KEYPAD_BASIC
        .iter()
        .chain(KEYPAD_EXTENDED.iter())
        .chain(KEYPAD_FULL.iter())
        .take(count)
}

/// Estado del keypad: visibilidad + recientes MRU (todo en memoria, sin I/O).
#[derive(Debug, Clone, Default)]
pub struct MathKeypadState {
    /// Colapsado por defecto en pantallas chicas; el toggle siempre funciona.
    pub visible: bool,
    recent: Vec<String>,
}

impl MathKeypadState {
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Registra una inserción en el MRU (sin duplicados, cap 8).
    pub fn record_insertion(&mut self, insertion: &str) {
        if insertion.is_empty() {
            return;
        }
        self.recent.retain(|item| item != insertion);
        self.recent.insert(0, insertion.to_owned());
        self.recent.truncate(KEYPAD_MRU_CAP);
    }

    /// Recientes para la fila MRU (vacío = se explica, no se dibuja).
    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

/// Dibuja el keypad. `on_insert` recibe el texto a insertar; el llamador
/// decide dónde va (input, consola, etc.). Nada aquí hace I/O ni spawn.
pub fn draw_math_keypad(
    ui: &mut egui::Ui,
    state: &mut MathKeypadState,
    level_value: u32,
    mut on_insert: impl FnMut(&str),
) {
    ui.add_space(SPACE_SM);
    let toggle_label = if state.visible {
        "Símbolos ▾"
    } else {
        "Símbolos ▸"
    };
    let toggle = ui
        .add_sized(
            [ui.available_width(), TYPE_BASE + SPACE_SM],
            egui::Button::new(egui::RichText::new(toggle_label).size(TYPE_SM).strong())
                .rounding(RADIUS_SM),
        )
        .on_hover_text("Muestra u oculta el teclado de símbolos matemáticos");
    if toggle.clicked() {
        state.toggle();
    }
    if !state.visible {
        return;
    }
    ui.add_space(SPACE_XS);

    if !state.recent.is_empty() {
        ui.label(egui::RichText::new("Recientes").size(TYPE_XS).weak());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(SPACE_XS, SPACE_XS);
            let recent = state.recent.clone();
            for item in &recent {
                if ui
                    .add_sized(
                        [TYPE_BASE * 2.4, TYPE_BASE + SPACE_XS],
                        egui::Button::new(egui::RichText::new(item.as_str()).size(TYPE_SM))
                            .rounding(RADIUS_SM),
                    )
                    .on_hover_text(format!("Reinsertar «{item}»"))
                    .clicked()
                {
                    on_insert(item);
                }
            }
            if ui
                .small_button("Limpiar")
                .on_hover_text("Olvida los símbolos recientes de esta sesión")
                .clicked()
            {
                state.clear_recent();
            }
        });
        ui.add_space(SPACE_XS);
    }

    egui::Grid::new("math_keypad_grid")
        .num_columns(4)
        .spacing([SPACE_XS, SPACE_XS])
        .show(ui, |ui| {
            for (index, entry) in keypad_visible_entries(level_value).enumerate() {
                // Ancho acotado inferior: egui exige desired_size ≥ 0 aun en
                // drawers mínimos o tests headless.
                let cell_w = ((ui.available_width() - SPACE_XS * 3.0) / 4.0).max(32.0);
                if ui
                    .add_sized(
                        [cell_w, TYPE_BASE + SPACE_SM],
                        egui::Button::new(egui::RichText::new(entry.symbol).size(TYPE_BASE))
                            .rounding(RADIUS_SM),
                    )
                    .on_hover_text(entry.tooltip)
                    .clicked()
                {
                    state.record_insertion(entry.insertion);
                    on_insert(entry.insertion);
                }
                if index % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    ui.label(
        egui::RichText::new("Tocá un símbolo para insertarlo donde estés escribiendo.")
            .size(TYPE_XS)
            .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_counts_follow_levels() {
        assert_eq!(keypad_visible_count(2), 8);
        assert_eq!(keypad_visible_count(8), 14);
        assert_eq!(keypad_visible_count(12), 20);
        assert_eq!(keypad_visible_entries(2).count(), 8);
        assert_eq!(keypad_visible_entries(99).count(), 20);
    }

    #[test]
    fn every_entry_inserts_something_with_tooltip() {
        for entry in KEYPAD_BASIC
            .iter()
            .chain(KEYPAD_EXTENDED.iter())
            .chain(KEYPAD_FULL.iter())
        {
            assert!(!entry.symbol.is_empty());
            assert!(
                !entry.insertion.is_empty(),
                "tecla {} sin inserción",
                entry.symbol
            );
            assert!(
                !entry.tooltip.is_empty(),
                "tecla {} sin tooltip",
                entry.symbol
            );
        }
    }

    #[test]
    fn mru_dedups_and_caps() {
        let mut state = MathKeypadState::default();
        state.record_insertion("");
        assert!(state.recent().is_empty());
        for entry in keypad_visible_entries(99) {
            state.record_insertion(entry.insertion);
        }
        assert_eq!(state.recent().len(), KEYPAD_MRU_CAP);
        let first = state.recent()[0].clone();
        state.record_insertion(&first);
        assert_eq!(state.recent()[0], first);
        assert_eq!(state.recent().iter().filter(|s| **s == first).count(), 1);
        state.clear_recent();
        assert!(state.recent().is_empty());
    }

    #[test]
    fn toggle_flips_visibility() {
        let mut state = MathKeypadState::default();
        assert!(!state.visible);
        state.toggle();
        assert!(state.visible);
    }
}
