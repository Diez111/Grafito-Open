//! Avatar Scandinavian profesional — super configurable, eye-tracking y morph a burbuja.
//!
//! Reemplaza el habitáculo Pou por un avatar minimalista con 12 dimensiones
//! personalizables. Todo el dibujo es vectorial `egui::Painter` sin assets,
//! con eye-tracking determinista y testes headless para la lógica pura.

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use grafito_profile::{
    AvatarAccessory, AvatarBlush, AvatarConfig, AvatarEyeStyle, AvatarMouthStyle, AvatarShape,
};

// ── Helpers de color ───────────────────────────────────────────────────────
fn accent_color(config: &AvatarConfig) -> Color32 {
    let rgb = config.accent_color();
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}
#[allow(dead_code)]
fn accent_muted(config: &AvatarConfig) -> Color32 {
    let c = accent_color(config);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 32)
}
fn accent_strong(config: &AvatarConfig) -> Color32 {
    let c = accent_color(config);
    // strong = slightly darker
    Color32::from_rgb(
        (c.r() as f32 * 0.85) as u8,
        (c.g() as f32 * 0.85) as u8,
        (c.b() as f32 * 0.85) as u8,
    )
}

// ── Eye-tracking puro (testeable sin egui) ────────────────────────────────
/// Calcula posición de pupila con tracking. `eye_center` en coords pantalla,
/// `hover` opcional (None = centrar). Retorna offset relativo a eye_center.
pub fn eye_pupil_offset(
    eye_center: Pos2,
    eye_radius: f32,
    pupil_radius: f32,
    hover: Option<Pos2>,
    eye_tracking: bool,
) -> egui::Vec2 {
    if !eye_tracking {
        return egui::Vec2::ZERO;
    }
    let Some(hover) = hover else {
        return egui::Vec2::ZERO;
    };
    let dir = hover - eye_center;
    let dist = dir.length();
    if dist < 0.5 {
        return egui::Vec2::ZERO;
    }
    let normalized = dir / dist;
    let max_offset = (eye_radius - pupil_radius - 1.0).max(0.0) * 0.45;
    // seguir con factor 0.22 de la distancia para sutileza
    let desired = (dist * 0.22).min(max_offset);
    normalized * desired
}

// ── Shape path ─────────────────────────────────────────────────────────────
fn shape_points(center: Pos2, radius: f32, shape: AvatarShape) -> Vec<Pos2> {
    match shape {
        AvatarShape::Circle => {
            let n = 32;
            (0..n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    center + vec2(t.cos() * radius, t.sin() * radius)
                })
                .collect()
        }
        AvatarShape::Squircle => {
            // superellipse n=4
            let n = 28;
            (0..n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    let cos = t.cos();
                    let sin = t.sin();
                    let _r = radius * (cos.abs().powf(2.2) + sin.abs().powf(2.2)).powf(-1.0 / 2.2);
                    // Actually map to superellipse
                    let x = cos.signum() * cos.abs().powf(0.5) * radius;
                    let y = sin.signum() * sin.abs().powf(0.5) * radius;
                    center + vec2(x, y)
                })
                .collect()
        }
        AvatarShape::RoundedSquare => {
            let r = radius * 0.92;
            let rect = Rect::from_center_size(center, egui::vec2(r * 2.0, r * 2.0));
            // approximate with 8 points + rounding via convex
            vec![
                pos2(rect.min.x + 8.0, rect.min.y),
                pos2(rect.max.x - 8.0, rect.min.y),
                pos2(rect.max.x, rect.min.y + 8.0),
                pos2(rect.max.x, rect.max.y - 8.0),
                pos2(rect.max.x - 8.0, rect.max.y),
                pos2(rect.min.x + 8.0, rect.max.y),
                pos2(rect.min.x, rect.max.y - 8.0),
                pos2(rect.min.x, rect.min.y + 8.0),
            ]
        }
        AvatarShape::Hexagon => {
            let n = 6;
            (0..n)
                .map(|i| {
                    let t =
                        i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    center + vec2(t.cos() * radius, t.sin() * radius)
                })
                .collect()
        }
        AvatarShape::Blob => {
            // orgánico blob 20 puntos con variación radial
            let n = 20;
            (0..n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    let r = radius * (1.0 + 0.12 * (t * 3.0).sin() + 0.06 * (t * 5.0).cos());
                    center + vec2(t.cos() * r, t.sin() * r)
                })
                .collect()
        }
        AvatarShape::Star => {
            let n = 10;
            (0..n)
                .map(|i| {
                    let t =
                        i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { radius } else { radius * 0.45 };
                    center + vec2(t.cos() * r, t.sin() * r)
                })
                .collect()
        }
    }
}

