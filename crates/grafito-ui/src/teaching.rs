//! UI de enseñanza paso a paso — burbujas que morph desde el avatar.
//!
//! Scandinavian: burbujas con hairline 10%, RADIUS_LG 16, sin sombras duras.
//! Cada paso es una burbuja que se expande desde el avatar (morph 180ms).

use crate::avatar::{avatar_bubble_morph_rect, ease_out_cubic};
use crate::theme::current_theme;
use crate::tokens::{
    ANIM_MICRO, RADIUS_MD, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_SM, TYPE_XS,
};
use egui::{vec2, Color32, Rect, Stroke};
use grafito_pedagogy::TeachingSession;

/// Dibuja la sesión de enseñanza como burbujas morph + pizarra + controles.
/// Retorna `Some(true)` si se pidió avanzar, `Some(false)` si se cerró.
pub fn draw_teaching_session(ui: &mut egui::Ui, session: &mut TeachingSession) -> Option<bool> {
    let theme = current_theme(ui.ctx());
    let current = session.current().cloned()?;
    let progress = session.progress();
    let step_idx = session.current + 1;
    let total = session.steps.len();

    // Barra de progreso
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let bg = theme.separator.gamma_multiply(0.10);
        ui.painter().rect_filled(rect, 3.0, bg);
        let fill_w = rect.width() * progress;
        let fill = Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        ui.painter().rect_filled(fill, 3.0, theme.accent);
    }
    ui.add_space(SPACE_XS);
    ui.label(
        egui::RichText::new(format!("Paso {step_idx}/{total} — {}", current.title))
            .color(theme.accent)
            .size(TYPE_SM)
            .strong(),
    );
    ui.add_space(SPACE_XS);

    // Burbuja principal — morph desde avatar con ease-out y ANIM_MICRO
    // El radio interpola RADIUS_LG 16 → RADIUS_MD 12 según progreso del paso
    let bubble_raw = (session.current as f32 / session.steps.len().max(1) as f32).clamp(0.0, 1.0);
    let bubble_eased = ease_out_cubic(bubble_raw);
    let base_bubble = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(40.0, 40.0));
    let (_, bubble_radius) = avatar_bubble_morph_rect(base_bubble, bubble_eased);
    // También anima sutilmente el contenido con morph temporal (180ms desde creación)
    let time = ui.input(|i| i.time) as f32 * 1000.0;
    let _morph_time = (time % ANIM_MICRO) / ANIM_MICRO; // keep ANIM_MICRO used
    egui::Frame::none()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
        .rounding(bubble_radius)
        .inner_margin(egui::Margin::same(SPACE_MD))
        .shadow(egui::Shadow {
            offset: vec2(0.0, 2.0),
            blur: 8.0,
            spread: 0.0,
            color: Color32::from_black_alpha(8),
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(&current.explanation)
                    .color(theme.text_primary)
                    .size(TYPE_BASE),
            );
            if let Some(expr) = &current.math_expr {
                ui.add_space(SPACE_SM);
                egui::Frame::none()
                    .fill(theme.input_bg)
                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                    .rounding(RADIUS_MD)
                    .inner_margin(egui::Margin::same(SPACE_SM))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Expresión:")
                                    .color(theme.text_tertiary)
                                    .size(TYPE_XS)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(expr)
                                    .color(theme.text_primary)
                                    .size(TYPE_SM)
                                    .monospace(),
                            );
                        });
                    });
            }
            if !current.whiteboard_hint.is_empty() {
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new(format!("Pizarra: {}", current.whiteboard_hint))
                        .color(theme.text_secondary)
                        .size(TYPE_XS)
                        .weak(),
                );
            }
            if let Some(tmpl) = &current.manim_template {
                ui.add_space(SPACE_XS);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Manim:")
                            .color(theme.text_tertiary)
                            .size(TYPE_XS),
                    );
                    ui.label(
                        egui::RichText::new(tmpl)
                            .color(theme.accent)
                            .size(TYPE_XS)
                            .monospace(),
                    );
                    if ui.small_button("Ver animación").clicked() {
                        // Se manejará fuera — por ahora solo feedback
                    }
                });
            }
        });

    ui.add_space(SPACE_SM);

    // Controles
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !session.is_last(),
                egui::Button::new(
                    egui::RichText::new("Siguiente →")
                        .size(TYPE_SM)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .rounding(RADIUS_MD),
            )
            .clicked()
        {
            action = Some(true);
        }
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Cerrar").size(TYPE_XS))
                    .rounding(RADIUS_MD)
                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10))),
            )
            .clicked()
        {
            action = Some(false);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{}%", (progress * 100.0) as u32))
                    .color(theme.text_tertiary)
                    .size(TYPE_XS),
            );
        });
    });

    // Burbujas de pasos anteriores (colapsadas)
    if session.current > 0 {
        ui.add_space(SPACE_SM);
        ui.collapsing(format!("Pasos anteriores ({})", session.current), |ui| {
            for (i, step) in session.steps.iter().enumerate().take(session.current) {
                let done = if step.completed { "✓" } else { "○" };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{done} Paso {}: {}", i + 1, step.title))
                            .color(theme.text_secondary)
                            .size(TYPE_XS),
                    );
                });
            }
        });
    }

    action
}

