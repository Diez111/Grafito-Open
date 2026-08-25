//! UI de enseñanza — integra TeachingSession + Whiteboard + ManimOrchestrator.
//!
//! Muestra burbujas morph desde el avatar, pizarra para cada paso y
//! controles para avanzar. Usa `grafito-whiteboard` para dibujo y
//! `manim_orchestrator` para animaciones 3b1b (con fallback nativo).

use crate::manim_orchestrator::{ManimOrchestrator, OrchestratorState};
use crate::whiteboard_ui::WhiteboardSession;
use egui::{Color32, Stroke};
use grafito_pedagogy::TeachingSession;
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_whiteboard::WhiteboardDoc;
use std::time::{Duration, Instant};

/// Estado de la UI de enseñanza.
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
    cached_hash: u64,
    cached_len: usize,
}

#[allow(clippy::derivable_impls)]
impl Default for TeachingUiState {
    fn default() -> Self {
        Self {
            session: None,
            whiteboard: WhiteboardSession::default(),
            orchestrator: ManimOrchestrator::default(),
            show_manim: false,
            opened_at: None,
            anim_frames: None,
            anim_textures: Vec::new(),
            cached_hash: 0,
            cached_len: 0,
        }
    }
}

fn whiteboard_elements_for_hint(hint: &str) -> Vec<grafito_whiteboard::WhiteboardElement> {
    use grafito_whiteboard::WhiteboardElement;
    let lower = hint.to_lowercase();
    let mut elems = Vec::new();
    if lower.contains("secante") || lower.contains("tangente") || lower.contains("pendiente") {
        // Curva x² como trazo suave + secante + tangente
        elems.push(WhiteboardElement::Stroke {
            points: vec![
                (-3.0, 9.0),
                (-2.0, 4.0),
                (-1.0, 1.0),
                (0.0, 0.0),
                (1.0, 1.0),
                (2.0, 4.0),
            ],
            color: (55, 55, 55),
            width: 2.0,
        });
        elems.push(WhiteboardElement::Arrow {
            from: (-1.0, 1.0),
            to: (1.0, 1.0),
        });
        elems.push(WhiteboardElement::Arrow {
            from: (0.5, 0.25),
            to: (1.5, 2.25),
        });
    } else if lower.contains("rectángulo")
        || lower.contains("rectangulo")
        || lower.contains("riemann")
        || lower.contains("área")
        || lower.contains("area")
    {
        // 4 rectángulos de Riemann bajo x² entre 0 y 2
        for i in 0..4 {
            let x = i as f64 * 0.5;
            let y = x * x * 0.5 + 0.2;
            elems.push(WhiteboardElement::Rectangle {
                min: (x, 0.0),
                max: (x + 0.45, y),
                fill: None,
            });
        }
        elems.push(WhiteboardElement::Stroke {
            points: vec![(0.0, 0.0), (2.0, 2.0)],
            color: (55, 55, 55),
            width: 1.5,
        });
    } else if lower.contains("pitágoras")
        || lower.contains("pitagoras")
        || lower.contains("triángulo")
    {
        elems.push(WhiteboardElement::Rectangle {
            min: (-2.0, -1.0),
            max: (0.0, 1.0),
            fill: None,
        });
        elems.push(WhiteboardElement::Rectangle {
            min: (0.0, -1.0),
            max: (2.0, 1.0),
            fill: None,
        });
        elems.push(WhiteboardElement::Stroke {
            points: vec![(-2.0, -1.0), (2.0, -1.0), (0.0, 1.0), (-2.0, -1.0)],
            color: (55, 55, 55),
            width: 2.0,
        });
    } else if lower.contains("pizarra libre") || lower.contains("libre") {
        // Vacía para que el usuario dibuje
    } else if !hint.trim().is_empty() {
        // Fallback: texto centrado
        elems.push(WhiteboardElement::Text {
            at: (-1.5, 0.0),
            text: hint.chars().take(40).collect(),
            size: 14.0,
        });
    }
    elems
}

