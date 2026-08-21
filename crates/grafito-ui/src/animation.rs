//! Grafito Animations — Easing functions and interactive effects.

use crate::theme::{current_theme, Theme};
use egui::{Color32, Pos2, Stroke};
use std::time::Duration;

/// Interpola colores de superficie sin modificar la geometría del control.
pub fn interpolate_color(from: Color32, to: Color32, progress: f32) -> Color32 {
    let progress = progress.clamp(0.0, 1.0);
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * progress).round() as u8
    };

    Color32::from_rgba_premultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

/// Easing functions for smooth transitions.
pub mod easing {
    pub fn linear(t: f32) -> f32 {
        t
    }
    pub fn quadratic_in(t: f32) -> f32 {
        t * t
    }
    pub fn quadratic_out(t: f32) -> f32 {
        t * (2.0 - t)
    }
    pub fn cubic_in(t: f32) -> f32 {
        t * t * t
    }
    pub fn cubic_out(t: f32) -> f32 {
        let t1 = t - 1.0;
        t1 * t1 * t1 + 1.0
    }
    pub fn cubic_in_out(t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            let t1 = t - 1.0;
            4.0 * t1 * t1 * t1 + 1.0
        }
    }
    pub fn sin_in_out(t: f32) -> f32 {
        -((std::f32::consts::PI * t).cos() - 1.0) * 0.5
    }
    pub fn ease_out_back(t: f32) -> f32 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        1.0 + c3 * (t - 1.0_f32).powi(3) + c1 * (t - 1.0_f32).powi(2)
    }
}

/// Canvas click ripple effect.
pub struct Ripple {
    pub position: Pos2,
    pub start_time: f64,
    pub duration: f64,
    pub max_radius: f32,
    pub color: Color32,
}

impl Ripple {
    pub fn new(pos: Pos2, time: f64, color: Color32) -> Self {
        Self {
            position: pos,
            start_time: time,
            duration: 0.45,
            max_radius: 28.0,
            color,
        }
    }

    pub fn draw(&self, painter: &egui::Painter, current_time: f64) -> bool {
        let elapsed = current_time - self.start_time;
        if elapsed >= self.duration {
            return false;
        }
        let t = (elapsed / self.duration) as f32;
        let radius = self.max_radius * easing::cubic_out(t);
        let alpha = ((1.0 - t) * 160.0) as u8;
        let c =
            Color32::from_rgba_premultiplied(self.color.r(), self.color.g(), self.color.b(), alpha);
        painter.circle_stroke(
            self.position,
            radius.max(1.0),
            Stroke::new(2.5 * (1.0 - t).max(0.2), c),
        );
        true
    }
}

/// Manages a set of active ripple effects.
#[derive(Default)]
pub struct RippleManager {
    ripples: Vec<Ripple>,
}

impl RippleManager {
    pub fn add(&mut self, pos: Pos2, time: f64, color: Color32) {
        self.ripples.push(Ripple::new(pos, time, color));
        if self.ripples.len() > 20 {
            self.ripples.remove(0);
        }
    }

    pub fn draw(&mut self, painter: &egui::Painter, current_time: f64) {
        self.ripples.retain_mut(|r| r.draw(painter, current_time));
    }
}

/// Animated value that smoothly interpolates to a target.
pub struct AnimatedValue {
    current: f32,
    target: f32,
}

impl AnimatedValue {
    pub fn new(val: f32) -> Self {
        Self {
            current: val,
            target: val,
        }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Returns the current interpolated value and advances animation.
    pub fn update(&mut self, dt: f32) -> f32 {
        let speed = 8.0; // lerp factor per second
        let t = (speed * dt).min(1.0);
        self.current += (self.target - self.current) * easing::cubic_out(t);
        self.current
    }

    pub fn get(&self) -> f32 {
        self.current
    }
}

/// Estado visual de un proceso local que todavía está en curso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingOrbState {
    /// El asistente está esperando o recibiendo una entrada.
    Listening,
    /// El asistente o un cálculo local está resolviendo el problema.
    Solving,
    /// Se está preparando una respuesta, una gráfica o un resultado.
    Shaping,
    /// Se pidió cancelar y el trabajo cooperativo todavía debe finalizar.
    Cancelling,
}

impl ThinkingOrbState {
    /// Etiqueta que complementa el indicador puramente visual.
    pub fn accessible_label(self) -> &'static str {
        match self {
            Self::Listening => "Escuchando",
            Self::Solving => "Resolviendo",
            Self::Shaping => "Preparando respuesta",
            Self::Cancelling => "Cancelando",
        }
    }
}

/// Indicador nativo y determinista para trabajo local en curso.
///
/// No mantiene estado entre frames: su movimiento depende únicamente del reloj
/// de egui, por lo que no introduce hilos, red ni estado persistido.
#[derive(Debug, Clone, Copy)]
pub struct ThinkingOrb {
    state: ThinkingOrbState,
    size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ThinkingOrbSample {
    x: f32,
    y: f32,
    radius: f32,
    alpha: u8,
}

impl ThinkingOrb {
    /// Crea un orb para el estado y diámetro solicitados.
    pub fn new(state: ThinkingOrbState, size: f32) -> Self {
        Self {
            state,
            size: size.clamp(20.0, 128.0),
        }
    }

