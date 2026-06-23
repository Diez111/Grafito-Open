//! Grafito Toast Notifications — Non-intrusive feedback messages.
//!
//! Los colores se resuelven dinámicamente desde el [`crate::theme::Theme`] activo usando
//! [`current_theme`]. Esto garantiza que los toasts respeten el modo
//! claro/oscuro sin hardcodear valores.

use egui::{Align2, Color32, FontId, Stroke, Vec2};

use crate::theme::current_theme;

#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created: f64,
    pub duration: f64,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
    Cas,
}

impl ToastKind {
    /// Color del borde/acento del toast según el tema activo.
    pub fn color(&self, ctx: &egui::Context) -> Color32 {
        let t = current_theme(ctx);
        match self {
            ToastKind::Info => t.toast_info,
            ToastKind::Success => t.toast_success,
            ToastKind::Error => t.toast_error,
            ToastKind::Cas => t.toast_cas,
        }
    }
}

#[derive(Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    pub fn push(&mut self, msg: impl Into<String>, kind: ToastKind, time: f64) {
        self.toasts.push(Toast {
            message: msg.into(),
            kind,
            created: time,
            duration: 3.5,
        });
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, current_time: f64) {
        self.toasts
            .retain(|t| current_time - t.created < t.duration);
        if self.toasts.is_empty() {
            return;
        }

        let ctx = ui.ctx().clone();
        let theme = current_theme(&ctx);
        let screen_rect = ctx.screen_rect();
        let mut y_offset = 0.0_f32;

        for toast in &self.toasts {
            let elapsed = (current_time - toast.created) as f32;
            let fade_in = (elapsed / 0.2).min(1.0);
            let fade_out = if elapsed > toast.duration as f32 - 0.5 {
                ((toast.duration as f32 - elapsed) / 0.5).max(0.0)
            } else {
                1.0
            };
            let alpha = fade_in * fade_out;
            if alpha <= 0.01 {
                continue;
            }

            let text = &toast.message;
            let font = FontId::proportional(13.0);
            let galley =
                ui.painter()
                    .layout_no_wrap(text.to_string(), font.clone(), theme.toast_text);
            let w = galley.size().x + 24.0;
            let h = 30.0_f32.max(galley.size().y + 12.0);
            y_offset += h + 8.0;

            let pos = screen_rect.max - Vec2::new(12.0 + w, y_offset);
            let painter = ui.painter();
            let rect = egui::Rect::from_min_size(pos, Vec2::new(w, h));
            let kind_color = toast.kind.color(&ctx);
            let bg_alpha = (theme.toast_bg.a() as f32 * alpha) as u8;
            let bg = Color32::from_rgba_premultiplied(
                theme.toast_bg.r(),
                theme.toast_bg.g(),
                theme.toast_bg.b(),
                bg_alpha,
            );
            let border = Color32::from_rgba_premultiplied(
                kind_color.r(),
                kind_color.g(),
                kind_color.b(),
                (100.0 * alpha) as u8,
            );
            painter.rect_filled(rect, egui::Rounding::same(8.0), bg);
            painter.rect_stroke(rect, egui::Rounding::same(8.0), Stroke::new(1.0, border));
            painter.text(
                rect.center() - Vec2::new(0.0, galley.size().y * 0.5 - 3.0),
                Align2::CENTER_CENTER,
                text,
                font,
                Color32::from_rgba_premultiplied(
                    theme.toast_text.r(),
                    theme.toast_text.g(),
                    theme.toast_text.b(),
                    (theme.toast_text.a() as f32 * alpha) as u8,
                ),
            );
        }
    }
}
