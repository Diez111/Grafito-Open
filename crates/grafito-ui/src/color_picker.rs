//! HSV Color Picker con rueda de color, slider de valor y favoritos
//!
//! Características:
//! - Rueda de color HSV interactiva
//! - Slider de valor (brightness)
//! - Preview de color actual vs nuevo
//! - 5 colores favoritos editables
//! - Diseño responsive (mobile/desktop)
//! - Persistencia con eframe storage

use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};
use grafito_geometry::Color;

/// Estado del color picker HSV
#[derive(Clone, Debug)]
pub struct HsvColorPicker {
    /// Tono (0-360)
    pub hue: f32,
    /// Saturación (0-1)
    pub saturation: f32,
    /// Valor/Brillo (0-1)
    pub value: f32,
    /// Color original (para preview)
    pub original_color: Color,
}

impl HsvColorPicker {
    /// Crear nuevo picker desde un color RGB
    pub fn new(color: Color) -> Self {
        let (h, s, v) = rgb_to_hsv(color);
        Self {
            hue: h,
            saturation: s,
            value: v,
            original_color: color,
        }
    }

    /// Obtener color RGB actual
    pub fn to_color(&self) -> Color {
        hsv_to_rgb(self.hue, self.saturation, self.value)
    }

    /// Actualizar desde color RGB
    pub fn set_color(&mut self, color: Color) {
        let (h, s, v) = rgb_to_hsv(color);
        self.hue = h;
        self.saturation = s;
        self.value = v;
    }

    /// Dibujar el color picker completo
    /// Retorna true si el color cambió
    pub fn show(&mut self, ui: &mut Ui, favorites: &mut [Color; 5]) -> bool {
        let mut changed = false;

        ui.columns(2, |cols| {
            // Columna Izquierda: Rueda
            cols[0].vertical_centered(|ui| {
                changed |= self.show_wheel(ui, 136.0);
            });

            // Columna Derecha: Slider y Preview
            cols[1].vertical_centered(|ui| {
                ui.add_space(2.0);
                changed |= self.show_value_slider(ui, 136.0);
                ui.add_space(12.0);
                changed |= self.show_preview(ui, 136.0);
            });
        });

        ui.add_space(12.0);
        changed |= self.show_favorites(ui, favorites);

        changed
    }

    /// Dibujar rueda de color HSV con un gradiente Mesh ultra-suave
    fn show_wheel(&mut self, ui: &mut Ui, size: f32) -> bool {
        let (response, painter) = ui.allocate_painter(Vec2::splat(size), Sense::click_and_drag());
        let rect = response.rect;
        let center = rect.center();
        let radius = size * 0.45;
        let inner_radius = radius * 0.3;
        let outer_radius = radius;

        // Generar malla (Mesh) para un gradiente continuo y perfecto
        let mut mesh = egui::Mesh::default();
        let segments = 64;

        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let hue = (i as f32 / segments as f32) * 360.0;

            // Color en el borde exterior (saturación máxima, brillo máximo)
            let c_out = hsv_to_color32(hue, 1.0, 1.0);
            // Color en el borde interior (saturación 0 es blanco)
            let c_in = Color32::WHITE;

            let cos = angle.cos();
            let sin = angle.sin();

            let p_in = center + Vec2::new(cos, sin) * inner_radius;
            let p_out = center + Vec2::new(cos, sin) * outer_radius;

            mesh.vertices.push(egui::epaint::Vertex {
                pos: p_in,
                uv: egui::Pos2::ZERO,
                color: c_in,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p_out,
                uv: egui::Pos2::ZERO,
                color: c_out,
            });
        }

        // Generar índices para las caras (triángulos) de la rueda
        for i in 0..segments {
            let next_i = (i + 1) % segments;
            let v0 = 2 * i as u32;
            let v1 = v0 + 1;
            let v2 = 2 * next_i as u32;
            let v3 = v2 + 1;

            mesh.indices.push(v0);
            mesh.indices.push(v1);
            mesh.indices.push(v2);

            mesh.indices.push(v1);
            mesh.indices.push(v3);
            mesh.indices.push(v2);
        }

