//! Panel de animaciones — preview, progreso y export sin bloquear la UI.
//!
//! Estado puro en GrafitoApp::anim_preview, render en background thread,
//! progreso via egui::ProgressBar y ctx.request_repaint().
//! Separacion Cerebro/Piel: este modulo solo renderiza &AnimPreviewState.

use egui::{Color32, ProgressBar, ScrollArea};
use grafito_anim::{AnimDuration, AnimParams, ExportFormat, Resolution};
use std::collections::BTreeMap;

pub struct AnimPreviewState {
    pub template: String,
    pub concept: String,
    pub progress: f32,
    pub status: String,
    pub media_path: Option<String>,
    pub frames: Vec<egui::ColorImage>,
    pub source_turn: Option<usize>,
    // Cache de texturas para evitar clone+load cada frame (27KB×6 leak).
    cached_textures: Vec<egui::TextureHandle>,
    cached_hash: u64,
    cached_len: usize,
}

impl Default for AnimPreviewState {
    fn default() -> Self {
        Self {
            template: String::new(),
            concept: String::new(),
            progress: 0.0,
            status: String::new(),
            media_path: None,
            frames: Vec::new(),
            source_turn: None,
            cached_textures: Vec::new(),
            cached_hash: 0,
            cached_len: 0,
        }
    }
}

impl AnimPreviewState {
    pub fn is_active(&self) -> bool {
        self.progress > 0.0 && self.progress < 1.0
    }
    pub fn clear(&mut self) {
        self.cached_textures.clear();
        *self = Self::default();
    }

    /// Limpia texturas explicitamente con context (libera GPU). Llamar desde UI con ctx.
    pub fn clear_with_ctx(&mut self, ctx: &egui::Context) {
        for (idx, _) in self.cached_textures.iter().enumerate() {
            // TextureHandle Drop libera, pero forget_image asegura limpieza si el id fue reutilizado.
            ctx.forget_image(&format!("anim_preview_{}", idx));
        }
        self.cached_textures.clear();
        *self = Self::default();
    }

    fn frames_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.frames.len().hash(&mut hasher);
        for f in self.frames.iter().take(6) {
            f.size.hash(&mut hasher);
            // Hash pixels: suficiente para detectar cambio; acotado a 6 frames × 27KB
            for px in &f.pixels {
                px.r().hash(&mut hasher);
                px.g().hash(&mut hasher);
                px.b().hash(&mut hasher);
                px.a().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.frames.is_empty() {
            if !self.cached_textures.is_empty() {
                for (idx, _) in self.cached_textures.iter().enumerate() {
                    ctx.forget_image(&format!("anim_preview_{}", idx));
                }
                self.cached_textures.clear();
                self.cached_hash = 0;
                self.cached_len = 0;
            }
            return;
        }
        let hash = self.frames_hash();
        let len = self.frames.len().min(6);
        if self.cached_textures.len() == len && self.cached_hash == hash && self.cached_len == len {
            return;
        }
        // Libera anteriores
        for (idx, _) in self.cached_textures.iter().enumerate() {
            ctx.forget_image(&format!("anim_preview_{}", idx));
        }
        self.cached_textures.clear();
        // Solo clona cuando cambia; evita clone 27KB×6 cada frame.
        self.cached_textures = self
            .frames
            .iter()
            .take(6)
            .enumerate()
            .map(|(idx, frame)| {
                let tex_id = format!("anim_preview_{}", idx);
                // frame.clone() solo aquí, cuando hash cambió.
                ctx.load_texture(tex_id, frame.clone(), egui::TextureOptions::LINEAR)
            })
            .collect();
        self.cached_hash = hash;
        self.cached_len = len;
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
            state.clear_with_ctx(ui.ctx());
        }
    }
    if !state.frames.is_empty() {
        state.ensure_textures(ui.ctx());
        ScrollArea::horizontal().max_height(84.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                for tex in state.cached_textures.iter() {
                    ui.image((tex.id(), egui::vec2(96.0, 72.0)));
                }
            });
        });
    } else {
        // Asegura limpieza si frames se vació externamente
        if !state.cached_textures.is_empty() {
            for (idx, _) in state.cached_textures.iter().enumerate() {
                ui.ctx().forget_image(&format!("anim_preview_{}", idx));
            }
            state.cached_textures.clear();
        }
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
