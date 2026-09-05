//! Grafito Toast Notifications — Non-intrusive feedback messages.
//!
//! Los colores se resuelven dinámicamente desde el [`crate::theme::Theme`] activo usando
//! [`current_theme`]. Esto garantiza que los toasts respeten el modo
//! claro/oscuro sin hardcodear valores.
//!
//! Accesibilidad (WCAG 2.2.1 Timing Adjustable):
//! - Duraciones desde tokens (`TOAST_DURATION_DEFAULT` 7 s, error persistente).
//! - Hover pausa el temporizador; clic descarta el toast.

use egui::{Color32, FontId, Stroke, Vec2};

use crate::theme::current_theme;
use crate::tokens::{
    RADIUS_SM, SPACE_MD, SPACE_SM, TOAST_DURATION_DEFAULT, TOAST_DURATION_ERROR, TOAST_FADE_IN,
    TOAST_FADE_OUT, TOAST_MAX_SCREEN_FRACTION, TOAST_MIN_HEIGHT, TOAST_TOP_OFFSET, TYPE_SM,
};

#[derive(Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created: f64,
    pub duration: f64,
}

impl Toast {
    /// Error persistente (sin auto-dismiss): sólo sale con clic.
    pub fn is_persistent(&self) -> bool {
        self.duration.is_infinite()
    }
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

    /// Duración por severidad: 7 s por defecto, error persistente hasta dismiss.
    pub fn default_duration(&self) -> f64 {
        match self {
            ToastKind::Error => TOAST_DURATION_ERROR,
            ToastKind::Info | ToastKind::Success | ToastKind::Cas => TOAST_DURATION_DEFAULT,
        }
    }
}

#[derive(Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
    /// Último `current_time` visto: permite pausar el temporizador en hover.
    last_time: Option<f64>,
}

impl ToastManager {
    pub fn push(&mut self, msg: impl Into<String>, kind: ToastKind, time: f64) {
        let duration = kind.default_duration();
        self.toasts.push(Toast {
            message: msg.into(),
            kind,
            created: time,
            duration,
        });
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, current_time: f64) {
        self.toasts
            .retain(|t| current_time - t.created < t.duration);
        if self.toasts.is_empty() {
            self.last_time = Some(current_time);
            return;
        }

        // Pausa en hover (WCAG 2.2.1): congela el temporizador avanzando
        // `created` por el dt transcurrido, sin pasar nunca `current_time`.
        let dt = match self.last_time {
            Some(prev) => (current_time - prev).max(0.0),
            None => 0.0,
        };

        let ctx = ui.ctx().clone();
        let theme = current_theme(&ctx);
        let screen_rect = ctx.screen_rect();
        let mut y_offset = TOAST_TOP_OFFSET;
        // Leave the lower three quarters free for compact panels and their composer.
        let max_stack_y =
            (screen_rect.height() * TOAST_MAX_SCREEN_FRACTION).max(TOAST_TOP_OFFSET + SPACE_MD);

        let mut hovered: Vec<usize> = Vec::new();
        let mut clicked: Vec<usize> = Vec::new();

        for (index, toast) in self.toasts.iter().enumerate().rev() {
            let elapsed = (current_time - toast.created) as f32;
            let fade_in = (elapsed / TOAST_FADE_IN).min(1.0);
            let fade_out = if elapsed > toast.duration as f32 - TOAST_FADE_OUT {
                ((toast.duration as f32 - elapsed) / TOAST_FADE_OUT).max(0.0)
            } else {
                1.0
            };
            let alpha = fade_in * fade_out;
            if alpha <= 0.01 {
                continue;
            }

            let text = &toast.message;
            let font = FontId::proportional(TYPE_SM);
            let painter = ui.painter();
            let galley = painter.layout(
                text.to_string(),
                font,
                theme.toast_text,
                toast_text_width_limit(screen_rect.width()),
            );
            let w = (galley.size().x + SPACE_MD * 2.0)
                .min((screen_rect.width() - SPACE_MD * 2.0).max(1.0));
            let h = TOAST_MIN_HEIGHT.max(galley.size().y + SPACE_MD);
            if y_offset + h + SPACE_MD > max_stack_y {
                break;
            }

            let pos = egui::pos2(screen_rect.min.x + SPACE_MD, screen_rect.min.y + y_offset);
            let rect = egui::Rect::from_min_size(pos, Vec2::new(w, h));
            y_offset += h + SPACE_SM;
            let response = ui.interact(rect, ui.id().with(("toast", index)), egui::Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
            if response.hovered() {
                hovered.push(index);
            }
            if response.clicked() {
                clicked.push(index);
            }
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
            painter.rect_filled(rect, egui::Rounding::same(RADIUS_SM), bg);
            painter.rect_stroke(
                rect,
                egui::Rounding::same(RADIUS_SM),
                Stroke::new(1.0, border),
            );
            painter.galley_with_override_text_color(
                rect.min + Vec2::new(SPACE_MD, (h - galley.size().y).max(0.0) * 0.5),
                galley,
                Color32::from_rgba_premultiplied(
                    theme.toast_text.r(),
                    theme.toast_text.g(),
                    theme.toast_text.b(),
                    (theme.toast_text.a() as f32 * alpha) as u8,
                ),
            );
        }

        for index in hovered {
            if let Some(toast) = self.toasts.get_mut(index) {
                toast.created = (toast.created + dt).min(current_time);
            }
        }
        // `clicked` llega en orden descendente (loop `.rev()`): `remove` es seguro.
        for index in clicked {
            if index < self.toasts.len() {
                self.toasts.remove(index);
            }
        }
        self.last_time = Some(current_time);
    }
}

fn toast_text_width_limit(screen_width: f32) -> f32 {
    (screen_width - (SPACE_MD + SPACE_MD) * 2.0).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        toast_text_width_limit, ToastKind, ToastManager, TOAST_DURATION_DEFAULT,
        TOAST_DURATION_ERROR,
    };

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

