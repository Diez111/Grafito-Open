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
///
/// Vocabulario compartido con el wire (`grafito_anim::protocol::EASING_NAMES`)
/// y con `RateFunc` (`grafito_anim::scene`, W2): la Piel resuelve el nombre a
/// la fn existente vía [`easing::by_name`] (desconocido → `linear` honesto).
/// Correspondencia documentada en W2 (`RateFunc::from_name`/`legacy_name`):
/// `linear`→`Linear` (exacto), `cubic_in`/`cubic_out`/`cubic_in_out`→
/// `EaseInOut` (exacto el in_out), `sin_in_out`→`Smooth` (aproximado),
/// `quadratic_in`/`quadratic_out`→`RushInOut` (aproximado),
/// `ease_out_back`→`Wiggle` (aproximado, ambos no-monotónicos).
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

    /// Resuelve un easing por nombre del wire (`EASING_NAMES`): los 8
    /// canónicos van a la fn existente; desconocido o vacío → `linear`
    /// honesto (jamás inventa curva). Puro, sin pánicos.
    pub fn by_name(name: &str) -> fn(f32) -> f32 {
        match name.trim().to_lowercase().as_str() {
            "linear" => linear,
            "quadratic_in" => quadratic_in,
            "quadratic_out" => quadratic_out,
            "cubic_in" => cubic_in,
            "cubic_out" => cubic_out,
            "cubic_in_out" => cubic_in_out,
            "sin_in_out" => sin_in_out,
            "ease_out_back" => ease_out_back,
            _ => linear,
        }
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

/// Intervalo de repintado del orb de pensamiento (F17).
///
/// Este widget vive en `grafito-ui` (capa Piel) y no puede alcanzar
/// `GrafitoApp::request_repaint_budget` (DAG: `ui → app`). Mientras el
/// asistente está pendiente, el scheduler unificado de `app.rs` ya repinta a
/// 16ms (`assistant.is_pending`), subsumiendo este pedido; se mantiene como
/// fallback para uso standalone del widget.
pub const THINKING_ORB_REPAINT_INTERVAL: Duration = Duration::from_millis(50);

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
        // F17: subsumido por el scheduler de app.rs (16ms) cuando is_pending.
        ui.ctx()
            .request_repaint_after(THINKING_ORB_REPAINT_INTERVAL);
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

/// Ejes y viewport compartidos para previews de animación (Piel pura).
///
/// `fn render(&Estado) -> Frame`: todo acá es puro y sin E/S; el raster
/// (`grafito-app/src/anim_native.rs`) lo usa para no duplicar la mate.
/// Cero I/O, cero spawn.
pub mod anim_axes {
    use crate::tokens::SPACE_XS;

    /// Ancho estimado por glifo del rótulo quemado a escala 1 (avance
    /// `ab_glyph` ≈ 6px + tracking): caja conservadora para el anti-solape.
    pub const TICK_CHAR_W_PX: f32 = 7.0;
    /// Alto estimado del rótulo a escala 1 (ascenso ≈ 10px + 1 de aire).
    pub const TICK_CHAR_H_PX: f32 = 11.0;

    /// Etiqueta corta de tick: 1 decimal máximo, sin ceros colgando
    /// (`1.50` → `1.5`, `2.0` → `2`). No-finito → `"?"` honesto.
    /// Pura, sin pánicos.
    pub fn short_tick_label(value: f64) -> String {
        if !value.is_finite() {
            return "?".to_string();
        }
        if value == 0.0 {
            return "0".to_string();
        }
        let redondeado = (value * 10.0).round() / 10.0;
        if redondeado == 0.0 {
            return "0".to_string();
        }
        if redondeado.fract() == 0.0 && redondeado.abs() < 1e15 {
            return format!("{:.0}", redondeado);
        }
        format!("{redondeado:.1}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }

    /// Caja ocupada por un rótulo (píxeles de frame). El padding deriva de
    /// `SPACE_XS` (tokens, no mágico).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LabelCaja {
        pub x: usize,
        pub y: usize,
        pub w: usize,
        pub h: usize,
    }

    impl LabelCaja {
        /// ¿Se tocan (con 2px de aire por lado = `SPACE_XS / 2`)?
        pub fn solapa(self, otra: Self) -> bool {
            let pad = SPACE_XS as usize / 2;
            let (ax0, ay0) = (self.x.saturating_sub(pad), self.y.saturating_sub(pad));
            let (ax1, ay1) = (
                self.x.saturating_add(self.w).saturating_add(pad),
                self.y.saturating_add(self.h).saturating_add(pad),
            );
            let (bx0, by0) = (otra.x.saturating_sub(pad), otra.y.saturating_sub(pad));
            let (bx1, by1) = (
                otra.x.saturating_add(otra.w).saturating_add(pad),
                otra.y.saturating_add(otra.h).saturating_add(pad),
            );
            ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1
        }
    }

    /// ¿Cabe `(x, y, tw, th)` sin tocar ninguna ocupada? No muta.
    pub fn cabe_label_entre(
        x: usize,
        y: usize,
        tw: usize,
        th: usize,
        ocupadas: &[LabelCaja],
    ) -> bool {
        let caja = LabelCaja { x, y, w: tw, h: th };
        !ocupadas.iter().any(|o| caja.solapa(*o))
    }

    /// Reserva `(x, y, tw, th)` si cabe: `true` = dibujar y quedó ocupada,
    /// `false` = no dibujar (colisiona). Puro sobre el registro.
    pub fn reserva_label(
        x: usize,
        y: usize,
        tw: usize,
        th: usize,
        ocupadas: &mut Vec<LabelCaja>,
    ) -> bool {
        if cabe_label_entre(x, y, tw, th, ocupadas) {
            ocupadas.push(LabelCaja { x, y, w: tw, h: th });
            true
        } else {
            false
        }
    }

    /// Recorta un segmento en float-píxeles a la caja `[0,w-1]×[0,h-1]`
    /// (Cohen–Sutherland, como máximo 4 iteraciones acotadas).
    /// `None` = fuera de vista o degenerado no-finito: el llamador NO dibuja
    /// (clip limpio, jamás plateau). Puro, sin pánicos.
    #[allow(clippy::too_many_arguments)]
    pub fn clip_seg_a_caja(
        ax: f32,
        ay: f32,
        bx: f32,
        by: f32,
        w: usize,
        h: usize,
    ) -> Option<((usize, usize), (usize, usize))> {
        if w == 0 || h == 0 {
            return None;
        }
        if !(ax.is_finite() && ay.is_finite() && bx.is_finite() && by.is_finite()) {
            return None;
        }
        let (x0, y0) = (0.0f32, 0.0f32);
        let (x1, y1) = ((w - 1) as f32, (h - 1) as f32);
        const IZQ: u8 = 1;
        const DER: u8 = 2;
        const ARRIBA: u8 = 4;
        const ABAJO: u8 = 8;
        fn codigo(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> u8 {
            let mut c = 0u8;
            if x < x0 {
                c |= IZQ;
            } else if x > x1 {
                c |= DER;
            }
            if y < y0 {
                c |= ARRIBA;
            } else if y > y1 {
                c |= ABAJO;
            }
            c
        }
        let (mut ax, mut ay, mut bx, mut by) = (ax, ay, bx, by);
        for _ in 0..4 {
            let (ca, cb) = (
                codigo(ax, ay, x0, y0, x1, y1),
                codigo(bx, by, x0, y0, x1, y1),
            );
            if ca == 0 && cb == 0 {
                let redondea = |v: f32, max: i32| (v.round() as i32).clamp(0, max) as usize;
                return Some((
                    (redondea(ax, x1 as i32), redondea(ay, y1 as i32)),
                    (redondea(bx, x1 as i32), redondea(by, y1 as i32)),
                ));
            }
            if ca & cb != 0 {
                return None;
            }
            let cc = if ca != 0 { ca } else { cb };
            let (dx, dy) = (bx - ax, by - ay);
            if !dx.is_finite() || !dy.is_finite() {
                return None;
            }
            let (nx, ny) = if cc & (IZQ | DER) != 0 {
                if dx == 0.0 {
                    return None;
                }
                let x = if cc & IZQ != 0 { x0 } else { x1 };
                let t = (x - ax) / dx;
                if !t.is_finite() {
                    return None;
                }
                (x, ay + t * dy)
            } else if cc & ARRIBA != 0 {
                if dy == 0.0 {
                    return None;
                }
                let t = (y0 - ay) / dy;
                if !t.is_finite() {
                    return None;
                }
                (ax + t * dx, y0)
            } else {
                if dy == 0.0 {
                    return None;
                }
                let t = (y1 - ay) / dy;
                if !t.is_finite() {
                    return None;
                }
                (ax + t * dx, y1)
            };
            if !nx.is_finite() || !ny.is_finite() {
                return None;
            }
            if ca != 0 {
                (ax, ay) = (nx, ny);
            } else {
                (bx, by) = (nx, ny);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{anim_axes, easing, interpolate_color, ThinkingOrb, ThinkingOrbState};
    use egui::Color32;

    // ── anim_axes: rótulos cortos + clip limpio + anti-solape ──────────
    #[test]
    fn tick_corto_un_decimal_maximo() {
        assert_eq!(anim_axes::short_tick_label(2.0), "2");
        assert_eq!(anim_axes::short_tick_label(-1.0), "-1");
        assert_eq!(anim_axes::short_tick_label(1.5), "1.5");
        assert_eq!(anim_axes::short_tick_label(0.0), "0");
        assert_eq!(anim_axes::short_tick_label(-0.04), "0");
        assert_eq!(anim_axes::short_tick_label(f64::NAN), "?");
        assert_eq!(anim_axes::short_tick_label(f64::INFINITY), "?");
        // 1 decimal máximo: 1/3 no sale como "0.33".
        assert_eq!(anim_axes::short_tick_label(1.0 / 3.0), "0.3");
    }

    #[test]
    fn clip_recorta_sin_aplastar() {
        // Dentro intacto.
        assert!(anim_axes::clip_seg_a_caja(10.0, 10.0, 50.0, 50.0, 320, 240).is_some());
        // Totalmente fuera (arriba, donde la parábola se aplastaba) → None.
        assert!(anim_axes::clip_seg_a_caja(10.0, -80.0, 50.0, -40.0, 320, 240).is_none());
        // Cruza el borde: sale clipado dentro de la caja, sin plateau.
        let ((ax, _), (bx, _)) = anim_axes::clip_seg_a_caja(160.0, 120.0, 160.0, -60.0, 320, 240)
            .expect("cruce vertical debe clipar");
        assert_eq!((ax, bx), (160, 160));
        // Degenerados honestos.
        assert!(anim_axes::clip_seg_a_caja(f32::NAN, 0.0, 1.0, 1.0, 320, 240).is_none());
        assert!(anim_axes::clip_seg_a_caja(0.0, 0.0, 1.0, 1.0, 0, 240).is_none());
    }

    #[test]
    fn reserva_rechaza_cajas_que_solapan() {
        let mut ocupadas = Vec::new();
        assert!(anim_axes::reserva_label(100, 100, 30, 11, &mut ocupadas));
        // Encima de la anterior ("0000" amontonados) → se rechaza.
        assert!(!anim_axes::reserva_label(105, 102, 30, 11, &mut ocupadas));
        // Lejos → se acepta.
        assert!(anim_axes::reserva_label(200, 100, 30, 11, &mut ocupadas));
        assert_eq!(ocupadas.len(), 2);
    }

    // ── F1: unificación con RateFunc (W2) ─────────────────────────────
    #[test]
    fn easing_by_name_cubre_wire_y_falla_a_linear() {
        for name in grafito_anim::protocol::EASING_NAMES {
            let f = easing::by_name(name);
            for t in [0.0f32, 0.5, 1.0] {
                assert!(f(t).is_finite(), "{name}({t}) finito");
            }
            // Todos arrancan en 0 y cierran en 1 (ease_out_back sobrepasa
            // en el medio pero cierra exacto).
            assert!(f(0.0).abs() < 1e-6, "{name}(0)==0");
            assert!((f(1.0) - 1.0).abs() < 1e-6, "{name}(1)==1");
        }
        // Desconocido o vacío → linear honesto (no inventa curva).
        assert_eq!(easing::by_name("no-existe")(0.5), easing::linear(0.5));
        assert_eq!(easing::by_name("")(0.5), easing::linear(0.5));
        assert_eq!(
            easing::by_name("  CUBIC_IN_OUT  ")(0.25),
            easing::cubic_in_out(0.25)
        );
    }

    #[test]
    fn easing_paridad_con_ratefunc_w2() {
        use grafito_anim::scene::RateFunc;
        // Exactos (misma fórmula en f32 vs f64 casteado: tolerancia ulp).
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            assert!(
                (easing::linear(t) - RateFunc::Linear.apply_f32(t)).abs() <= 1e-6,
                "linear exacto en t={t}"
            );
            assert!(
                (easing::cubic_in_out(t) - RateFunc::EaseInOut.apply_f32(t)).abs() <= 1e-6,
                "cubic_in_out exacto en t={t}"
            );
        }
        // Aproximado documentado en W2: sin_in_out vs Smooth (smoothstep).
        // Endpoints y medio coinciden; el interior difiere apenas.
        let mut max = 0.0f32;
        for i in 0..=200 {
            let t = i as f32 / 200.0;
            max = max.max((easing::sin_in_out(t) - RateFunc::Smooth.apply_f32(t)).abs());
        }
        assert!(max < 0.025, "sin_in_out≈Smooth, max diff={max}");
        // Los 3 exactos de `legacy_name` resuelven por nombre del wire.
        for (rate, legacy) in [
            (RateFunc::Linear, "linear"),
            (RateFunc::Smooth, "sin_in_out"),
            (RateFunc::EaseInOut, "cubic_in_out"),
        ] {
            assert_eq!(rate.legacy_name(), Some(legacy));
            let _ = easing::by_name(legacy);
        }
    }

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
