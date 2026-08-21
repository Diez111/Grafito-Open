//! Panel pedagógico — ejercicios y feedback socrático (sin bloquear UI).
//!
//! Usa `grafito-pedagogy` puro para generar ejercicios y evaluar.

use egui::Color32;
use grafito_pedagogy::curriculum::{Curriculum, LearningObjective};
use grafito_pedagogy::{
    Exercise, ExerciseGenerator, FeedbackEngine, PedagogicalLevel, ScaffoldEngine,
};

#[derive(Debug, Clone, Default)]
pub struct PedagogyPanelState {
    pub concept: String,
    pub level: PedagogicalLevel,
    pub exercise: Option<Exercise>,
    pub answer: String,
    pub feedback: Option<grafito_pedagogy::Feedback>,
    pub scaffold: Option<grafito_pedagogy::Scaffold>,
}

pub fn draw_pedagogy_panel(ui: &mut egui::Ui, state: &mut PedagogyPanelState) {
    ui.heading("Tutor Pedagógico");
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Concepto:");
        ui.text_edit_singleline(&mut state.concept);
        if ui.button("Scaffold").clicked() {
            let engine = ScaffoldEngine;
            state.scaffold = Some(engine.scaffold(&state.concept, state.level, &[]));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Nivel:");
        egui::ComboBox::from_id_salt("pedagogy_level")
            .selected_text(state.level.label())
            .show_ui(ui, |ui| {
                for lvl in [
                    PedagogicalLevel::Primary,
                    PedagogicalLevel::Secondary,
                    PedagogicalLevel::University,
                ] {
                    ui.selectable_value(&mut state.level, lvl, lvl.label());
                }
            });
    });
    if let Some(scaffold) = &state.scaffold {
        ui.collapsing("Andamiaje socrático", |ui| {
            ui.label(egui::RichText::new(&scaffold.question).strong());
            if let Some(hint) = &scaffold.hint {
                ui.label(egui::RichText::new(format!("Pista: {}", hint)).color(Color32::GRAY));
            }
            ui.label(&scaffold.explanation);
        });
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Generar ejercicio").clicked() {
            let lo = Curriculum::find_for_concept(&state.concept)
                .into_iter()
                .next()
                .unwrap_or_else(|| {
                    LearningObjective::new("custom", &state.concept, &state.concept, None)
                });
            let ex = ExerciseGenerator.generate(&lo, state.level);
            state.exercise = Some(ex);
            state.answer.clear();
            state.feedback = None;
        }
        if ui.button("Limpiar").clicked() {
            state.exercise = None;
            state.feedback = None;
        }
    });
    if let Some(ex) = &state.exercise {
        ui.group(|ui| {
            ui.label(egui::RichText::new(&ex.prompt).strong());
            ui.horizontal(|ui| {
                ui.label("Tu respuesta:");
                ui.text_edit_singleline(&mut state.answer);
                if ui.button("Evaluar").clicked() {
                    let fb = FeedbackEngine.assess(ex, &state.answer);
                    state.feedback = Some(fb);
                }
            });
        });
    }
    if let Some(fb) = &state.feedback {
        let color = if fb.correct {
            Color32::from_rgb(40, 180, 70)
        } else {
            Color32::from_rgb(200, 60, 60)
        };
        ui.colored_label(color, &fb.message);
        ui.label(&fb.next_step);
    }
    ui.small("Tip: el nivel se sincroniza con tu perfil (Streak/XP) y con el concepto del canvas.");
}
