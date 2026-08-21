//! Panel de animaciones — preview, progreso y export sin bloquear la UI.
//!
//! Estado puro en GrafitoApp::anim_preview, render en background thread,
//! progreso via egui::ProgressBar y ctx.request_repaint().
//! Separacion Cerebro/Piel: este modulo solo renderiza &AnimPreviewState.

use egui::{Color32, ProgressBar, ScrollArea};
use grafito_anim::{AnimDuration, AnimParams, ExportFormat, Resolution};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct AnimPreviewState {
    pub template: String,
    pub concept: String,
    pub progress: f32,
    pub status: String,
    pub media_path: Option<String>,
    pub frames: Vec<egui::ColorImage>,
    pub source_turn: Option<usize>,
}

impl AnimPreviewState {
    pub fn is_active(&self) -> bool {
        self.progress > 0.0 && self.progress < 1.0
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn draw_anim_panel(ui: &mut egui::Ui, state: &mut AnimPreviewState) {
    ui.heading("Animaciones Didácticas");
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Plantilla:");
        egui::ComboBox::from_id_salt("anim_template")
            .selected_text(if state.template.is_empty() {
                "derivative-slope"
            } else {
                &state.template
            })
            .show_ui(ui, |ui| {
                for tmpl in [
                    "derivative-slope",
                    "integral-area",
                    "taylor-series",
                    "conformal-map",
                ] {
                    ui.selectable_value(&mut state.template, tmpl.to_string(), tmpl);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Concepto:");
        ui.text_edit_singleline(&mut state.concept);
    });
    if state.is_active() {
        let bar_width = ui.available_width().min(420.0);
        ui.add_sized(
            [bar_width, 18.0],
            ProgressBar::new(state.progress.clamp(0.0, 1.0)).text(&state.status),
        );
        ui.label(
            egui::RichText::new(&state.status)
                .color(Color32::GRAY)
                .size(12.0),
        );
    } else if let Some(path) = &state.media_path {
        ui.label(format!("Listo: {}", path));
        if ui.button("Exportar GIF").clicked() {
            // TODO: copiar media_path a destino elegido por usuario (I/O en background)
        }
        if ui.button("Limpiar").clicked() {
            state.clear();
        }
    }
    if !state.frames.is_empty() {
        ScrollArea::horizontal().max_height(84.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (idx, frame) in state.frames.iter().enumerate().take(6) {
                    let tex_id = format!("anim_preview_{}", idx);
                    let tex =
                        ui.ctx()
                            .load_texture(tex_id, frame.clone(), egui::TextureOptions::LINEAR);
                    ui.image((tex.id(), egui::vec2(96.0, 72.0)));
                }
            });
        });
    }
    ui.separator();
    ui.small("Tip: pedí al asistente 'explica derivada con animación' y usá GenerateAnimation.");
}

pub fn build_anim_params(state: &AnimPreviewState) -> Result<AnimParams, String> {
    let template = if state.template.is_empty() {
        "derivative-slope".to_string()
    } else {
        state.template.clone()
    };
    let concept = state.concept.trim().to_string();
    if concept.is_empty() {
        return Err("concepto vacío".into());
    }
    if concept.len() > 200 {
        return Err("concepto demasiado largo (max 200)".into());
    }
    let mut params = BTreeMap::new();
    params.insert("x0".to_string(), 1.0);
    let anim_params = AnimParams {
        template,
        concept,
        params,
        duration: AnimDuration::try_new(2.0).unwrap_or_default(),
        resolution: Resolution::try_new(640, 480).unwrap_or_default(),
        export: ExportFormat::Gif,
        spec: None,
    };
    anim_params.validate().map_err(|e| e.to_string())?;
    Ok(anim_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_state_lifecycle() {
        let mut s = AnimPreviewState::default();
        assert!(!s.is_active());
        s.progress = 0.5;
        assert!(s.is_active());
        s.clear();
        assert_eq!(s.progress, 0.0);
    }
    #[test]
    fn build_params_valid() {
        let state = AnimPreviewState {
            template: "derivative-slope".into(),
            concept: "derivada".into(),
            ..Default::default()
        };
        assert!(build_anim_params(&state).is_ok());
    }
    #[test]
    fn build_params_empty_concept_fails() {
        let state = AnimPreviewState::default();
        assert!(build_anim_params(&state).is_err());
    }
}