    #[test]
    fn toast_durations_come_from_tokens() {
        assert_eq!(ToastKind::Info.default_duration(), TOAST_DURATION_DEFAULT);
        assert_eq!(
            ToastKind::Success.default_duration(),
            TOAST_DURATION_DEFAULT
        );
        assert_eq!(ToastKind::Cas.default_duration(), TOAST_DURATION_DEFAULT);
        assert_eq!(ToastKind::Error.default_duration(), TOAST_DURATION_ERROR);
        assert!(ToastKind::Error.default_duration().is_infinite());
    }

    #[test]
    fn push_assigns_duration_by_kind() {
        let mut manager = ToastManager::default();
        manager.push("info", ToastKind::Info, 0.0);
        manager.push("boom", ToastKind::Error, 0.0);

        assert_eq!(manager.toasts.len(), 2);
        assert_eq!(manager.toasts[0].duration, TOAST_DURATION_DEFAULT);
        assert!(!manager.toasts[0].is_persistent());
        assert!(manager.toasts[1].is_persistent());
    }

    fn toast_rect_count(toasts: &mut ToastManager, time: f64) -> usize {
        let context = egui::Context::default();
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| toasts.draw(ui, time));
        });
        output
            .shapes
            .iter()
            .filter(|clipped| matches!(clipped.shape, egui::Shape::Rect(_)))
            .count()
    }

    fn empty_rect_count(time: f64) -> usize {
        toast_rect_count(&mut ToastManager::default(), time)
    }

    #[test]
    fn default_toast_expires_after_token_duration() {
        let mut manager = ToastManager::default();
        manager.push("hola", ToastKind::Info, 0.0);

        assert!(toast_rect_count(&mut manager, 6.9) > empty_rect_count(6.9));
        assert_eq!(toast_rect_count(&mut manager, 7.1), empty_rect_count(7.1));
    }

    #[test]
    fn error_toast_persists_until_click() {
        let mut manager = ToastManager::default();
        manager.push("boom", ToastKind::Error, 0.0);

        assert!(toast_rect_count(&mut manager, 3_600.0) > empty_rect_count(3_600.0));
    }

    fn draw_with_pointer(
        ctx: &egui::Context,
        toasts: &mut ToastManager,
        time: f64,
        events: Vec<egui::Event>,
    ) -> usize {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 1_200.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| toasts.draw(ui, time));
            },
        );
        output
            .shapes
            .iter()
            .filter(|clipped| matches!(clipped.shape, egui::Shape::Rect(_)))
            .count()
    }

    #[test]
    fn hover_pauses_toast_expiry() {
        // Toast en (12, 56): hover en (20, 65), lejos en (350, 1100).
        let ctx = egui::Context::default();
        let hover = egui::pos2(20.0, 65.0);
        let away = egui::pos2(350.0, 1_100.0);
        let moved = |pos| vec![egui::Event::PointerMoved(pos)];
        let mut manager = ToastManager::default();
        manager.push("pausa", ToastKind::Info, 0.0);

        // Priming: egui resuelve hover con los rects del pass previo, así que
        // el primer frame sólo registra el widget (hovered=false).
        draw_with_pointer(&ctx, &mut manager, 0.3, moved(hover));
        // 5.7 s bajo el cursor: el temporizador se congela en created = 5.7.
        draw_with_pointer(&ctx, &mut manager, 6.0, moved(hover));
        // A t=12 sin hover han pasado 6.3 s efectivos < 7 s: sigue visible.
        assert!(draw_with_pointer(&ctx, &mut manager, 12.0, moved(away)) > empty_rect_count(12.0));
        // A t=13.5 sin hover: 7.8 s efectivos ≥ 7 s: expiró.
        assert_eq!(
            draw_with_pointer(&ctx, &mut manager, 13.5, moved(away)),
            empty_rect_count(13.5)
        );
    }

    #[test]
    fn click_dismisses_toast() {
        let ctx = egui::Context::default();
        let at_toast = egui::pos2(20.0, 65.0);
        let mut manager = ToastManager::default();
        let baseline = draw_with_pointer(&ctx, &mut ToastManager::default(), 0.0, Vec::new());
        manager.push("fuera", ToastKind::Error, 0.0);

        // Priming: el primer frame sólo registra el widget (hovered=false).
        draw_with_pointer(
            &ctx,
            &mut manager,
            0.3,
            vec![egui::Event::PointerMoved(at_toast)],
        );
        draw_with_pointer(
            &ctx,
            &mut manager,
            0.4,
            vec![egui::Event::PointerButton {
                pos: at_toast,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
        );
        draw_with_pointer(
            &ctx,
            &mut manager,
            0.5,
            vec![egui::Event::PointerButton {
                pos: at_toast,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        // Tras press+release sobre el toast, el error persistente desapareció.
        assert_eq!(
            draw_with_pointer(&ctx, &mut manager, 0.6, Vec::new()),
            baseline
        );
    }
}