    /// Pinta el indicador minimalista macOS: tres puntos con pulso, como Siri.
    pub fn draw(self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(self.size, self.size), egui::Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, self.state.accessible_label())
        });
        let theme = current_theme(ui.ctx());
        let accent = self.state_color(theme);
        let center = rect.center();
        let time = ui.input(|input| input.time as f32);
        let painter = ui.painter_at(rect);
        // Fondo sutil vibrancy
        painter.circle_filled(center, self.size * 0.42, with_alpha(accent, 14));
        // Tres puntos con fase offset, como macOS typing indicator
        let dot_r = (self.size * 0.065).clamp(2.5, 5.0);
        let gap = dot_r * 2.6;
        for i in 0..3 {
            let phase = time * 2.2 + i as f32 * 0.7;
            let pulse = (phase.sin() + 1.0) * 0.5;
            let alpha = 0.35_f32 + 0.65_f32 * pulse;
            let y_off = (pulse * dot_r * 0.6) - dot_r * 0.3;
            let x = (i as f32 - 1.0) * (dot_r * 2.0 + gap * 0.6);
            let col = with_alpha(accent, (alpha * 220.0) as u8);
            painter.circle_filled(center + egui::vec2(x, y_off), dot_r, col);
        }
        // macOS: no orbit, solo puntos con pulso; resto eliminado para minimalismo
        if false {
            for sample in self.samples_at(time) {
                let sample_center = center + egui::vec2(sample.x * 0.0, sample.y * 0.0);
                painter.circle_filled(sample_center, 1.0, with_alpha(accent, 0));
            }
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        response
    }

    fn samples_at(self, time: f32) -> [ThinkingOrbSample; 3] {
        let base_phase = self.base_phase(time);
        std::array::from_fn(|index| {
            let phase = base_phase + index as f32 * std::f32::consts::TAU / 3.0;
            let (x, y) = self.position_at(phase, index);
            let pulse = (phase.sin() + 1.0) * 0.5;
            ThinkingOrbSample {
                x,
                y,
                radius: 0.075 + pulse * 0.04,
                alpha: (150.0 + pulse * 90.0) as u8,
            }
        })
    }

    fn base_phase(self, time: f32) -> f32 {
        let time = if time.is_finite() { time } else { 0.0 };
        let speed = match self.state {
            ThinkingOrbState::Listening => 1.15,
            ThinkingOrbState::Solving => 2.1,
            ThinkingOrbState::Shaping => 1.65,
            ThinkingOrbState::Cancelling => -0.9,
        };
        time * speed
    }

    fn position_at(self, phase: f32, satellite: usize) -> (f32, f32) {
        let offset = satellite as f32 * 0.31;
        match self.state {
            ThinkingOrbState::Listening => (
                phase.cos() * 0.56,
                phase.sin() * 0.30 + (phase * 2.0 + offset).sin() * 0.08,
            ),
            ThinkingOrbState::Solving => (
                (phase * 1.2).cos() * 0.58,
                (phase * 2.0 + offset).sin() * 0.30,
            ),
            ThinkingOrbState::Shaping => (
                (phase * 1.5 + offset).sin() * 0.58,
                (phase * 2.5).sin() * 0.42,
            ),
            ThinkingOrbState::Cancelling => {
                (phase.cos() * 0.42, (phase * 1.5 + offset).sin() * 0.24)
            }
        }
    }

    fn state_color(self, theme: &Theme) -> Color32 {
        match self.state {
            ThinkingOrbState::Listening => theme.accent_strong,
            ThinkingOrbState::Solving => theme.accent,
            ThinkingOrbState::Shaping => theme.success,
            ThinkingOrbState::Cancelling => theme.warning,
        }
    }
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod tests {
    use super::{interpolate_color, ThinkingOrb, ThinkingOrbState};
    use egui::Color32;

    #[test]
    fn color_interpolation_clamps_and_preserves_endpoints() {
        let from = Color32::from_rgba_unmultiplied(10, 20, 30, 40);
        let to = Color32::from_rgba_unmultiplied(110, 120, 130, 140);

        assert_eq!(interpolate_color(from, to, -1.0), from);
        assert_eq!(interpolate_color(from, to, 2.0), to);
        assert_eq!(
            interpolate_color(
                Color32::from_rgb(10, 20, 30),
                Color32::from_rgb(110, 120, 130),
                0.5
            ),
            Color32::from_rgb(60, 70, 80)
        );
    }

    #[test]
    fn thinking_orb_samples_are_finite_bounded_and_deterministic() {
        let orb = ThinkingOrb::new(ThinkingOrbState::Shaping, 32.0);
        let first = orb.samples_at(0.75);
        let second = orb.samples_at(0.75);

        assert_eq!(first, second);
        for sample in first {
            assert!(sample.x.is_finite());
            assert!(sample.y.is_finite());
            assert!((-1.0..=1.0).contains(&sample.x));
            assert!((-1.0..=1.0).contains(&sample.y));
            assert!(sample.radius > 0.0);
            assert!(sample.alpha > 0);
        }
    }

    #[test]
    fn thinking_orb_states_have_accessible_labels() {
        assert_eq!(ThinkingOrbState::Listening.accessible_label(), "Escuchando");
        assert_eq!(ThinkingOrbState::Solving.accessible_label(), "Resolviendo");
        assert_eq!(
            ThinkingOrbState::Shaping.accessible_label(),
            "Preparando respuesta"
        );
        assert_eq!(
            ThinkingOrbState::Cancelling.accessible_label(),
            "Cancelando"
        );
    }

    #[test]
    fn thinking_orb_renders_for_every_state_in_light_and_dark_themes() {
        for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
            let context = egui::Context::default();
            context.set_visuals(visuals);
            for state in [
                ThinkingOrbState::Listening,
                ThinkingOrbState::Solving,
                ThinkingOrbState::Shaping,
                ThinkingOrbState::Cancelling,
            ] {
                let _ = context.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ThinkingOrb::new(state, 64.0).draw(ui);
                    });
                });
            }
        }
    }
}