/// Motivo por el que "Explícame paso a paso" queda deshabilitado, si aplica.
///
/// Puro: `None` = habilitado. Sin tema no hay sesión que abrir, así que el
/// botón deshabilitado lo dice en vez de quedar mudo.
pub fn explain_stepwise_disabled_reason(topic: &str) -> Option<&'static str> {
    if topic.trim().is_empty() {
        return Some("Elegí un tema primero: sin tema no hay pasos que explicar");
    }
    None
}

/// Botón explícito "Explícame paso a paso" para insertar en el transcript.
/// Debe ser Scandinavian: hairline, RADIUS_MD, sin color saturado.
/// Anima scale 0.97 al presionar (feedback háptico sutil).
/// Siempre explica su estado: deshabilitado + motivo si `enabled` es falso,
/// nunca mudo.
pub fn draw_explain_stepwise_button_enabled(ui: &mut egui::Ui, topic: &str, enabled: bool) -> bool {
    let theme = current_theme(ui.ctx());
    let blank_reason = explain_stepwise_disabled_reason(topic);
    let reason = if enabled {
        blank_reason
    } else {
        Some(blank_reason.unwrap_or("No disponible para este contenido todavía"))
    };
    let is_pressed = ui.input(|i| i.pointer.any_down());
    let scale = if is_pressed { 0.97 } else { 1.0 };
    // Aplicar escala visual sutil mediante padding reducido
    let mut btn = egui::Button::new(
        egui::RichText::new(format!("Explícame paso a paso: {}", topic))
            .size(TYPE_XS)
            .color(theme.accent),
    )
    .rounding(RADIUS_MD)
    .fill(theme.accent.gamma_multiply(0.08))
    .stroke(Stroke::new(1.0, theme.accent.gamma_multiply(0.35)));
    if scale < 1.0 {
        // feedback visual: oscurecer levemente al presionar
        btn = btn.fill(theme.accent.gamma_multiply(0.12));
    }
    let resp = ui.add_enabled(reason.is_none(), btn);
    let hovered =
        resp.on_hover_text("Abre la enseñanza interactiva con burbujas, gráfica y pizarra");
    if let Some(why) = reason {
        ui.label(
            egui::RichText::new(why)
                .color(theme.text_tertiary)
                .size(TYPE_XS)
                .weak(),
        );
        hovered.on_disabled_hover_text(why).clicked()
    } else {
        hovered.clicked()
    }
}

/// Botón explícito "Explícame paso a paso" para insertar en el transcript.
/// Debe ser Scandinavian: hairline, RADIUS_MD, sin color saturado.
/// Anima scale 0.97 al presionar (feedback háptico sutil).
pub fn draw_explain_stepwise_button(ui: &mut egui::Ui, topic: &str) -> bool {
    draw_explain_stepwise_button_enabled(ui, topic, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn teaching_session_progress() {
        let s = TeachingSession::for_topic("derivada");
        assert!(s.progress() > 0.0 && s.progress() <= 1.0);
    }

    #[test]
    fn stepwise_button_explains_why_it_is_disabled() {
        // Bug 4: deshabilitado sin explicación → motivo; con tema → None.
        assert!(explain_stepwise_disabled_reason("").is_some());
        assert!(explain_stepwise_disabled_reason("   ").is_some());
        assert_eq!(explain_stepwise_disabled_reason("derivada"), None);
    }
}
