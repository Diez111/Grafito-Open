//! Modo proyector — contraste y tamaño para el aula.
//!
//! Piel pura: `fn render(&Estado) -> Frame`. Sin I/O ni spawn. Reusa los
//! tokens A11Y (`HIT_TARGET_AULA`, `aula_font_size`, `TEXT_GAMMA_FLOOR`):
//! el texto nunca se atenúa bajo 85 % ni baja de 12 px en proyector.

use crate::tokens::{
    aula_font_size, hit_target_size, HIT_TARGET_AULA, RADIUS_SM, SPACE_SM, SPACE_XS, TYPE_BASE,
    TYPE_SM, TYPE_XS,
};

/// Estado del modo proyector (un toggle, sin persistencia: la app decide).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectorMode {
    /// ¿Proyector activo?
    pub enabled: bool,
}

impl ProjectorMode {
    /// Tamaño tipográfico efectivo: base o escala aula (piso 12 px).
    pub fn font_size(&self, base: f32) -> f32 {
        if self.enabled {
            aula_font_size(base)
        } else {
            base
        }
    }

    /// Hit-target efectivo: 24 px normal, 44 px en proyector (WCAG 2.5.5).
    pub fn hit_target(&self, requested: f32) -> f32 {
        if self.enabled {
            requested.max(HIT_TARGET_AULA)
        } else {
            hit_target_size(requested)
        }
    }

    /// Altura de fila cómoda a distancia (base + aire aula).
    pub fn row_height(&self) -> f32 {
        if self.enabled {
            TYPE_BASE + SPACE_SM + HIT_TARGET_AULA / 4.0
        } else {
            TYPE_BASE + SPACE_SM
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

/// Interruptor del modo proyector. Siempre alterna y explica el estado;
/// nunca es un botón mudo.
pub fn draw_projector_toggle(ui: &mut egui::Ui, mode: &mut ProjectorMode) {
    let label = if mode.enabled {
        "🔆 Proyector: ON"
    } else {
        "🔆 Proyector: OFF"
    };
    if ui
        .add_sized(
            [ui.available_width(), mode.hit_target(TYPE_BASE + SPACE_SM)],
            egui::Button::new(egui::RichText::new(label).size(TYPE_SM).strong())
                .rounding(RADIUS_SM),
        )
        .on_hover_text(if mode.enabled {
            "Desactivar: vuelve al tamaño y contraste normales"
        } else {
            "Activar: tipo grande (×1.25, piso 12 px) y blancos clicables de 44 px para el aula"
        })
        .clicked()
    {
        mode.toggle();
    }
    if mode.enabled {
        ui.add_space(SPACE_XS);
        ui.label(
            egui::RichText::new("Modo aula: contraste alto, sin atenuar texto bajo 85 %.")
                .size(TYPE_XS)
                .weak(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{HIT_TARGET_MIN, TEXT_GAMMA_FLOOR, TYPE_MIN_AULA};

    #[test]
    fn off_keeps_base_sizes() {
        let mode = ProjectorMode::default();
        assert_eq!(mode.font_size(TYPE_BASE), TYPE_BASE);
        assert_eq!(mode.hit_target(16.0), HIT_TARGET_MIN);
    }

    #[test]
    fn on_enlarges_type_and_targets() {
        let mode = ProjectorMode { enabled: true };
        assert!(mode.font_size(TYPE_BASE) > TYPE_BASE);
        assert!(mode.font_size(TYPE_BASE) >= TYPE_MIN_AULA);
        assert_eq!(mode.hit_target(16.0), HIT_TARGET_AULA);
        assert_eq!(mode.hit_target(60.0), 60.0);
        assert!(mode.row_height() > TYPE_BASE);
    }

    #[test]
    fn gamma_floor_keeps_contrast() {
        assert!((TEXT_GAMMA_FLOOR - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn toggle_flips() {
        let mut mode = ProjectorMode::default();
        mode.toggle();
        assert!(mode.enabled);
        mode.toggle();
        assert!(!mode.enabled);
    }
}