// ── Dibujo principal ───────────────────────────────────────────────────────
/// Dibuja avatar completo. `hover_pos` es la posición global del puntero
/// (si está dentro del rect, los ojos lo siguen). `time` para animación sutil.
/// `bg` permite fondo custom; `None` usa input_bg del tema oscuro/claro.
pub fn draw_avatar(
    painter: &Painter,
    rect: Rect,
    config: &AvatarConfig,
    time: f64,
    hover_pos: Option<Pos2>,
) {
    draw_avatar_with_bg(painter, rect, config, time, hover_pos, None);
}

pub fn draw_avatar_with_bg(
    painter: &Painter,
    rect: Rect,
    config: &AvatarConfig,
    time: f64,
    hover_pos: Option<Pos2>,
    bg_override: Option<Color32>,
) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.42;
    let theme_accent = accent_color(config);
    let theme_strong = accent_strong(config);

    // Fondo shape — usa bg_color custom o blanco/input_bg según tema
    let bg = if let Some(o) = bg_override {
        o
    } else if let Some(rgb) = config.bg_color {
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    } else {
        Color32::WHITE
    };
    let points = shape_points(center, radius, config.shape);
    painter.add(Shape::convex_polygon(points.clone(), bg, Stroke::NONE));
    painter.add(Shape::closed_line(points, Stroke::new(1.5, theme_accent)));

    // Sombra sutil — Scandinavian 0,2,8 alpha 8
    painter.circle_filled(
        center + vec2(0.0, 2.0),
        radius * 0.95,
        Color32::from_black_alpha(8),
    );

    // Blush (rubor)
    match config.blush {
        AvatarBlush::None => {}
        AvatarBlush::Subtle => {
            let blush_c = Color32::from_rgba_unmultiplied(
                theme_accent.r(),
                theme_accent.g(),
                theme_accent.b(),
                18,
            );
            painter.circle_filled(
                center + vec2(-radius * 0.42, radius * 0.22),
                radius * 0.18,
                blush_c,
            );
            painter.circle_filled(
                center + vec2(radius * 0.42, radius * 0.22),
                radius * 0.18,
                blush_c,
            );
        }
        AvatarBlush::Strong => {
            let blush_c = Color32::from_rgba_unmultiplied(
                theme_accent.r(),
                theme_accent.g(),
                theme_accent.b(),
                32,
            );
            painter.circle_filled(
                center + vec2(-radius * 0.42, radius * 0.22),
                radius * 0.22,
                blush_c,
            );
            painter.circle_filled(
                center + vec2(radius * 0.42, radius * 0.22),
                radius * 0.22,
                blush_c,
            );
        }
    }

    // Ojos
    let eye_spacing = radius * config.eye_spacing_factor() * 2.0;
    let eye_r = radius * config.eye_size_factor();
    let pupil_r = eye_r * config.pupil_ratio();
    let eye_y = center.y - radius * 0.08;

    let left_eye = pos2(center.x - eye_spacing * 0.5, eye_y);
    let right_eye = pos2(center.x + eye_spacing * 0.5, eye_y);

    // Parpadeo sutil basado en blink_speed
    let blink_phase = if config.blink_speed == 0 {
        1.0
    } else {
        let speed = 0.8 + (config.blink_speed as f64 / 100.0) * 2.0;
        let t = time * speed;
        let frac = t.fract() as f32;
        // parpadeo cada ~3s, duración 0.12
        if frac > 0.96 {
            1.0 - (frac - 0.96) / 0.04
        } else if frac < 0.04 {
            frac / 0.04
        } else {
            1.0
        }
    };
    let eye_scale_y = blink_phase.clamp(0.15, 1.0);

    for eye_center in [left_eye, right_eye] {
        // Esclerótica
        let offset = eye_pupil_offset(eye_center, eye_r, pupil_r, hover_pos, config.eye_tracking);
        // Dibujar ojo según eye_style
        match config.eye_style {
            AvatarEyeStyle::Round => {
                // ojo redondo
                painter.circle_filled(eye_center, eye_r, Color32::WHITE);
                painter.circle_stroke(
                    eye_center,
                    eye_r,
                    Stroke::new(1.0, theme_accent.gamma_multiply(0.25)),
                );
                // pupila con tracking
                let pupil_pos = eye_center + offset;
                painter.circle_filled(pupil_pos, pupil_r, theme_strong);
                // brillo
                painter.circle_filled(
                    pupil_pos + vec2(pupil_r * 0.35, -pupil_r * 0.35),
                    pupil_r * 0.32,
                    Color32::WHITE,
                );
            }
            AvatarEyeStyle::Almond => {
                // almendra
                let w = eye_r * 1.6;
                let h = eye_r * 0.9 * eye_scale_y;
                let _rect_eye = Rect::from_center_size(eye_center, egui::vec2(w, h * 2.0));
                // aproximar con ellipse usando puntos
                let n = 16;
                let mut pts = Vec::with_capacity(n);
                for i in 0..n {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    // ellipse
                    pts.push(eye_center + vec2(t.cos() * w * 0.5, t.sin() * h));
                }
                painter.add(Shape::convex_polygon(
                    pts.clone(),
                    Color32::WHITE,
                    Stroke::new(1.0, theme_accent.gamma_multiply(0.25)),
                ));
                let pupil_pos = eye_center + offset * 0.6;
                // clip pupila dentro
                painter.circle_filled(pupil_pos, pupil_r * 0.9, theme_strong);
                painter.circle_filled(
                    pupil_pos + vec2(pupil_r * 0.3, -pupil_r * 0.3),
                    pupil_r * 0.28,
                    Color32::WHITE,
                );
            }
            AvatarEyeStyle::Dot => {
                // punto minimal
                painter.circle_filled(eye_center, eye_r * 0.7, Color32::WHITE);
                let pupil_pos = eye_center + offset * 0.4;
                painter.circle_filled(pupil_pos, pupil_r * 0.85, theme_strong);
            }
            AvatarEyeStyle::Wide => {
                let w = eye_r * 1.9;
                let h = eye_r * 1.1 * eye_scale_y;
                let _rect_eye = Rect::from_center_size(eye_center, egui::vec2(w, h));
                painter.add(Shape::convex_polygon(
                    vec![
                        eye_center + vec2(-w * 0.5, 0.0),
                        eye_center + vec2(0.0, -h * 0.5),
                        eye_center + vec2(w * 0.5, 0.0),
                        eye_center + vec2(0.0, h * 0.5),
                    ],
                    Color32::WHITE,
                    Stroke::new(1.0, theme_accent.gamma_multiply(0.25)),
                ));
                let pupil_pos = eye_center + offset * 0.5;
                painter.circle_filled(pupil_pos, pupil_r, theme_strong);
            }
            AvatarEyeStyle::Winky => {
                // un ojo guiña
                let is_left = eye_center == left_eye;
                if is_left {
                    // cerrado con línea
                    painter.line_segment(
                        [
                            eye_center + vec2(-eye_r * 0.9, 0.0),
                            eye_center + vec2(eye_r * 0.9, 0.0),
                        ],
                        Stroke::new(1.6, theme_strong),
                    );
                    painter.line_segment(
                        [
                            eye_center + vec2(-eye_r * 0.6, -2.0),
                            eye_center + vec2(eye_r * 0.6, -2.0),
                        ],
                        Stroke::new(1.0, theme_accent.gamma_multiply(0.35)),
                    );
                } else {
                    painter.circle_filled(eye_center, eye_r, Color32::WHITE);
                    painter.circle_stroke(
                        eye_center,
                        eye_r,
                        Stroke::new(1.0, theme_accent.gamma_multiply(0.25)),
                    );
                    let pupil_pos = eye_center + offset;
                    painter.circle_filled(pupil_pos, pupil_r, theme_strong);
                    painter.circle_filled(
                        pupil_pos + vec2(pupil_r * 0.35, -pupil_r * 0.35),
                        pupil_r * 0.32,
                        Color32::WHITE,
                    );
                }
            }
            AvatarEyeStyle::Closed => {
                painter.line_segment(
                    [
                        eye_center + vec2(-eye_r * 0.9, 0.0),
                        eye_center + vec2(eye_r * 0.9, 0.0),
                    ],
                    Stroke::new(1.5, theme_strong),
                );
            }
        }
    }

    // Boca
    let mouth_y = center.y + radius * 0.28;
    match config.mouth_style {
        AvatarMouthStyle::Hidden => {}
        AvatarMouthStyle::Line => {
            painter.line_segment(
                [
                    pos2(center.x - radius * 0.18, mouth_y),
                    pos2(center.x + radius * 0.18, mouth_y),
                ],
                Stroke::new(1.5, theme_strong),
            );
        }
        AvatarMouthStyle::Smile => {
            let mut pts = Vec::new();
            for i in 0..=12 {
                let t = i as f32 / 12.0;
                let x = center.x - radius * 0.18 + t * radius * 0.36;
                let y = mouth_y + (t * std::f32::consts::PI).sin() * radius * 0.08;
                pts.push(pos2(x, y));
            }
            painter.add(Shape::line(pts, Stroke::new(1.5, theme_strong)));
        }
        AvatarMouthStyle::Small => {
            painter.circle_filled(pos2(center.x, mouth_y), radius * 0.045, theme_strong);
        }
        AvatarMouthStyle::Open => {
            painter.circle_filled(
                pos2(center.x, mouth_y),
                radius * 0.07,
                Color32::from_rgb(60, 60, 65),
            );
            painter.circle_filled(
                pos2(center.x, mouth_y + 2.0),
                radius * 0.04,
                Color32::from_rgb(200, 80, 80),
            );
        }
        AvatarMouthStyle::Teeth => {
            let rect =
                Rect::from_center_size(pos2(center.x, mouth_y), vec2(radius * 0.28, radius * 0.10));
            painter.rect_filled(rect, 4.0, Color32::WHITE);
            painter.rect_stroke(rect, 4.0, Stroke::new(1.2, theme_strong));
            painter.line_segment(
                [
                    pos2(rect.center().x, rect.min.y),
                    pos2(rect.center().x, rect.max.y),
                ],
                Stroke::new(1.0, theme_strong.gamma_multiply(0.6)),
            );
        }
    }

    // Accesorio
    match config.accessory {
        AvatarAccessory::None => {}
        AvatarAccessory::Glasses => {
            let left = left_eye;
            let right = right_eye;
            let r = eye_r * 1.25;
            painter.circle_stroke(left, r, Stroke::new(1.5, theme_strong));
            painter.circle_stroke(right, r, Stroke::new(1.5, theme_strong));
            painter.line_segment(
                [pos2(left.x + r, left.y), pos2(right.x - r, right.y)],
                Stroke::new(1.5, theme_strong),
            );
            // patillas
            painter.line_segment(
                [
                    pos2(left.x - r, left.y),
                    pos2(left.x - r - radius * 0.18, left.y - radius * 0.05),
                ],
                Stroke::new(1.2, theme_strong),
            );
            painter.line_segment(
                [
                    pos2(right.x + r, right.y),
                    pos2(right.x + r + radius * 0.18, right.y - radius * 0.05),
                ],
                Stroke::new(1.2, theme_strong),
            );
        }
        AvatarAccessory::Sparkle => {
            let sparkle = center + vec2(radius * 0.55, -radius * 0.55);
            for i in 0..4 {
                let a = i as f32 * std::f32::consts::TAU / 4.0;
                let p1 = sparkle + vec2(a.cos() * radius * 0.08, a.sin() * radius * 0.08);
                let p2 = sparkle + vec2(a.cos() * radius * 0.18, a.sin() * radius * 0.18);
                painter.line_segment([p1, p2], Stroke::new(1.5, theme_accent));
            }
        }
        AvatarAccessory::Halo => {
            let halo_y = center.y - radius * 1.05;
            let mut pts = Vec::new();
            for i in 0..=24 {
                let t = i as f32 / 24.0 * std::f32::consts::TAU;
                pts.push(pos2(
                    center.x + t.cos() * radius * 0.55,
                    halo_y + t.sin() * radius * 0.12,
                ));
            }
            painter.add(Shape::closed_line(pts, Stroke::new(1.8, theme_accent)));
        }
        AvatarAccessory::Hat => {
            let top = pos2(center.x, center.y - radius * 1.05);
            let base = center.y - radius * 0.65;
            let left = center.x - radius * 0.45;
            let right = center.x + radius * 0.45;
            painter.add(Shape::convex_polygon(
                vec![top, pos2(right, base), pos2(left, base)],
                theme_accent.gamma_multiply(0.85),
                Stroke::new(1.4, theme_strong),
            ));
            painter.rect_filled(
                Rect::from_min_max(pos2(left - 6.0, base - 4.0), pos2(right + 6.0, base + 4.0)),
                3.0,
                theme_accent,
            );
        }
        AvatarAccessory::Beard => {
            let beard_center = pos2(center.x, mouth_y + radius * 0.08);
            let points = vec![
                beard_center + vec2(-radius * 0.20, -radius * 0.05),
                beard_center + vec2(radius * 0.20, -radius * 0.05),
                beard_center + vec2(radius * 0.12, radius * 0.18),
                beard_center + vec2(-radius * 0.12, radius * 0.18),
            ];
            painter.add(Shape::convex_polygon(
                points,
                theme_strong.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
    }
}

/// Morph avatar → burbuja: interpola radio y forma para transición.
/// `t` 0=avatar círculo (32×32 pill), 1=burbuja rectangular redondeada.
/// Usa ease-out cúbico (`1-(1-t)³`) y `ANIM_MICRO` 180 ms en el llamador.
/// Retorna `(rect_burbuja, radius)` con radius interpolado 16→12 (RADIUS_LG→RADIUS_MD).
pub fn avatar_bubble_morph_rect(base: Rect, t: f32) -> (Rect, f32) {
    let t = t.clamp(0.0, 1.0);
    let e = 1.0 - (1.0 - t).powi(3); // ease-out cúbico
                                     // expandir ancho, aplanar levemente alto — morph Scandinavian
    let w = base.width() * (1.0 + e * 2.2);
    let h = base.height() * (1.0 - e * 0.25);
    let rect = Rect::from_center_size(base.center(), egui::vec2(w, h));
    let r = 16.0 + (12.0 - 16.0) * e; // RADIUS_LG 16 → RADIUS_MD 12
    (rect, r)
}

/// Progreso 0..=1 del morph avatar→burbuja según tiempo transcurrido.
/// `elapsed_ms` en milisegundos, `duration_ms` = `ANIM_MICRO` (180 ms).
pub fn bubble_morph_progress(elapsed_ms: f32, duration_ms: f32) -> f32 {
    if duration_ms <= 0.0 {
        return 1.0;
    }
    (elapsed_ms / duration_ms).clamp(0.0, 1.0)
}

/// Ease-out cúbico puro (0→1). Usado por el morph y tests.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eye_tracking_centers_when_no_hover() {
        let c = pos2(100.0, 100.0);
        assert_eq!(eye_pupil_offset(c, 10.0, 4.0, None, true), egui::Vec2::ZERO);
    }
    #[test]
    fn eye_tracking_disabled_returns_zero() {
        let c = pos2(100.0, 100.0);
        let h = pos2(200.0, 100.0);
        assert_eq!(
            eye_pupil_offset(c, 10.0, 4.0, Some(h), false),
            egui::Vec2::ZERO
        );
    }
    #[test]
    fn eye_tracking_follows() {
        let c = pos2(100.0, 100.0);
        let h = pos2(110.0, 100.0);
        let off = eye_pupil_offset(c, 10.0, 3.0, Some(h), true);
        assert!(off.x > 0.0 && off.length() < 6.0);
    }
    #[test]
    fn shape_points_nonempty() {
        for s in AvatarShape::all() {
            let pts = shape_points(pos2(0.0, 0.0), 20.0, *s);
            assert!(!pts.is_empty());
        }
    }
    #[test]
    fn bubble_morph_interpolates_smoothly() {
        let base = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(32.0, 32.0));
        let (r0, rad0) = avatar_bubble_morph_rect(base, 0.0);
        let (r1, rad1) = avatar_bubble_morph_rect(base, 1.0);
        assert!((r0.width() - 32.0).abs() < 0.01);
        assert!((r1.width() - 32.0 * 3.2).abs() < 0.01);
        assert!((rad0 - 16.0).abs() < 0.01);
        assert!((rad1 - 12.0).abs() < 0.01);
        // ease-out: midpoint should be > linear 14
        let (_, rad_mid) = avatar_bubble_morph_rect(base, 0.5);
        assert!(rad_mid < 14.0, "ease-out should bend toward target early");
        assert!(rad_mid > 12.0 && rad_mid < 16.0);
    }
    #[test]
    fn bubble_morph_progress_clamped() {
        assert_eq!(bubble_morph_progress(0.0, 180.0), 0.0);
        assert_eq!(bubble_morph_progress(180.0, 180.0), 1.0);
        assert_eq!(bubble_morph_progress(300.0, 180.0), 1.0);
        assert_eq!(bubble_morph_progress(90.0, 180.0), 0.5);
        assert_eq!(bubble_morph_progress(10.0, 0.0), 1.0);
    }
    #[test]
    fn ease_out_cubic_is_monotonic() {
        let a = ease_out_cubic(0.25);
        let b = ease_out_cubic(0.5);
        let c = ease_out_cubic(0.75);
        assert!(a < b && b < c);
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-6);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-6);
    }
}