        painter.add(egui::Shape::Mesh(mesh));

        // Bordes de la rueda adaptados al tema actual
        let border_color = ui.visuals().widgets.noninteractive.bg_stroke.color;
        painter.circle_stroke(center, inner_radius, egui::Stroke::new(1.0, border_color));
        painter.circle_stroke(center, outer_radius, egui::Stroke::new(1.0, border_color));

        // Dibujar indicador de posición actual (anillo hueco con alto contraste)
        let angle = (self.hue / 360.0) * std::f32::consts::TAU;
        let indicator_radius = inner_radius + (outer_radius - inner_radius) * self.saturation;
        let indicator_pos = center + Vec2::new(angle.cos(), angle.sin()) * indicator_radius;

        // Sombra sutil del anillo
        painter.circle_stroke(
            indicator_pos + Vec2::new(0.0, 1.5),
            6.0,
            egui::Stroke::new(2.0, Color32::from_black_alpha(80)),
        );

        // Anillo de selección: doble trazo para máxima visibilidad (blanco por fuera, negro por dentro)
        painter.circle_stroke(indicator_pos, 6.0, egui::Stroke::new(2.0, Color32::WHITE));
        painter.circle_stroke(indicator_pos, 5.0, egui::Stroke::new(1.0, Color32::BLACK));

