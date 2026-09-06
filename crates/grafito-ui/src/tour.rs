//! Tour de onboarding 3–5 pasos — primera corrida sin laberinto.
//!
//! Piel pura: `fn render(&Estado) -> Frame`. Todo botón avanza, retrocede u
//! omite el tour; ninguno es mudo. Sin I/O, sin spawn, sólo tokens.

use crate::tokens::{RADIUS_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_SM, TYPE_XS};

/// Un paso del tour: qué mostrar y a qué apunta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TourStep {
    /// Título corto del paso.
    pub title: &'static str,
    /// Cuerpo rioplatense, 1–2 líneas.
    pub body: &'static str,
    /// Dónde mirar (nombre honesto del sector, no un selector frágil).
    pub target: &'static str,
}

/// 4 pasos: lienzo → entrada → paleta → animar. Ni 3 escuetos ni 6 eternos.
pub const TOUR_STEPS: &[TourStep] = &[
    TourStep {
        title: "1 · Lienzo",
        body: "Arrastrá para mover la vista, rueda para zoom. Todo lo que crees aparece acá.",
        target: "lienzo central",
    },
    TourStep {
        title: "2 · Entrada",
        body: "Escribí «f(x)=x^2» o «A=(1,2)» y Enter. El keypad Σ inserta símbolos.",
        target: "barra de entrada",
    },
    TourStep {
        title: "3 · Paleta",
        body: "Ctrl+K busca 200+ comandos en español o inglés, con recientes arriba.",
        target: "paleta de comandos",
    },
    TourStep {
        title: "4 · Animar",
        body: "Creá un deslizador y dale Play: el punto recorre la traza en vivo.",
        target: "panel de sliders",
    },
];

/// Estado del tour. `step: None` = no iniciado o terminado.
#[derive(Debug, Clone, Default)]
pub struct TourState {
    step: Option<usize>,
    dismissed: bool,
}

impl TourState {
    /// Arranca (o reinicia) el tour en el paso 0.
    pub fn start(&mut self) {
        self.step = Some(0);
        self.dismissed = false;
    }

    /// Paso actual, si el tour está en curso.
    pub fn current(&self) -> Option<&'static TourStep> {
        self.step.and_then(|index| TOUR_STEPS.get(index))
    }

    /// Índice actual (para tests y paginación "2 de 4").
    pub fn index(&self) -> Option<usize> {
        self.step
    }

    /// ¿El tour está en curso?
    pub fn is_active(&self) -> bool {
        self.step.is_some()
    }

    /// ¿El usuario lo omitió explícitamente?
    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    /// Avanza; al pasar el último paso el tour termina (no queda colgado).
    pub fn next(&mut self) {
        match self.step {
            Some(index) if index + 1 < TOUR_STEPS.len() => self.step = Some(index + 1),
            _ => {
                self.step = None;
            }
        }
    }

    /// Retrocede; en el paso 0 se queda (no va a negativo).
    pub fn back(&mut self) {
        match self.step {
            Some(0) | None => {}
            Some(index) => self.step = Some(index - 1),
        }
    }

    /// Omite el tour (no vuelve a molestar en la sesión).
    pub fn skip(&mut self) {
        self.step = None;
        self.dismissed = true;
    }

    /// Texto de progreso honesto: "2 de 4" o vacío si no hay tour.
    pub fn progress_text(&self) -> String {
        match self.step {
            Some(index) => format!("{} de {}", index + 1, TOUR_STEPS.len()),
            None => String::new(),
        }
    }
}

/// Tarjeta del tour. Todos los botones mutan el estado (cero mudos).
pub fn draw_onboarding_tour(ui: &mut egui::Ui, state: &mut TourState) {
    let Some(index) = state.index() else {
        return;
    };
    let Some(step) = TOUR_STEPS.get(index) else {
        return;
    };
    egui::Frame::none()
        .fill(ui.visuals().panel_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .rounding(RADIUS_MD)
        .inner_margin(SPACE_SM)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(step.title).size(TYPE_BASE).strong());
            ui.add_space(SPACE_XS);
            ui.label(egui::RichText::new(step.body).size(TYPE_SM));
            ui.label(
                egui::RichText::new(format!("Mirá: {}", step.target))
                    .size(TYPE_XS)
                    .weak(),
            );
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                if ui
                    .button("← Atrás")
                    .on_hover_text("Volver al paso anterior del tour")
                    .clicked()
                {
                    state.back();
                }
                let next_label = if index + 1 == TOUR_STEPS.len() {
                    "Terminar ✓"
                } else {
                    "Siguiente →"
                };
                if ui
                    .button(next_label)
                    .on_hover_text("Avanzar al siguiente paso del tour")
                    .clicked()
                {
                    state.next();
                }
                if ui
                    .small_button("Omitir tour")
                    .on_hover_text("Cerrar el tour; podés reabrirlo desde Ayuda")
                    .clicked()
                {
                    state.skip();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(state.progress_text())
                            .size(TYPE_XS)
                            .weak(),
                    );
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_has_between_3_and_5_steps() {
        assert!(
            (3..=5).contains(&TOUR_STEPS.len()),
            "tour de {} pasos",
            TOUR_STEPS.len()
        );
        for step in TOUR_STEPS {
            assert!(!step.title.is_empty());
            assert!(!step.body.is_empty());
            assert!(!step.target.is_empty());
        }
    }

    #[test]
    fn lifecycle_start_next_back_skip() {
        let mut tour = TourState::default();
        assert!(!tour.is_active());
        assert!(tour.progress_text().is_empty());
        tour.start();
        assert!(tour.is_active());
        assert_eq!(tour.progress_text(), "1 de 4");
        tour.next();
        assert_eq!(tour.index(), Some(1));
        tour.back();
        assert_eq!(tour.index(), Some(0));
        tour.back();
        assert_eq!(tour.index(), Some(0));
        tour.skip();
        assert!(!tour.is_active());
        assert!(tour.is_dismissed());
    }

    #[test]
    fn finishing_last_step_ends_tour_without_hang() {
        let mut tour = TourState::default();
        tour.start();
        for _ in 0..TOUR_STEPS.len() {
            tour.next();
        }
        assert!(!tour.is_active());
        assert!(!tour.is_dismissed());
    }
}
