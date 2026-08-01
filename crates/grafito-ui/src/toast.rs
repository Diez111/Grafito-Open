//! Grafito Toast Notifications — Non-intrusive feedback messages.
//!
//! Los colores se resuelven dinámicamente desde el [`crate::theme::Theme`] activo usando
//! [`current_theme`]. Esto garantiza que los toasts respeten el modo
//! claro/oscuro sin hardcodear valores.

use egui::{Color32, FontId, Stroke, Vec2};

use crate::theme::current_theme;

const TOAST_SCREEN_MARGIN: f32 = 12.0;
const TOAST_HORIZONTAL_PADDING: f32 = 12.0;
const TOAST_STACK_GAP: f32 = 8.0;
const TOAST_TOP_OFFSET: f32 = 56.0;
const TOAST_MAX_SCREEN_FRACTION: f32 = 0.25;

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
        let mut y_offset = TOAST_TOP_OFFSET;
        // Leave the lower three quarters free for compact panels and their composer.
        let max_stack_y = (screen_rect.height() * TOAST_MAX_SCREEN_FRACTION)
            .max(TOAST_TOP_OFFSET + TOAST_SCREEN_MARGIN);

        for (index, toast) in self.toasts.iter().enumerate().rev() {
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
            let painter = ui.painter();
            let galley = painter.layout(
                text.to_string(),
                font,
                theme.toast_text,
                toast_text_width_limit(screen_rect.width()),
            );
            let w = (galley.size().x + TOAST_HORIZONTAL_PADDING * 2.0)
                .min((screen_rect.width() - TOAST_SCREEN_MARGIN * 2.0).max(1.0));
            let h = 30.0_f32.max(galley.size().y + 12.0);
            if y_offset + h + TOAST_SCREEN_MARGIN > max_stack_y {
                break;
            }

            let pos = egui::pos2(
                screen_rect.min.x + TOAST_SCREEN_MARGIN,
                screen_rect.min.y + y_offset,
            );
            let rect = egui::Rect::from_min_size(pos, Vec2::new(w, h));
            y_offset += h + TOAST_STACK_GAP;
            let response = ui.interact(rect, ui.id().with(("toast", index)), egui::Sense::hover());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
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
            painter.galley_with_override_text_color(
                rect.min
                    + Vec2::new(
                        TOAST_HORIZONTAL_PADDING,
                        (h - galley.size().y).max(0.0) * 0.5,
                    ),
                galley,
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

fn toast_text_width_limit(screen_width: f32) -> f32 {
    (screen_width - (TOAST_SCREEN_MARGIN + TOAST_HORIZONTAL_PADDING) * 2.0).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::{toast_text_width_limit, ToastKind, ToastManager};

    #[test]
    fn toast_text_width_preserves_screen_margins() {
        let screen_width = 100.0;
        let limit = toast_text_width_limit(screen_width);

        assert!(limit > 0.0);
        assert!(limit + 48.0 <= screen_width);
    }

    #[test]
    fn long_toasts_stay_inside_a_narrow_viewport() {
        let context = egui::Context::default();
        let mut toasts = ToastManager::default();
        toasts.push("x".repeat(100), ToastKind::Error, 0.0);
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(100.0, 1_200.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| toasts.draw(ui, 0.1));
            },
        );
        let rectangles = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) => Some(shape.rect),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(!rectangles.is_empty());
        assert!(rectangles.iter().all(|rect| {
            rect.min.x >= 0.0 && rect.max.x <= 100.0 && rect.min.y >= 0.0 && rect.max.y <= 1_200.0
        }));
        assert!(rectangles
            .iter()
            .filter(|rect| rect.width() < 100.0)
            .all(|rect| rect.min.y >= 56.0 && rect.max.y <= 300.0));
    }
}