        // Manejar interacción — click+drag continuo e intuitivo (incluso fuera del círculo)
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if self.value < 0.01 {
                    self.value = 1.0;
                }
                let delta = pos - center;
                let distance = delta.length();
                let angle = delta.y.atan2(delta.x);
                self.hue = ((angle / std::f32::consts::TAU) * 360.0 + 360.0) % 360.0;
                self.saturation =
                    ((distance - inner_radius) / (outer_radius - inner_radius)).clamp(0.0, 1.0);
                return true;
            }
        }

        false
    }

    /// Dibujar slider de valor (brightness) sin saltos y con indicador no recortado
    fn show_value_slider(&mut self, ui: &mut Ui, width: f32) -> bool {
        let height = 24.0;

        ui.label(
            egui::RichText::new("Valor (Brillo):")
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        ui.add_space(4.0);

        let (response, mut painter) =
            ui.allocate_painter(Vec2::new(width, height), Sense::click_and_drag());
        let rect = response.rect;

        // Guardar clip rect original para evitar recortar el indicador
        let original_clip_rect = painter.clip_rect();
        painter.set_clip_rect(rect);

        // Dibujar gradiente con redondeo solo en los extremos (evita huecos visuales)
        let segments = 32;
        for i in 0..segments {
            let val1 = i as f32 / segments as f32;
            let val2 = (i + 1) as f32 / segments as f32;

            let color1 = hsv_to_color32(self.hue, self.saturation, val1);

            let x1 = rect.left() + rect.width() * val1;
            let x2 = rect.left() + rect.width() * val2;

            let rounding = if i == 0 {
                egui::Rounding {
                    nw: 6.0,
                    ne: 0.0,
                    sw: 6.0,
                    se: 0.0,
                }
            } else if i == segments - 1 {
                egui::Rounding {
                    nw: 0.0,
                    ne: 6.0,
                    sw: 0.0,
                    se: 6.0,
                }
            } else {
                egui::Rounding::ZERO
            };

            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x1, rect.top()), Pos2::new(x2, rect.bottom())),
                rounding,
                color1,
            );
        }

        // Restaurar clip rect para el borde exterior y el indicador
        painter.set_clip_rect(original_clip_rect);

        // Borde exterior del slider adaptativo
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        // Dibujar indicador (cápsula blanca con sombra y borde adaptado)
        let indicator_x = rect.left() + (rect.width() - 8.0) * self.value + 4.0;
        let ind_rect = Rect::from_center_size(
            Pos2::new(indicator_x, rect.center().y),
            Vec2::new(8.0, rect.height() + 4.0),
        );

        // Sombra del indicador
        painter.rect_filled(
            ind_rect.translate(Vec2::new(0.0, 1.5)),
            4.0,
            Color32::from_black_alpha(60),
        );

        // Cuerpo del indicador
        painter.rect_filled(ind_rect, 4.0, Color32::WHITE);
        painter.rect_stroke(
            ind_rect,
            4.0,
            egui::Stroke::new(1.5, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        // Manejar interacción — click+drag absoluto
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let value = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.value = value;
                return true;
            }
        }

        false
    }

    /// Dibujar preview de color (actual vs nuevo) como una tarjeta unificada dividida
    fn show_preview(&mut self, ui: &mut Ui, width: f32) -> bool {
        ui.label(
            egui::RichText::new("Previsualización:")
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        ui.add_space(4.0);

        let height = 32.0;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        let painter = ui.painter();

        let left_rect = Rect::from_min_max(rect.min, Pos2::new(rect.center().x, rect.max.y));
        let right_rect = Rect::from_min_max(Pos2::new(rect.center().x, rect.min.y), rect.max);

        let left_rounding = egui::Rounding {
            nw: 6.0,
            ne: 0.0,
            sw: 6.0,
            se: 0.0,
        };
        let right_rounding = egui::Rounding {
            nw: 0.0,
            ne: 6.0,
            sw: 0.0,
            se: 6.0,
        };

        painter.rect_filled(
            left_rect,
            left_rounding,
            color_to_color32(self.original_color),
        );
        painter.rect_filled(
            right_rect,
            right_rounding,
            color_to_color32(self.to_color()),
        );

        // Borde exterior
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        // Línea divisoria sutil
        painter.line_segment(
            [rect.center_top(), rect.center_bottom()],
            egui::Stroke::new(1.0, ui.visuals().panel_fill.linear_multiply(0.8)),
        );

        let mut changed = false;
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x < rect.center().x {
                    let old_color = self.to_color();
                    self.set_color(self.original_color);
                    changed = self.to_color() != old_color;
                }
            }
        }

        response.on_hover_text("Clic en 'Original' para restaurar");

        // Etiquetas
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let half_w = width / 2.0;
            ui.allocate_ui(Vec2::new(half_w, 16.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Original").small().weak());
                });
            });
            ui.allocate_ui(Vec2::new(half_w, 16.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Nuevo").small().strong());
                });
            });
        });

        changed
    }

    /// Dibujar colores favoritos alineados y centrados horizontalmente
    fn show_favorites(&mut self, ui: &mut Ui, favorites: &mut [Color; 5]) -> bool {
        let mut changed = false;

        ui.label(
            egui::RichText::new("Favoritos:")
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        ui.add_space(4.0);

        let item_w = 32.0;
        let spacing = 8.0;
        let total_width = 5.0 * item_w + 4.0 * spacing;

        ui.horizontal(|ui| {
            let avail_w = ui.available_width();
            let offset = ((avail_w - total_width) / 2.0).max(0.0);
            ui.add_space(offset);

            ui.spacing_mut().item_spacing.x = spacing;
            #[allow(clippy::needless_range_loop)]
            for i in 0..5 {
                let color = favorites[i];
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(item_w), Sense::click());

                painter_draw_swatch_interactive(ui, &response, rect, color_to_color32(color));

                // Click izquierdo: aplicar color favorito
                if response.clicked() {
                    self.set_color(color);
                    changed = true;
                }

                // Click derecho: guardar color actual como favorito
                if response.secondary_clicked() {
                    favorites[i] = self.to_color();
                    changed = true;
                }

                // Tooltip
                response.on_hover_text(format!(
                    "Click: aplicar\nClick derecho: guardar\nRGB: ({:.2}, {:.2}, {:.2})",
                    color.r, color.g, color.b
                ));
            }
        });

        changed
    }
}

