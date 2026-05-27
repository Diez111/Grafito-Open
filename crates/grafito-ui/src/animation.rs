//! Grafito Animations — Easing functions and interactive effects.

use egui::{Color32, Pos2, Stroke};

/// Easing functions for smooth transitions.
pub mod easing {
    pub fn linear(t: f32) -> f32 { t }
    pub fn quadratic_in(t: f32) -> f32 { t * t }
    pub fn quadratic_out(t: f32) -> f32 { t * (2.0 - t) }
    pub fn cubic_in(t: f32) -> f32 { t * t * t }
    pub fn cubic_out(t: f32) -> f32 { let t1 = t - 1.0; t1 * t1 * t1 + 1.0 }
    pub fn cubic_in_out(t: f32) -> f32 {
        if t < 0.5 { 4.0 * t * t * t } else { let t1 = t - 1.0; 4.0 * t1 * t1 * t1 + 1.0 }
    }
    pub fn sin_in_out(t: f32) -> f32 { -((std::f32::consts::PI * t).cos() - 1.0) * 0.5 }
    pub fn ease_out_back(t: f32) -> f32 { let c1 = 1.70158; let c3 = c1 + 1.0; 1.0 + c3 * (t - 1.0_f32).powi(3) + c1 * (t - 1.0_f32).powi(2) }
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
        Self { position: pos, start_time: time, duration: 0.45, max_radius: 28.0, color }
    }

    pub fn draw(&self, painter: &egui::Painter, current_time: f64) -> bool {
        let elapsed = current_time - self.start_time;
        if elapsed >= self.duration { return false; }
        let t = (elapsed / self.duration) as f32;
        let radius = self.max_radius * easing::cubic_out(t);
        let alpha = ((1.0 - t) * 160.0) as u8;
        let c = Color32::from_rgba_premultiplied(
            self.color.r(), self.color.g(), self.color.b(), alpha,
        );
        painter.circle_stroke(self.position, radius.max(1.0), Stroke::new(2.5 * (1.0 - t).max(0.2), c));
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
        if self.ripples.len() > 20 { self.ripples.remove(0); }
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
    pub fn new(val: f32) -> Self { Self { current: val, target: val } }

    pub fn set(&mut self, target: f32) { self.target = target; }

    /// Returns the current interpolated value and advances animation.
    pub fn update(&mut self, dt: f32) -> f32 {
        let speed = 8.0; // lerp factor per second
        let t = (speed * dt).min(1.0);
        self.current += (self.target - self.current) * easing::cubic_out(t);
        self.current
    }

    pub fn get(&self) -> f32 { self.current }
}