impl TeachingUiState {
    fn anim_frames_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let Some(frames) = &self.anim_frames else {
            return 0;
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        frames.len().hash(&mut hasher);
        for frame in frames.iter().take(6) {
            frame.size.hash(&mut hasher);
            for pixel in &frame.pixels {
                pixel.r().hash(&mut hasher);
                pixel.g().hash(&mut hasher);
                pixel.b().hash(&mut hasher);
                pixel.a().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        let Some(frames) = &self.anim_frames else {
            if !self.anim_textures.is_empty() {
                for idx in 0..self.anim_textures.len() {
                    ctx.forget_image(&format!("teaching_anim_{idx}"));
                }
                self.anim_textures.clear();
                self.cached_hash = 0;
                self.cached_len = 0;
            }
            return;
        };
        if frames.is_empty() {
            if !self.anim_textures.is_empty() {
                for idx in 0..self.anim_textures.len() {
                    ctx.forget_image(&format!("teaching_anim_{idx}"));
                }
                self.anim_textures.clear();
                self.cached_hash = 0;
                self.cached_len = 0;
            }
            return;
        }
        let hash = self.anim_frames_hash();
        let len = frames.len();
        if self.anim_textures.len() == len && self.cached_hash == hash && self.cached_len == len {
            return;
        }
        for idx in 0..self.anim_textures.len() {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        self.anim_textures.clear();
        self.anim_textures = frames
            .iter()
            .enumerate()
            .map(|(idx, frame)| {
                ctx.load_texture(
                    format!("teaching_anim_{idx}"),
                    frame.clone(),
                    egui::TextureOptions::LINEAR,
                )
            })
            .collect();
        self.cached_hash = hash;
        self.cached_len = len;
    }

    pub fn clear(&mut self) {
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
        self.anim_frames = None;
    }

    pub fn clear_with_ctx(&mut self, ctx: &egui::Context) {
        for idx in 0..self.anim_textures.len() {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
        self.anim_frames = None;
    }

    fn clear_anim_textures_only(&mut self, ctx: Option<&egui::Context>) {
        if let Some(ctx) = ctx {
            for idx in 0..self.anim_textures.len() {
                ctx.forget_image(&format!("teaching_anim_{idx}"));
            }
        }
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
    }

    pub fn start(&mut self, topic: &str) {
        let session = TeachingSession::for_topic(topic);
        // Inicializar pizarra con elementos vectoriales reales según hint
        if let Some(step) = session.current() {
            let mut doc = WhiteboardDoc::default();
            for elem in whiteboard_elements_for_hint(&step.whiteboard_hint) {
                doc.add(elem);
            }
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
        self.clear_anim_textures_only(None);
    }
    pub fn advance(&mut self) -> bool {
        if let Some(session) = &mut self.session {
            let ok = session.advance();
            if let Some(step) = session.current() {
                // Hidratar pizarra del nuevo paso
                let mut doc = WhiteboardDoc::default();
                for elem in whiteboard_elements_for_hint(&step.whiteboard_hint) {
                    doc.add(elem);
                }
                self.whiteboard.doc = doc;
                if let Some(tmpl) = &step.manim_template {
                    // Avanzar implica nuevo concepto → cancelar previo y relanzar
                    self.orchestrator.cancel();
                    let _ = self.orchestrator.start(&step.title, tmpl.clone());
                    self.anim_frames = None;
                    self.clear_anim_textures_only(None);
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
        self.clear_anim_textures_only(None);
    }

    pub fn close_with_ctx(&mut self, ctx: &egui::Context) {
        self.session = None;
        self.orchestrator.cancel();
        self.opened_at = None;
        self.anim_frames = None;
        self.clear_anim_textures_only(Some(ctx));
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
                self.clear_anim_textures_only(None);
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
    // Unifica patrón cached_hash + ensure_textures como en anim_ui.rs:61 — evita clone masivo y leak.
    state.ensure_textures(ctx);
    // Snapshot inmutable para el closure (evita borrow cruzado &mut state.session + &state.orchestrator)
    let opened_at = state.opened_at;
    let Some(session_snapshot) = state.session.clone() else {
        return false;
    };
    let ledger = state.orchestrator.ledger.clone();
    let template = state.orchestrator.template.clone();
    let is_busy = state.orchestrator.is_busy();
    // Evita clone en draw: sólo se clonan handles si hash cambió (ensure_textures). Aquí se snapshot sin clonar frames.
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
    let _ = opened_at;
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
                    offset: egui::vec2(0.0, grafito_ui::tokens::SHADOW_WINDOW_OFFSET_Y),
                    blur: grafito_ui::tokens::SHADOW_WINDOW_BLUR,
                    spread: 0.0,
                    color: Color32::from_black_alpha(grafito_ui::tokens::SHADOW_ALPHA),
                }),
        )
        .show(ctx, |ui| {
            let time = ui.input(|i| i.time);
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            // Header Scandinavian — left-aligned, avatar pequeño + título, cierre con icono
            ui.horizontal(|ui| {
                let cfg = grafito_profile::AvatarConfig::default();
                let (avatar_rect, _) = ui.allocate_exact_size(
                    egui::vec2(grafito_ui::tokens::TYPE_XXL, grafito_ui::tokens::TYPE_XXL),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(avatar_rect) {
                    let painter = ui.painter_at(avatar_rect);
                    grafito_ui::avatar::draw_avatar(&painter, avatar_rect, &cfg, time, hover_pos);
                }
                ui.add_space(grafito_ui::tokens::SPACE_SM);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(topic_label.clone())
                            .strong()
                            .size(grafito_ui::tokens::TYPE_BASE)
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("Paso {} de {}", current_idx + 1, step_count))
                            .size(grafito_ui::tokens::TYPE_XS)
                            .color(theme.text_secondary.gamma_multiply(0.60)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if action_icon_button(ui, Icon::Close, theme.text_secondary, "Cerrar").clicked()
                    {
                        should_close = true;
                    }
                });
            });
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Barra progreso — hairline 4px, sin animación extra
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), grafito_ui::tokens::SPACE_XS),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(r, 2.0, theme.separator.gamma_multiply(0.10));
            ui.painter().rect_filled(
                egui::Rect::from_min_size(r.min, egui::vec2(r.width() * progress, r.height())),
                2.0,
                theme.accent,
            );
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Contenido principal — dentro de ScrollArea con max_height para no crear "altura enorme al pedo"
            // cuando la explicación es larga o la animación es alta. El footer (controles) queda fijo.
            let max_scroll_h = (ctx.screen_rect().height() * 0.55).clamp(220.0, 420.0);
            egui::ScrollArea::vertical()
                .id_salt("teaching_scroll")
                .max_height(max_scroll_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(step) = current_step.clone() {
                        egui::Frame::none()
                            .fill(theme.panel_bg)
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
                                ui.add_space(grafito_ui::tokens::SPACE_XS);
                                ui.label(
                                    egui::RichText::new(&step.explanation)
                                        .size(grafito_ui::tokens::TYPE_BASE)
                                        .color(theme.text_primary),
                                );
                                if let Some(expr) = &step.math_expr {
                                    ui.add_space(grafito_ui::tokens::SPACE_SM);
                                    egui::Frame::none()
                                        .fill(theme.input_bg)
                                        .stroke(Stroke::new(
                                            1.0,
                                            theme.separator.gamma_multiply(0.10),
                                        ))
                                        .rounding(grafito_ui::tokens::RADIUS_MD)
                                        .inner_margin(egui::Margin::same(
                                            grafito_ui::tokens::SPACE_SM,
                                        ))
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
                                    ui.add_space(grafito_ui::tokens::SPACE_XS);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Pizarra: {}",
                                            step.whiteboard_hint
                                        ))
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_tertiary)
                                        .weak(),
                                    );
                                }
                            });
                        // Pizarra vectorial real — altura responsive clamp 96..160, no fija 120 enorme
                        if !step.whiteboard_hint.is_empty() || !state.whiteboard.doc.is_empty() {
                            ui.add_space(grafito_ui::tokens::SPACE_SM);
                            egui::Frame::none()
                                .fill(theme.input_bg)
                                .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                                .rounding(grafito_ui::tokens::RADIUS_MD)
                                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Pizarra")
                                                .size(grafito_ui::tokens::TYPE_XS)
                                                .color(theme.text_tertiary)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "· {}",
                                                step.whiteboard_hint
                                            ))
                                            .size(grafito_ui::tokens::TYPE_XS)
                                            .color(theme.text_secondary),
                                        );
                                    });
                                    ui.add_space(grafito_ui::tokens::SPACE_XS);
                                    // Altura responsive: 120 ideal pero clamp a 96..160 y a 30% del alto disponible
                                    let wb_h = (grafito_ui::tokens::SPACE_XXL * 3.0)
                                        .clamp(96.0, 160.0)
                                        .min((ui.available_height() * 0.35).max(96.0));
                                    let (wb_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), wb_h),
                                        egui::Sense::click_and_drag(),
                                    );
                                    // Dibujar pizarra vectorial real (trazo, rectángulos, flechas)
                                    state.whiteboard.draw(ui, wb_rect);
                                    // Permitir dibujar encima (pencil) dentro del overlay
                                    state.whiteboard.handle_canvas_input(wb_rect, ui);
                                    if ui.is_rect_visible(wb_rect) {
                                        // Borde sutil por encima del draw para definición
                                        ui.painter().rect_stroke(
                                            wb_rect,
                                            grafito_ui::tokens::RADIUS_MD,
                                            Stroke::new(1.0, theme.separator.gamma_multiply(0.08)),
                                        );
                                        // Hint centrado sobre grilla
                                        ui.painter().text(
                                            wb_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            &step.whiteboard_hint,
                                            egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
                                            theme.text_tertiary.gamma_multiply(0.85),
                                        );
                                    }
                                });
                        }
                        // Animación nativa fallback (si completó)
                        if !anim_textures.is_empty() {
                            ui.add_space(grafito_ui::tokens::SPACE_SM);
                            let time = ui.input(|i| i.time);
                            let idx = ((time * 12.0) as usize) % anim_textures.len();
                            let tex = &anim_textures[idx];
                            let max_w = ui.available_width().max(80.0);
                            // Clampear altura para no generar "altura enorme al pedo" con texturas retrato
                            let max_h = 200.0_f32.min(ui.available_height().max(80.0) * 0.6);
                            let size = tex.size_vec2();
                            let scale_w = (max_w / size.x.max(1.0)).clamp(0.25, 1.0);
                            let scale_h = (max_h / size.y.max(1.0)).clamp(0.25, 1.0);
                            let scale = scale_w.min(scale_h);
                            let display = egui::vec2(size.x * scale, size.y * scale).ceil();
                            let (rect, _) = ui.allocate_exact_size(display, egui::Sense::hover());
                            ui.painter().image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                Color32::WHITE,
                            );
                            ui.painter().rect_stroke(
                                rect,
                                grafito_ui::tokens::RADIUS_MD,
                                Stroke::new(1.0, theme.separator.gamma_multiply(0.10)),
                            );
                            ui.ctx().request_repaint_after(Duration::from_millis(80));
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
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
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
                            ui.horizontal(|ui| {
                                let t = ui.input(|i| i.time);
                                let pulse = ((t * 3.0).sin() + 1.0) * 0.5;
                                let col = theme.accent.gamma_multiply(0.45 + 0.55 * pulse as f32);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(
                                        grafito_ui::tokens::SPACE_SM + 2.0,
                                        grafito_ui::tokens::SPACE_SM + 2.0,
                                    ),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(
                                    rect.center(),
                                    grafito_ui::tokens::SPACE_XS,
                                    col,
                                );
                                ui.label(
                                    egui::RichText::new("Generando animación con Manim…")
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_secondary),
                                );
                            });
                            ui.ctx().request_repaint_after(Duration::from_millis(48));
                        }
                        // Ledger colapsable — no ocupa altura si no se necesita
                        if let Some(ledger) = &ledger {
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
                            egui::CollapsingHeader::new(
                                egui::RichText::new("Detalle de generación")
                                    .size(grafito_ui::tokens::TYPE_XS)
                                    .color(theme.text_tertiary),
                            )
                            .id_salt("teaching_ledger")
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .fill(theme.input_bg)
                                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.08)))
                                    .rounding(grafito_ui::tokens::RADIUS_MD)
                                    .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(ledger)
                                                .monospace()
                                                .size(grafito_ui::tokens::TYPE_XS - 1.0)
                                                .color(theme.text_secondary),
                                        );
                                    });
                            });
                        }
                    }
                });
            ui.add_space(grafito_ui::tokens::SPACE_MD);
            // Controles profesionales — primaria llena ancho, secundaria ghost, iconografía limpia
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = grafito_ui::tokens::SPACE_SM;
                let primary_label = if is_last { "Finalizar" } else { "Siguiente" };
                let primary_icon = if is_last {
                    Icon::Check
                } else {
                    Icon::ChevronRight
                };
                // Botón primario: fill accent, 36h, RADIUS_MD, left-aligned label + right icon
                let btn = egui::Button::new(
                    egui::RichText::new(primary_label)
                        .strong()
                        .size(grafito_ui::tokens::TYPE_SM)
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .rounding(grafito_ui::tokens::RADIUS_MD);
                // Tamaño igual para primaria, ocupa espacio proporcional
                if ui
                    .add_sized(
                        egui::vec2(
                            grafito_ui::tokens::SPACE_XXL * 3.5,
                            grafito_ui::tokens::SPACE_LG * 2.25,
                        ),
                        btn,
                    )
                    .on_hover_text(if is_last {
                        "Cierra la enseñanza"
                    } else {
                        "Avanza al siguiente paso"
                    })
                    .clicked()
                {
                    if is_last {
                        should_close = true;
                    } else {
                        should_advance = true;
                    }
                }
                // Icono decorativo pequeño al lado (no duplica label, solo indica dirección)
                let icon_color = theme.accent;
                let (icon_rect, _) = ui.allocate_exact_size(
                    egui::vec2(grafito_ui::tokens::ICON_SM, grafito_ui::tokens::ICON_SM),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(icon_rect) {
                    grafito_ui::icons::draw_icon(ui.painter(), icon_rect, primary_icon, icon_color);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ghost = egui::Button::new(
                        egui::RichText::new("Cerrar").size(grafito_ui::tokens::TYPE_SM),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.12)))
                    .rounding(grafito_ui::tokens::RADIUS_MD);
                    if ui
                        .add_sized(
                            egui::vec2(
                                grafito_ui::tokens::SPACE_XL * 4.0,
                                grafito_ui::tokens::SPACE_LG * 2.25,
                            ),
                            ghost,
                        )
                        .clicked()
                    {
                        should_close = true;
                    }
                });
            });
        });
    if should_advance {
        // Avance con ctx para forget_image correcto.
        let cached_before = state.cached_len;
        let ok = state.advance();
        // advance limpió sin ctx; ahora olvidar los URIs previos que quedaron huérfanos.
        for idx in 0..cached_before {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        let _ = ok;
    }
    if should_close {
        state.close_with_ctx(ctx);
        true
    } else {
        false
    }
}
