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

/// Preferencia de movimiento reducido (WCAG 2.3.3: la animación no es
/// esencial acá — respiración del avatar, reveal por bloques, morph de
/// burbuja). egui 0.29 no expone la preferencia del SO, así que la app la
/// guarda en su `AppConfig` y la pasa a la Piel con este struct. Piel pura:
///
/// - `anim_ms(base)` devuelve 0.0 con movimiento reducido (sin transiciones),
/// - `morph_t(t)` salta al estado final (sin interpolación visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MotionConfig {
    /// ¿Movimiento reducido activo?
    pub reduced_motion: bool,
}

impl MotionConfig {
    /// Duración efectiva de una animación en ms.
    pub fn anim_ms(&self, base_ms: f32) -> f32 {
        if self.reduced_motion {
            0.0
        } else {
            base_ms
        }
    }

    /// Progreso efectivo de un morph 0.0–1.0 (1.0 = estado final directo).
    pub fn morph_t(&self, t: f32) -> f32 {
        if self.reduced_motion {
            1.0
        } else {
            t
        }
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

    #[test]
    fn motion_config_disables_animation_when_reduced() {
        let full = MotionConfig::default();
        assert_eq!(full.anim_ms(180.0), 180.0);
        assert_eq!(full.morph_t(0.3), 0.3);
        let reduced = MotionConfig {
            reduced_motion: true,
        };
        assert_eq!(reduced.anim_ms(180.0), 0.0);
        assert_eq!(reduced.morph_t(0.3), 1.0);
    }
}
