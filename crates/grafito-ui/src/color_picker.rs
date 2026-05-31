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
                changed |= self.show_wheel(ui, 150.0);
            });
            
            // Columna Derecha: Slider y Preview
            cols[1].vertical_centered(|ui| {
                ui.add_space(5.0);
                changed |= self.show_value_slider(ui, 140.0);
                ui.add_space(15.0);
                self.show_preview(ui);
            });
        });

        ui.add_space(15.0);
        ui.vertical_centered(|ui| {
            changed |= self.show_favorites(ui, favorites);
        });

        changed
    }

    /// Dibujar rueda de color HSV
    fn show_wheel(&mut self, ui: &mut Ui, size: f32) -> bool {
        let (response, painter) = ui.allocate_painter(Vec2::splat(size), Sense::click_and_drag());
        let rect = response.rect;
        let center = rect.center();
        let radius = size * 0.45;

        // Dibujar rueda de color (círculo HSV)
        let segments = 64;
        for i in 0..segments {
            let angle1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let angle2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
            
            // Dibujar sector con gradiente de saturación
            let inner_radius = radius * 0.3;
            let outer_radius = radius;
            
            for j in 0..8 {
                let sat1 = j as f32 / 8.0;
                let sat2 = (j + 1) as f32 / 8.0;
                
                let hue = (i as f32 / segments as f32) * 360.0;
                // Siempre dibujamos la rueda con valor = 1.0 para que se vean los colores
                let color1 = hsv_to_color32(hue, sat1, 1.0);
                
                let p1 = center + Vec2::new(angle1.cos(), angle1.sin()) * (inner_radius + (outer_radius - inner_radius) * sat1);
                let p2 = center + Vec2::new(angle1.cos(), angle1.sin()) * (inner_radius + (outer_radius - inner_radius) * sat2);
                let p3 = center + Vec2::new(angle2.cos(), angle2.sin()) * (inner_radius + (outer_radius - inner_radius) * sat2);
                let p4 = center + Vec2::new(angle2.cos(), angle2.sin()) * (inner_radius + (outer_radius - inner_radius) * sat1);
                
                painter.add(egui::Shape::convex_polygon(
                    vec![p1, p2, p3, p4],
                    color1,
                    egui::Stroke::NONE,
                ));
            }
        }

        // Hue hueco en el medio, mejor borde suave (anti-aliasing)
        painter.circle_stroke(
            center,
            radius * 0.3,
            egui::Stroke::new(1.0, Color32::from_gray(100)),
        );
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.0, Color32::from_gray(100)),
        );

        // Dibujar indicador de posición actual
        let angle = (self.hue / 360.0) * std::f32::consts::TAU;
        let indicator_radius = radius * (0.3 + 0.7 * self.saturation);
        let indicator_pos = center + Vec2::new(angle.cos(), angle.sin()) * indicator_radius;
        
        // Sombra sutil del indicador
        painter.circle_filled(
            indicator_pos + Vec2::new(0.0, 2.0),
            6.0,
            Color32::from_black_alpha(60),
        );
        
        painter.circle_filled(
            indicator_pos,
            6.0,
            Color32::WHITE,
        );
        painter.circle_stroke(
            indicator_pos,
            6.0,
            egui::Stroke::new(2.0, Color32::BLACK),
        );

        // Manejar interacción con drag_delta (movimiento relativo, inmune a offset de Window)
        if response.dragged() {
            let delta = response.drag_delta();
            self.hue = (self.hue + delta.x * 0.8).rem_euclid(360.0);
            self.saturation = (self.saturation + delta.y * 0.005).clamp(0.0, 1.0);
            return true;
        }
        if response.clicked() {
            if let Some(pos) = response.hover_pos() {
                let delta = pos - rect.center();
                let distance = delta.length();
                if distance <= radius && distance >= radius * 0.3 {
                    let angle = delta.y.atan2(delta.x);
                    self.hue = ((angle / std::f32::consts::TAU) * 360.0 + 360.0) % 360.0;
                    self.saturation = ((distance - radius * 0.3) / (radius * 0.7)).clamp(0.0, 1.0);
                    return true;
                }
            }
        }

        false
    }

    /// Dibujar slider de valor (brightness)
    fn show_value_slider(&mut self, ui: &mut Ui, width: f32) -> bool {
        let height = 24.0;
        
        ui.label(egui::RichText::new("Valor (Brillo):").strong());
        ui.add_space(4.0);
        
        let (response, mut painter) = ui.allocate_painter(Vec2::new(width, height), Sense::click_and_drag());
        let rect = response.rect;

        // Dibujar gradiente de valor con bordes redondeados
        let segments = 32;
        let rounding = 4.0;
        
        // Usar clip rect para el redondeo
        painter.set_clip_rect(rect);
        for i in 0..segments {
            let val1 = i as f32 / segments as f32;
            let val2 = (i + 1) as f32 / segments as f32;
            
            let color1 = hsv_to_color32(self.hue, self.saturation, val1);
            
            let x1 = rect.left() + rect.width() * val1;
            let x2 = rect.left() + rect.width() * val2;
            
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x1, rect.top()), Pos2::new(x2, rect.bottom())),
                if i == 0 { rounding } else if i == segments - 1 { rounding } else { 0.0 }, // Simple approx
                color1,
            );
        }
        
        // Bordes del slider
        painter.rect_stroke(
            rect,
            rounding,
            egui::Stroke::new(1.0, Color32::from_gray(100)),
        );

        // Dibujar indicador
        let indicator_x = rect.left() + (rect.width() - 8.0) * self.value + 4.0;
        let ind_rect = Rect::from_center_size(Pos2::new(indicator_x, rect.center().y), Vec2::new(8.0, rect.height() + 4.0));
        
        // Sombra
        painter.rect_filled(ind_rect.translate(Vec2::new(0.0, 1.0)), 3.0, Color32::from_black_alpha(60));
        
        // Indicador
        painter.rect_filled(ind_rect, 3.0, Color32::WHITE);
        painter.rect_stroke(ind_rect, 3.0, egui::Stroke::new(1.0, Color32::from_gray(80)));

        // Manejar interacción con drag_delta (movimiento relativo, inmune a offset de Window)
        if response.dragged() {
            let delta = response.drag_delta();
            self.value = (self.value + delta.x / rect.width()).clamp(0.0, 1.0);
            return true;
        }
        if response.clicked() {
            if let Some(pos) = response.hover_pos() {
                let value = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.value = value;
                return true;
            }
        }

        false
    }

    /// Dibujar preview de color (actual vs nuevo)
    fn show_preview(&self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Previsualización:").strong());
        ui.add_space(4.0);
        
        ui.horizontal(|ui| {
            // Color original
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 32.0), Sense::hover());
                painter_draw_swatch(ui, rect, color_to_color32(self.original_color));
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Original").small().color(Color32::from_gray(120)));
            });

            ui.add_space(16.0);

            // Color nuevo
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 32.0), Sense::hover());
                painter_draw_swatch(ui, rect, color_to_color32(self.to_color()));
                ui.add_space(2.0);
                ui.label(egui::RichText::new("Nuevo").small().strong());
            });
        });
    }

    /// Dibujar colores favoritos
    fn show_favorites(&mut self, ui: &mut Ui, favorites: &mut [Color; 5]) -> bool {
        let mut changed = false;
        
        ui.label(egui::RichText::new("Favoritos:").strong());
        ui.add_space(4.0);
        
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for i in 0..5 {
                let color = favorites[i];
                let size = 32.0;
                
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
                
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
fn painter_draw_swatch(ui: &mut Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, color);
    painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, Color32::from_gray(100)));
}

fn painter_draw_swatch_interactive(ui: &mut Ui, response: &egui::Response, rect: Rect, color: Color32) {
    let painter = ui.painter();
    
    // Shadow
    painter.rect_filled(
        rect.translate(Vec2::new(0.0, 2.0)),
        6.0,
        Color32::from_black_alpha(30),
    );
    
    // Dibujar swatch
    painter.rect_filled(
        rect,
        6.0,
        color,
    );
    
    if response.hovered() {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(2.0, Color32::WHITE),
        );
    } else {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, Color32::from_gray(100)),
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
        let (h, s, v) = rgb_to_hsv(color);
        assert!((s - 0.0).abs() < 0.01); // Sin saturación
        assert!((v - 0.5).abs() < 0.01);
    }
}
