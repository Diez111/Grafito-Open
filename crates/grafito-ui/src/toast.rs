//! Grafito Toast Notifications — Non-intrusive feedback messages.

use egui::{Align2, Color32, FontId, Stroke, Vec2};

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
    pub fn color(&self) -> Color32 {
        match self {
            ToastKind::Info => Color32::from_rgb(80, 150, 255),
            ToastKind::Success => Color32::from_rgb(80, 210, 80),
            ToastKind::Error => Color32::from_rgb(255, 80, 80),
            ToastKind::Cas => Color32::from_rgb(160, 100, 255),
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
        self.toasts.retain(|t| current_time - t.created < t.duration);
        if self.toasts.is_empty() { return; }

        let screen_rect = ui.ctx().screen_rect();
        let mut y_offset = 0.0_f32;

        for toast in &self.toasts {
            let elapsed = (current_time - toast.created) as f32;
            let fade_in = (elapsed / 0.2).min(1.0);
            let fade_out = if elapsed > toast.duration as f32 - 0.5 {
                ((toast.duration as f32 - elapsed) / 0.5).max(0.0)
            } else { 1.0 };
            let alpha = fade_in * fade_out;
            if alpha <= 0.01 { continue; }

            let text = &toast.message;
            let font = FontId::proportional(13.0);
            let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
            let w = galley.size().x + 24.0;
            let h = 30.0_f32.max(galley.size().y + 12.0);
            y_offset += h + 8.0;

            let pos = screen_rect.max - Vec2::new(12.0 + w, y_offset);
            let painter = ui.painter();
            let rect = egui::Rect::from_min_size(pos, Vec2::new(w, h));
            let bg = Color32::from_rgba_premultiplied(30, 33, 44, (220.0 * alpha) as u8);
            let border = Color32::from_rgba_premultiplied(
                toast.kind.color().r(), toast.kind.color().g(), toast.kind.color().b(), (100.0 * alpha) as u8,
            );
            painter.rect_filled(rect, egui::Rounding::same(8.0), bg);
            painter.rect_stroke(rect, egui::Rounding::same(8.0), Stroke::new(1.0, border));
            painter.text(rect.center() - Vec2::new(0.0, galley.size().y * 0.5 - 3.0),
                Align2::CENTER_CENTER, text, font,
                Color32::from_rgba_premultiplied(255, 255, 255, (255.0 * alpha) as u8));
        }
    }
}
