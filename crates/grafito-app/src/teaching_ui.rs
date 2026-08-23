//! UI de enseñanza — integra TeachingSession + Whiteboard + ManimOrchestrator.
//!
//! Muestra burbujas morph desde el avatar, pizarra para cada paso y
//! controles para avanzar. Usa `grafito-whiteboard` para dibujo y
//! `manim_orchestrator` para animaciones 3b1b (con fallback nativo).

use crate::manim_orchestrator::{ManimOrchestrator, OrchestratorState};
use crate::whiteboard_ui::WhiteboardSession;
use egui::{Color32, Stroke};
use grafito_pedagogy::TeachingSession;
use grafito_whiteboard::WhiteboardDoc;
use std::time::{Duration, Instant};

/// Estado de la UI de enseñanza.
#[derive(Default)]
pub struct TeachingUiState {
    pub session: Option<TeachingSession>,
    pub whiteboard: WhiteboardSession,
    pub orchestrator: ManimOrchestrator,
    pub show_manim: bool,
    /// Instant en que se abrió la overlay — para morph avatar→burbuja (ANIM_MICRO 180ms ease-out).
    pub opened_at: Option<Instant>,
    /// Frames nativos generados al completar el orchestrator (fallback sin Manim).
    pub anim_frames: Option<Vec<egui::ColorImage>>,
    /// Texturas cacheadas de `anim_frames` (creadas lazily con `ctx.load_texture`).
    pub anim_textures: Vec<egui::TextureHandle>,
}

impl TeachingUiState {
    pub fn start(&mut self, topic: &str) {
        let session = TeachingSession::for_topic(topic);
        // Inicializar pizarra con hint del primer paso
        if let Some(step) = session.current() {
            let doc = WhiteboardDoc::default();
            let _ = &step.whiteboard_hint;
            self.whiteboard.doc = doc;
            // Iniciar orquestación manim para el primer paso — cancela cualquier job previo
            if let Some(tmpl) = &step.manim_template {
                self.orchestrator.cancel();
                let _ = self.orchestrator.start(topic, tmpl.clone());
            }
        }
        self.session = Some(session);
        self.opened_at = Some(Instant::now());
        self.anim_frames = None;
        self.anim_textures.clear();
    }
    pub fn advance(&mut self) -> bool {
        if let Some(session) = &mut self.session {
            let ok = session.advance();
            if let Some(step) = session.current() {
                if let Some(tmpl) = &step.manim_template {
                    // Avanzar implica nuevo concepto → cancelar previo y relanzar
                    self.orchestrator.cancel();
                    let _ = self.orchestrator.start(&step.title, tmpl.clone());
                    self.anim_frames = None;
                    self.anim_textures.clear();
                }
            }
            // Reinicia morph para el nuevo paso (burbuja entra de nuevo)
            self.opened_at = Some(Instant::now());
            ok
        } else {
            false
        }
    }
    pub fn close(&mut self) {
        self.session = None;
        self.orchestrator.cancel();
        self.opened_at = None;
        self.anim_frames = None;
        self.anim_textures.clear();
    }
    pub fn tick(&mut self, now: Instant) {
        if let Some(state) = self.orchestrator.tick(now) {
            // Al completar, generar fallback nativo via anim_native si no hay artefacto real
            if matches!(state, OrchestratorState::Completed { .. }) && self.anim_frames.is_none() {
                let concept = self.orchestrator.concept.clone();
                let template = self.orchestrator.template.clone();
                let frames =
                    crate::anim_native::render_anim_for_concept(&template, &concept, 320, 180);
                self.anim_frames = Some(frames);
                self.anim_textures.clear();
            }
            let _ = state;
        }
    }
    /// Progreso 0..=1 del morph burbuja (ANIM_MICRO 180ms ease-out).
    pub fn morph_progress(&self) -> f32 {
        let Some(opened) = self.opened_at else {
            return 1.0;
        };
        let elapsed_ms = Instant::now().duration_since(opened).as_secs_f32() * 1000.0;
        (elapsed_ms / grafito_ui::tokens::ANIM_MICRO).clamp(0.0, 1.0)
    }
}