// Funciones helpers para dibujar swatches con calidad premium

fn painter_draw_swatch_interactive(
    ui: &mut Ui,
    response: &egui::Response,
    rect: Rect,
    color: Color32,
) {
    let painter = ui.painter();

    // Sombra
    painter.rect_filled(
        rect.translate(Vec2::new(0.0, 2.0)),
        6.0,
        Color32::from_black_alpha(30),
    );

    // Dibujar swatch
    painter.rect_filled(rect, 6.0, color);

    if response.hovered() {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(2.0, ui.visuals().hyperlink_color),
        );
    } else {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
    }
}

/// Convertir RGB a HSV
/// Retorna (hue [0-360], saturation [0-1], value [0-1])
pub fn rgb_to_hsv(color: Color) -> (f32, f32, f32) {
    let r = color.r;
    let g = color.g;
    let b = color.b;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    // Value
    let value = max;

    // Saturation
    let saturation = if max == 0.0 { 0.0 } else { delta / max };

    // Hue
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let hue = if hue < 0.0 { hue + 360.0 } else { hue };

    (hue, saturation, value)
}

/// Convertir HSV a RGB
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color {
    let c = value * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - c;

    let (r, g, b) = if hue < 60.0 {
        (c, x, 0.0)
    } else if hue < 120.0 {
        (x, c, 0.0)
    } else if hue < 180.0 {
        (0.0, c, x)
    } else if hue < 240.0 {
        (0.0, x, c)
    } else if hue < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    Color::new(r + m, g + m, b + m, 1.0)
}

/// Convertir HSV a Color32 (para egui)
fn hsv_to_color32(hue: f32, saturation: f32, value: f32) -> Color32 {
    let color = hsv_to_rgb(hue, saturation, value);
    color_to_color32(color)
}

/// Convertir Color a Color32
fn color_to_color32(color: Color) -> Color32 {
    Color32::from_rgba_premultiplied(
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        (color.a * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_hsv_red() {
        let color = Color::new(1.0, 0.0, 0.0, 1.0);
        let (h, s, v) = rgb_to_hsv(color);
        assert!((h - 0.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_green() {
        let color = Color::new(0.0, 1.0, 0.0, 1.0);
        let (h, s, v) = rgb_to_hsv(color);
        assert!((h - 120.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_blue() {
        let color = Color::new(0.0, 0.0, 1.0, 1.0);
        let (h, s, v) = rgb_to_hsv(color);
        assert!((h - 240.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hsv_to_rgb_red() {
        let color = hsv_to_rgb(0.0, 1.0, 1.0);
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.0).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_hsv_to_rgb_green() {
        let color = hsv_to_rgb(120.0, 1.0, 1.0);
        assert!((color.r - 0.0).abs() < 0.01);
        assert!((color.g - 1.0).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_hsv_to_rgb_blue() {
        let color = hsv_to_rgb(240.0, 1.0, 1.0);
        assert!((color.r - 0.0).abs() < 0.01);
        assert!((color.g - 0.0).abs() < 0.01);
        assert!((color.b - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = Color::new(0.5, 0.3, 0.8, 1.0);
        let (h, s, v) = rgb_to_hsv(original);
        let converted = hsv_to_rgb(h, s, v);

        assert!((original.r - converted.r).abs() < 0.01);
        assert!((original.g - converted.g).abs() < 0.01);
        assert!((original.b - converted.b).abs() < 0.01);
    }

    #[test]
    fn test_grayscale() {
        let color = Color::new(0.5, 0.5, 0.5, 1.0);
        let (_h, s, v) = rgb_to_hsv(color);
        assert!((s - 0.0).abs() < 0.01); // Sin saturación
        assert!((v - 0.5).abs() < 0.01);
    }
}