/// Dibuja la enseñanza si hay sesión activa. Retorna true si se cerró.
pub fn draw_teaching_overlay(state: &mut TeachingUiState, ctx: &egui::Context) -> bool {
    if state.session.is_none() {
        return false;
    }
    // Lazily crear texturas del fallback nativo (antes de snapshot para evitar borrow cruzado)
    if let Some(frames) = &state.anim_frames {
        if state.anim_textures.is_empty() && !frames.is_empty() {
            // Clona frames para crear texturas con ctx (necesita &mut statetextures)
            let cloned: Vec<egui::ColorImage> = frames.clone();
            state.anim_textures = cloned
                .into_iter()
                .enumerate()
                .map(|(i, img)| {
                    ctx.load_texture(
                        format!("teaching_anim_{i}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    )
                })
                .collect();
        }
    }
    // Snapshot inmutable para el closure (evita borrow cruzado &mut state.session + &state.orchestrator)
    let opened_at = state.opened_at;
    let session_snapshot = state.session.clone().unwrap();
    let ledger = state.orchestrator.ledger.clone();
    let template = state.orchestrator.template.clone();
    let is_busy = state.orchestrator.is_busy();
    let anim_textures: Vec<egui::TextureHandle> = state.anim_textures.clone();
    let progress = session_snapshot.progress();
    let is_last = session_snapshot.is_last();
    let topic_label = session_snapshot.topic.label();
    let step_count = session_snapshot.steps.len();
    let current_idx = session_snapshot.current;
    let current_step = session_snapshot.current().cloned();

    let mut should_close = false;
    let mut should_advance = false;
    let theme = grafito_ui::theme::current_theme(ctx);
    egui::Window::new("Enseñanza — Paso a paso")
        .id(egui::Id::new("teaching_overlay"))
        .collapsible(false)
        .resizable(true)
        .default_width(640.0)
        .min_width(480.0)
        .max_width(800.0)
        .default_height(520.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel_bg)
                .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                .rounding(grafito_ui::tokens::RADIUS_LG)
                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
                .shadow(egui::Shadow {
                    offset: egui::vec2(0.0, 2.0),
                    blur: 8.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(8),
                }),
        )
        .show(ctx, |ui| {
            let time = ui.input(|i| i.time);
            // Header con avatar morph a burbuja (ANIM_MICRO 180ms ease-out)
            ui.horizontal(|ui| {
                // Progreso morph 0..1 con ease-out cúbico basado en opened_at snapshot
                let raw_t = opened_at
                    .map(|opened| {
                        let elapsed_ms =
                            Instant::now().duration_since(opened).as_secs_f32() * 1000.0;
                        (elapsed_ms / grafito_ui::tokens::ANIM_MICRO).clamp(0.0, 1.0)
                    })
                    .unwrap_or(1.0);
                let eased = 1.0 - (1.0 - raw_t).powi(3);
                if raw_t < 1.0 {
                    ui.ctx().request_repaint();
                }
                // Base 36×36 avatar, morph rect interpolado via avatar_bubble_morph_rect
                let base_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(36.0, 36.0));
                let (target_rect, target_radius) =
                    grafito_ui::avatar::avatar_bubble_morph_rect(base_rect, eased);
                let target_size = target_rect.size();
                let (alloc_rect, _) = ui.allocate_exact_size(target_size, egui::Sense::hover());
                let morph_rect = egui::Rect::from_min_size(alloc_rect.min, target_size);
                // Fondo burbuja que se expande desde el avatar
                ui.painter().rect_filled(
                    morph_rect,
                    target_radius,
                    theme.accent.gamma_multiply(0.08),
                );
                ui.painter().rect_stroke(
                    morph_rect,
                    target_radius,
                    Stroke::new(1.0, theme.accent.gamma_multiply(0.12)),
                );
                // Avatar centrado dentro de la burbuja, con press scale 0.97
                let hover_pos = ui.input(|i| i.pointer.hover_pos());
                let cfg = grafito_profile::AvatarConfig::default();
                let is_pressed = ui.input(|i| i.pointer.any_down())
                    && alloc_rect
                        .contains(hover_pos.unwrap_or(egui::pos2(f32::INFINITY, f32::INFINITY)));
                let scale = if is_pressed { 0.97 } else { 1.0 };
                let avatar_size = 32.0 * scale;
                let avatar_rect = egui::Rect::from_center_size(
                    morph_rect.center(),
                    egui::vec2(avatar_size, avatar_size),
                );
                let painter = ui.painter_at(avatar_rect);
                grafito_ui::avatar::draw_avatar(&painter, avatar_rect, &cfg, time, hover_pos);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(topic_label.clone())
                            .strong()
                            .size(grafito_ui::tokens::TYPE_BASE)
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("Paso {}/{}", current_idx + 1, step_count))
                            .size(grafito_ui::tokens::TYPE_XS)
                            .color(theme.text_secondary),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕ Cerrar").clicked() {
                        should_close = true;
                    }
                });
            });
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Barra progreso
            let (r, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(r, 2.0, theme.separator.gamma_multiply(0.10));
            ui.painter().rect_filled(
                egui::Rect::from_min_size(r.min, egui::vec2(r.width() * progress, r.height())),
                2.0,
                theme.accent,
            );
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Burbuja principal — snapshot del paso actual
            if let Some(step) = current_step {
                egui::Frame::none()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                    .rounding(grafito_ui::tokens::RADIUS_LG)
                    .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(&step.title)
                                .strong()
                                .size(grafito_ui::tokens::TYPE_MD)
                                .color(theme.accent),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(&step.explanation)
                                .size(grafito_ui::tokens::TYPE_BASE)
                                .color(theme.text_primary),
                        );
                        if let Some(expr) = &step.math_expr {
                            ui.add_space(grafito_ui::tokens::SPACE_SM);
                            egui::Frame::none()
                                .fill(theme.input_bg)
                                .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                                .rounding(grafito_ui::tokens::RADIUS_MD)
                                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(expr)
                                            .monospace()
                                            .size(grafito_ui::tokens::TYPE_SM)
                                            .color(theme.text_primary),
                                    );
                                });
                        }
                        if !step.whiteboard_hint.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!("Pizarra: {}", step.whiteboard_hint))
                                    .size(grafito_ui::tokens::TYPE_XS)
                                    .color(theme.text_tertiary)
                                    .weak(),
                            );
                        }
                    });
                // Whiteboard preview (mini)
                ui.add_space(grafito_ui::tokens::SPACE_SM);
                let (wb_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 120.0),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(wb_rect) {
                    ui.painter().rect_filled(
                        wb_rect,
                        grafito_ui::tokens::RADIUS_MD,
                        theme.input_bg,
                    );
                    ui.painter().rect_stroke(
                        wb_rect,
                        grafito_ui::tokens::RADIUS_MD,
                        Stroke::new(1.0, theme.separator.gamma_multiply(0.10)),
                    );
                    ui.painter().text(
                        wb_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("Pizarra: {}", step.whiteboard_hint),
                        egui::FontId::proportional(11.0),
                        theme.text_tertiary,
                    );
                }
                // Animación nativa fallback (si completó)
                if !anim_textures.is_empty() {
                    ui.add_space(grafito_ui::tokens::SPACE_SM);
                    let time = ui.input(|i| i.time);
                    let idx = ((time * 12.0) as usize) % anim_textures.len();
                    let tex = &anim_textures[idx];
                    let max_w = ui.available_width().max(80.0);
                    let size = tex.size_vec2();
                    let scale = (max_w / size.x.max(1.0)).clamp(0.25, 1.0);
                    let display = egui::vec2(size.x * scale, size.y * scale).ceil();
                    let (rect, _) = ui.allocate_exact_size(display, egui::Sense::hover());
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.painter().rect_stroke(
                        rect,
                        grafito_ui::tokens::RADIUS_MD,
                        Stroke::new(1.0, theme.separator.gamma_multiply(0.10)),
                    );
                    ui.ctx().request_repaint_after(Duration::from_millis(80));
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Animación: {} — {} frames (fallback nativo)",
                            template,
                            anim_textures.len()
                        ))
                        .size(grafito_ui::tokens::TYPE_XS)
                        .color(theme.text_tertiary)
                        .weak(),
                    );
                } else if is_busy {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let t = ui.input(|i| i.time);
                        let pulse = ((t * 3.0).sin() + 1.0) * 0.5;
                        let col = theme.accent.gamma_multiply(0.45 + 0.55 * pulse as f32);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, col);
                        ui.label(
                            egui::RichText::new("Generando animación con Manim…")
                                .size(grafito_ui::tokens::TYPE_XS)
                                .color(theme.text_secondary),
                        );
                    });
                    ui.ctx().request_repaint_after(Duration::from_millis(48));
                }
                // Manim orchestrator ledger
                if let Some(ledger) = &ledger {
                    ui.add_space(4.0);
                    egui::Frame::none()
                        .fill(theme.input_bg)
                        .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                        .rounding(grafito_ui::tokens::RADIUS_MD)
                        .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(ledger)
                                    .monospace()
                                    .size(10.0)
                                    .color(theme.text_secondary),
                            );
                        });
                }
            }
            ui.add_space(grafito_ui::tokens::SPACE_MD);
            ui.horizontal(|ui| {
                let btn_label = if is_last {
                    "Finalizar ✓"
                } else {
                    "Siguiente →"
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(btn_label)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .rounding(grafito_ui::tokens::RADIUS_MD),
                    )
                    .clicked()
                {
                    if is_last {
                        should_close = true;
                    } else {
                        should_advance = true;
                    }
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Cerrar").size(grafito_ui::tokens::TYPE_XS),
                        )
                        .rounding(grafito_ui::tokens::RADIUS_MD)
                        .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10))),
                    )
                    .clicked()
                {
                    should_close = true;
                }
            });
        });
    if should_advance {
        state.advance();
    }
    if should_close {
        state.close();
        true
    } else {
        false
    }
}
