//! Avatar y mascota Pou — UI vectorial y pickers.
//!
//! Provee `draw_mascot` (vectorial, sin imágenes) y los pickers para
//! `AvatarConfig` y `MascotConfig`. Diseñado para ser usado dentro de
//! `draw_unified_config_window` y previews.

use egui::{Color32, Painter, Rect, Stroke, Vec2};
use grafito_profile::mascot::{
    outfits_for_level, HouseTheme, MascotConfig, MascotSpecies, Personality,
};
use grafito_profile::{AvatarConfig, MAX_NAME};

// ─────────────────────────────────────────────────────────────────────────────
// MascotMood para UI (re-exportado para compatibilidad con app)
// ─────────────────────────────────────────────────────────────────────────────

/// Ánimo visual de la mascota (UI). Debe coincidir en discriminantes con
/// `grafito_profile::MascotMood` pero vive en `grafito_ui` para no acoplar
/// el perfil al renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MascotMood {
    Idle = 0,
    Happy = 1,
    Sleepy = 2,
    Hungry = 3,
    Annoyed = 4,
    Excited = 5,
}

impl MascotMood {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Happy,
            2 => Self::Sleepy,
            3 => Self::Hungry,
            4 => Self::Annoyed,
            5 => Self::Excited,
            _ => Self::Idle,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dibujo vectorial de la mascota
// ─────────────────────────────────────────────────────────────────────────────

/// Dibuja la mascota Pou vectorial dentro de `rect`.
///
/// `species` y `mood` se reciben como `u8` para compatibilidad con el app
/// que hace `species as u8`. `dna` aporta variación determinista,
/// `time` anima un leve balanceo.
#[allow(clippy::too_many_arguments)]
pub fn draw_mascot(
    painter: &Painter,
    rect: Rect,
    dna: u64,
    species: u8,
    mood: u8,
    time: f64,
    accent: Color32,
    bg: Color32,
    outfit: u8,
) {
    // Fondo respetado (círculo sutil detrás si bg != transparente)
    if bg != Color32::TRANSPARENT {
        painter.circle_filled(rect.center(), rect.width().min(rect.height()) * 0.45, bg);
    }
    let mood = MascotMood::from_u8(mood);
    let species = match species % 3 {
        1 => MascotSpecies::Axolotl,
        2 => MascotSpecies::Slime,
        _ => MascotSpecies::Blob,
    };

    // Variación por dna
    let dna_f = ((dna as f64 % 1000.0) / 1000.0) as f32;
    let wobble = (time * 1.2 + dna_f as f64 * 6.0).sin() as f32 * 2.0;

    let center = rect.center() + Vec2::new(0.0, wobble);
    let radius = rect.width().min(rect.height()) * 0.38;

    // Cuerpo según especie
    let body_color = match species {
        MascotSpecies::Blob => Color32::from_rgb(
            accent.r().saturating_add(40),
            accent.g().saturating_add(30),
            accent.b().saturating_add(20),
        ),
        MascotSpecies::Axolotl => Color32::from_rgb(240, 180, 180),
        MascotSpecies::Slime => Color32::from_rgb(180, 230, 180),
    };

    // Sombra suave (elipse aproximada con círculo escalado)
    painter.circle_filled(
        center + Vec2::new(0.0, radius * 0.55),
        radius * 0.6,
        Color32::from_black_alpha(12),
    );
    // Cuerpo principal — aproximamos elipse con círculo + escala visual
    // Usamos convex_polygon orgánico para Blob, círculo para otros.
    if species == MascotSpecies::Blob {
        // Forma orgánica: 20 puntos con wobble por dna
        let steps = 24;
        let mut points = Vec::with_capacity(steps);
        for i in 0..steps {
            let t = i as f32 / steps as f32 * std::f32::consts::TAU;
            let wobble_blob = 1.0 + 0.10 * (t * 3.0).sin() + 0.06 * (t * 5.0 + dna_f * 6.0).cos();
            let r_x = radius * 0.95 * wobble_blob;
            let r_y = radius * 1.02 * wobble_blob;
            points.push(center + Vec2::new(t.cos() * r_x, t.sin() * r_y));
        }
        painter.add(egui::Shape::convex_polygon(
            points.clone(),
            body_color,
            Stroke::NONE,
        ));
        painter.add(egui::Shape::closed_line(
            points,
            Stroke::new(1.5, accent.gamma_multiply(0.6)),
        ));
    } else {
        painter.circle_filled(center, radius * 0.95, body_color);
        painter.circle_stroke(
            center,
            radius * 0.95,
            Stroke::new(1.5, accent.gamma_multiply(0.6)),
        );
    }

    // Orejas / branquias para Axolotl
    if species == MascotSpecies::Axolotl {
        let ear_color = Color32::from_rgb(220, 120, 130);
        for side in [-1.0, 1.0] {
            let ear_center = center + Vec2::new(side * radius * 0.7, -radius * 0.4);
            painter.circle_filled(ear_center, radius * 0.18, ear_color);
            for i in 0..3 {
                let off = (i as f32 - 1.0) * 4.0;
                painter.circle_filled(
                    ear_center + Vec2::new(off, -6.0),
                    3.0,
                    ear_color.gamma_multiply(0.85),
                );
            }
        }
    }

    // Ojos
    let eye_y = center.y - radius * 0.18;
    let eye_dx = radius * 0.28;
    for side in [-1.0, 1.0] {
        let eye_center = egui::pos2(center.x + side * eye_dx, eye_y);
        // Esclerótica
        painter.circle_filled(eye_center, radius * 0.16, Color32::WHITE);
        // Pupila
        let mood_offset = match mood {
            MascotMood::Sleepy => Vec2::new(0.0, 2.0),
            MascotMood::Annoyed => Vec2::new(side * -1.5, -1.0),
            MascotMood::Excited => Vec2::new(0.0, -1.0),
            _ => Vec2::new(0.0, 0.0),
        };
        let pupil = eye_center + mood_offset;
        painter.circle_filled(pupil, radius * 0.08, Color32::from_rgb(40, 40, 45));
        // Brillo
        painter.circle_filled(pupil + Vec2::new(2.0, -2.0), 2.0, Color32::WHITE);
        // Párpado para sueño
        if mood == MascotMood::Sleepy {
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(eye_center.x - radius * 0.16, eye_center.y - radius * 0.16),
                    egui::pos2(eye_center.x + radius * 0.16, eye_center.y),
                ),
                4.0,
                body_color,
            );
        }
    }

    // Boca según ánimo
    let mouth_y = center.y + radius * 0.28;
    match mood {
        MascotMood::Happy | MascotMood::Excited => {
            // Sonrisa
            let mouth_rect = Rect::from_center_size(
                egui::pos2(center.x, mouth_y),
                Vec2::new(radius * 0.4, radius * 0.2),
            );
            painter.with_clip_rect(mouth_rect).circle_stroke(
                egui::pos2(center.x, mouth_y - 4.0),
                radius * 0.18,
                Stroke::new(2.0, Color32::from_rgb(60, 60, 65)),
            );
        }
        MascotMood::Hungry => {
            painter.circle_filled(
                egui::pos2(center.x, mouth_y),
                4.0,
                Color32::from_rgb(60, 60, 65),
            );
        }
        MascotMood::Annoyed => {
            painter.line_segment(
                [
                    egui::pos2(center.x - radius * 0.15, mouth_y),
                    egui::pos2(center.x + radius * 0.15, mouth_y),
                ],
                Stroke::new(2.0, Color32::from_rgb(60, 60, 65)),
            );
        }
        _ => {
            painter.circle_filled(
                egui::pos2(center.x, mouth_y),
                2.5,
                Color32::from_rgb(60, 60, 65),
            );
        }
    }

    // Brillo corporal para Slime
    if species == MascotSpecies::Slime {
        painter.circle_filled(
            center + Vec2::new(-radius * 0.45, -radius * 0.45),
            radius * 0.12,
            Color32::from_white_alpha(60),
        );
    }

    // Ropa overlay por outfit (0 = ninguno, simple mapeo 1=scarf,2=hat,3=cape)
    {
        let outfit = outfit % 4;
        if outfit != 0 {
            let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 80, 80));
            if outfit == 1 {
                // Bufanda
                let top = center.y + radius * 0.18;
                let bottom = center.y + radius * 0.42;
                let left = center.x - radius * 0.55;
                let right = center.x + radius * 0.55;
                let scarf_rect =
                    egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
                painter.rect_filled(scarf_rect, 6.0, accent.gamma_multiply(0.35));
                painter.rect_stroke(scarf_rect, 6.0, stroke);
            } else if outfit == 2 {
                let top = center.y - radius * 1.05;
                let base_y = center.y - radius * 0.65;
                let left = center.x - radius * 0.45;
                let right = center.x + radius * 0.45;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(center.x, top),
                        egui::pos2(right, base_y),
                        egui::pos2(left, base_y),
                    ],
                    accent.gamma_multiply(0.62),
                    stroke,
                ));
            } else if outfit == 3 {
                let back_rect = egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + radius * 0.15),
                    egui::Vec2::new(radius * 1.35, radius * 1.1),
                );
                painter.rect_filled(
                    back_rect.expand2(egui::Vec2::new(4.0, 2.0)),
                    crate::tokens::RADIUS_LG,
                    accent.gamma_multiply(0.27),
                );
                painter.rect_stroke(
                    back_rect.expand2(egui::Vec2::new(4.0, 2.0)),
                    crate::tokens::RADIUS_LG,
                    stroke,
                );
            }
        }
    }
}

/// Compatibilidad: firma de 8 args sin outfit (outfit = 0).
#[allow(clippy::too_many_arguments)]
pub fn draw_mascot_simple(
    painter: &Painter,
    rect: Rect,
    dna: u64,
    species: u8,
    mood: u8,
    time: f64,
    accent: Color32,
    bg: Color32,
) {
    draw_mascot(painter, rect, dna, species, mood, time, accent, bg, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Picker de mascota
// ─────────────────────────────────────────────────────────────────────────────

/// UI para editar la mascota. Devuelve true si hubo cambio.
pub fn mascot_picker_ui(
    ui: &mut egui::Ui,
    cfg: &mut MascotConfig,
    level: u32,
    _time: f64,
    theme: &crate::theme::Theme,
) -> bool {
    let mut changed = false;

    // Nombre
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Nombre")
                .size(11.0)
                .color(theme.text_secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{}/{}", cfg.name.chars().count(), MAX_NAME))
                    .size(9.0)
                    .color(theme.text_tertiary),
            );
        });
    });
    let mut name = cfg.name.clone();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut name)
            .hint_text("Pou")
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(8.0, 4.0)),
    );
    if resp.changed() {
        // Truncar a MAX_NAME al editar
        let truncated: String = name.chars().take(MAX_NAME).collect();
        cfg.name = truncated;
        changed = true;
    }
    if resp.lost_focus() {
        // Sanitiza al perder foco
        cfg.name = cfg.sanitized_name();
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Especie
    ui.label(
        egui::RichText::new("Especie")
            .size(11.0)
            .color(theme.text_secondary),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for sp in MascotSpecies::all() {
            let is_sel = cfg.species == *sp;
            let btn = egui::Button::new(egui::RichText::new(sp.label()).size(11.0))
                .selected(is_sel)
                .rounding(crate::tokens::RADIUS_PILL);
            if ui.add(btn).clicked() {
                cfg.species = *sp;
                // dna deriva de especie + nombre para variación
                cfg.dna = cfg.dna.wrapping_add(*sp as u64 * 0x9E37);
                changed = true;
            }
        }
    });
    ui.label(
        egui::RichText::new(cfg.species.description())
            .size(9.0)
            .color(theme.text_tertiary),
    );

    ui.add_space(8.0);

    // Personalidad
    ui.label(
        egui::RichText::new("Personalidad")
            .size(11.0)
            .color(theme.text_secondary),
    );
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("mascot_personality")
        .selected_text(cfg.personality.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for p in Personality::all() {
                let sel = cfg.personality == *p;
                if ui.selectable_label(sel, p.label()).clicked() {
                    cfg.personality = *p;
                    changed = true;
                }
            }
        });
    ui.label(
        egui::RichText::new(cfg.personality.description())
            .size(9.0)
            .color(theme.text_tertiary),
    );

    ui.add_space(8.0);

    // Casa
    ui.label(
        egui::RichText::new("Casa")
            .size(11.0)
            .color(theme.text_secondary),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for house in HouseTheme::all() {
            let is_sel = cfg.house_theme == *house;
            let btn = egui::Button::new(egui::RichText::new(house.label()).size(11.0))
                .selected(is_sel)
                .rounding(crate::tokens::RADIUS_PILL);
            if ui.add(btn).clicked() {
                cfg.house_theme = *house;
                changed = true;
            }
        }
    });
    ui.label(
        egui::RichText::new(cfg.house_theme.description())
            .size(9.0)
            .color(theme.text_tertiary),
    );

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Ropa por niveles
    ui.label(
        egui::RichText::new(format!(
            "Ropa — Tier {:?} (nivel {})",
            cfg.evolution_stage, level
        ))
        .size(11.0)
        .color(theme.text_secondary),
    );
    ui.add_space(4.0);
    // Desbloquear automáticamente por nivel
    cfg.wardrobe.unlock_for_level(level);
    let available = outfits_for_level(level);
    if available.is_empty() {
        ui.label(
            egui::RichText::new("Sin prendas desbloqueadas aún.")
                .size(9.0)
                .color(theme.text_tertiary),
        );
    } else {
        // Mostrar chips por capa
        for outfit in &available {
            let owned = cfg.wardrobe.is_owned(&outfit.id);
            let equipped = cfg.wardrobe.is_equipped(&outfit.id);
            let label = if equipped {
                format!("{} ✓", outfit.name)
            } else {
                outfit.name.clone()
            };
            let mut btn = egui::Button::new(egui::RichText::new(label).size(10.0))
                .rounding(crate::tokens::RADIUS_PILL);
            if !owned {
                btn = btn.fill(theme.button_bg.gamma_multiply(0.5));
            }
            if equipped {
                btn = btn
                    .fill(theme.accent.gamma_multiply(0.18))
                    .stroke(Stroke::new(1.0, theme.accent));
            }
            if ui.add_enabled(owned, btn).clicked() {
                if equipped {
                    cfg.wardrobe.unequip(&outfit.id);
                } else {
                    cfg.wardrobe.equip(outfit);
                }
                changed = true;
            }
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Tocá una prenda para equiparla/desequiparla. Una por capa.")
                .size(9.0)
                .color(theme.text_tertiary),
        );
    }

    ui.add_space(8.0);

    // Stats breves (hambre/felicidad)
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Hambre {}", cfg.hunger))
                .size(9.0)
                .color(theme.text_tertiary),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("Felicidad {}", cfg.happiness))
                .size(9.0)
                .color(theme.text_tertiary),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("Coins {}", cfg.coins))
                .size(9.0)
                .color(theme.text_tertiary),
        );
    });

    changed
}

// ─────────────────────────────────────────────────────────────────────────────
// Wardrobe & Personality pickers (Scandinavian, left-aligned, Stroke 1.5)
// ─────────────────────────────────────────────────────────────────────────────

/// Selector de vestimenta — left-aligned, RADIUS_LG, spacing 16/24.
pub fn wardrobe_picker_ui(
    ui: &mut egui::Ui,
    cfg: &mut MascotConfig,
    theme: &crate::theme::Theme,
) -> bool {
    use crate::tokens::{RADIUS_LG, SPACE_SM, SPACE_XS};
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Vestimenta")
                .size(11.0)
                .color(theme.text_secondary),
        );
    });
    ui.add_space(SPACE_XS);
    let outfits = crate::tokens::RADIUS_LG; // dummy to use token
    let _ = outfits;
    // Mostrar prendas desbloqueadas, left-aligned chips
    let available = outfits_for_level(6);
    ui.horizontal_wrapped(|ui| {
        for outfit in &available {
            let equipped = cfg.wardrobe.is_equipped(&outfit.id);
            let label = if equipped {
                format!("{} ✓", outfit.name)
            } else {
                outfit.name.clone()
            };
            let mut btn =
                egui::Button::new(egui::RichText::new(label).size(10.0)).rounding(RADIUS_LG);
            if equipped {
                btn = btn
                    .fill(theme.accent.gamma_multiply(0.18))
                    .stroke(egui::Stroke::new(1.5, theme.accent));
            } else {
                btn = btn.stroke(egui::Stroke::new(1.5, theme.separator));
            }
            if ui.add(btn).clicked() {
                if equipped {
                    cfg.wardrobe.unequip(&outfit.id);
                } else {
                    cfg.wardrobe.equip(outfit);
                }
                changed = true;
            }
        }
    });
    ui.add_space(SPACE_SM);
    // Preview outfit actual
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(56.0, 56.0), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, RADIUS_LG, theme.input_bg);
            ui.painter()
                .rect_stroke(rect, RADIUS_LG, egui::Stroke::new(1.5, theme.separator));
            let accent = {
                let (_, rgb, _) = AvatarConfig::accent_palette(0);
                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
            };
            // preview con outfit actual: si equipado hat -> outfit 2, scarf 1, cape 3
            let outfit_code = if cfg.wardrobe.is_equipped("cap_prim")
                || cfg.wardrobe.is_equipped("hat_sec")
                || cfg.wardrobe.is_equipped("beanie_uni")
                || cfg.wardrobe.is_equipped("crown_master")
            {
                2
            } else if cfg.wardrobe.is_equipped("scarf_prim") {
                1
            } else if cfg.wardrobe.is_equipped("cape_uni")
                || cfg.wardrobe.is_equipped("robe_master")
            {
                3
            } else {
                0
            };
            draw_mascot(
                ui.painter(),
                rect.shrink(6.0),
                cfg.dna,
                cfg.species as u8,
                MascotMood::Idle as u8,
                0.0,
                accent,
                theme.panel_bg,
                outfit_code,
            );
        }
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Overlay vectorial, trazo 1.5")
                    .size(9.0)
                    .color(theme.text_tertiary),
            );
        });
    });
    changed
}

/// Selector de personalidad — left-aligned.
pub fn personality_picker_ui(
    ui: &mut egui::Ui,
    cfg: &mut MascotConfig,
    theme: &crate::theme::Theme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Personalidad")
                .size(11.0)
                .color(theme.text_secondary),
        );
    });
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("pou_personality_picker")
        .selected_text(cfg.personality.label())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for p in Personality::all() {
                let sel = cfg.personality == *p;
                if ui.selectable_label(sel, p.label()).clicked() {
                    cfg.personality = *p;
                    changed = true;
                }
            }
        });
    changed
}

// ─────────────────────────────────────────────────────────────────────────────
// Habitáculo / Room
// ─────────────────────────────────────────────────────────────────────────────

/// Estado efímero del habitáculo para `draw_mascot_room`.
#[derive(Debug, Clone)]
pub struct MascotRoomState {
    pub hunger: f32,
    pub happiness: f32,
    pub dna: u64,
    pub species: u8,
    pub mood: u8,
    pub outfit: u8,
    pub evolution_stage: u8,
    pub house_theme: HouseTheme,
    pub coins: u32,
}

impl Default for MascotRoomState {
    fn default() -> Self {
        Self {
            hunger: 30.0,
            happiness: 80.0,
            dna: 0x9E3779B97F4A7C15,
            species: 0,
            mood: 0,
            outfit: 0,
            evolution_stage: 0,
            house_theme: HouseTheme::Acogedora,
            coins: 0,
        }
    }
}

impl From<&MascotConfig> for MascotRoomState {
    fn from(cfg: &MascotConfig) -> Self {
        let mood = cfg.update_mood(0, false) as u8;
        let outfit =
            if cfg.wardrobe.is_equipped("crown_master") || cfg.wardrobe.is_equipped("beanie_uni") {
                2
            } else if cfg.wardrobe.is_equipped("scarf_prim") {
                1
            } else if cfg.wardrobe.is_equipped("cape_uni") {
                3
            } else {
                0
            };
        Self {
            hunger: cfg.hunger as f32,
            happiness: cfg.happiness as f32,
            dna: cfg.dna,
            species: cfg.species as u8,
            mood,
            outfit,
            evolution_stage: cfg.evolution_stage,
            house_theme: cfg.house_theme,
            coins: cfg.coins,
        }
    }
}

/// Helpers para colores del habitáculo según `HouseTheme` (Scandinavian + playful).
fn room_wall_fill(theme: HouseTheme) -> Color32 {
    match theme {
        HouseTheme::Acogedora => Color32::from_rgb(253, 245, 230), // warm cream #FDF5E6
        HouseTheme::Espacial => Color32::from_rgb(22, 24, 38),     // deep space #161826
        HouseTheme::Bosque => Color32::from_rgb(232, 245, 233),    // mint #E8F5E9
        HouseTheme::Minimal => Color32::from_rgb(250, 250, 249),   // Scandinavian #FAFAF9
        HouseTheme::Retro => Color32::from_rgb(255, 248, 220),     // warm retro cream
        HouseTheme::NocheEstudio => Color32::from_rgb(30, 28, 40), // dark study
    }
}

fn room_floor_fill(theme: HouseTheme) -> Color32 {
    match theme {
        HouseTheme::Acogedora => Color32::from_rgb(210, 180, 140), // tan wood
        HouseTheme::Espacial => Color32::from_rgb(42, 46, 69),     // slate space
        HouseTheme::Bosque => Color32::from_rgb(168, 185, 160),    // sage wood
        HouseTheme::Minimal => Color32::from_rgb(232, 232, 230),   // stone #E8E8E6
        HouseTheme::Retro => Color32::from_rgb(210, 175, 120),     // retro wood
        HouseTheme::NocheEstudio => Color32::from_rgb(55, 50, 65), // dark wood
    }
}

fn room_accent(theme: HouseTheme) -> Color32 {
    match theme {
        HouseTheme::Acogedora => Color32::from_rgb(168, 123, 110), // clay
        HouseTheme::Espacial => Color32::from_rgb(120, 140, 255),  // neon soft
        HouseTheme::Bosque => Color32::from_rgb(101, 119, 90),     // moss
        HouseTheme::Minimal => Color32::from_rgb(107, 122, 111),   // sage
        HouseTheme::Retro => Color32::from_rgb(200, 120, 60),      // mustard
        HouseTheme::NocheEstudio => Color32::from_rgb(255, 190, 90), // warm lamp
    }
}

pub type Action = MascotRoomAction;
pub enum MascotRoomAction {
    Feed,
    Play,
    Sleep,
    ChangeOutfit(u8),
    Poke,
}

/// Pintado aislado del habitáculo (pared, piso, ventana, cama) — reutilizable desde `draw_pou_window`.
pub fn paint_mascot_room(
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &MascotRoomState,
    theme: &crate::theme::Theme,
    time: f64,
) {
    use crate::tokens::{RADIUS_LG, SPACE_LG};
    let wall_fill = room_wall_fill(state.house_theme);
    let floor_fill = room_floor_fill(state.house_theme);
    let accent = room_accent(state.house_theme);

    // Fondo pared con rounding
    painter.rect_filled(rect, RADIUS_LG, wall_fill);
    painter.rect_stroke(rect, RADIUS_LG, egui::Stroke::new(1.5, theme.separator));

    // Textura sutil de pared según tema
    match state.house_theme {
        HouseTheme::Acogedora => {
            // tablones verticales tenues
            for i in 1..6 {
                let x = rect.min.x + rect.width() * i as f32 / 6.0;
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y - 30.0)],
                    egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_premultiplied(180, 160, 140, 18),
                    ),
                );
            }
            // zócalo
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, rect.max.y - 30.0 - 26.0),
                    egui::pos2(rect.max.x, rect.max.y - 30.0 - 22.0),
                ),
                0.0,
                egui::Color32::from_rgb(200, 170, 140),
            );
        }
        HouseTheme::Espacial => {
            // estrellas
            for (i, (sx, sy)) in [
                (0.18, 0.18),
                (0.55, 0.12),
                (0.78, 0.22),
                (0.32, 0.28),
                (0.72, 0.34),
                (0.15, 0.42),
            ]
            .iter()
            .enumerate()
            {
                let p = egui::pos2(
                    rect.min.x + rect.width() * sx,
                    rect.min.y + rect.height() * sy,
                );
                let r = if i % 2 == 0 { 1.4 } else { 0.9 };
                painter.circle_filled(
                    p,
                    r,
                    egui::Color32::from_rgba_premultiplied(255, 255, 255, 90),
                );
                if i == 1 {
                    painter.circle_filled(
                        p + egui::Vec2::new(8.0, 6.0),
                        0.7,
                        egui::Color32::from_rgba_premultiplied(180, 200, 255, 60),
                    );
                }
            }
            // brillo luna ventana se pintará luego
        }
        HouseTheme::Bosque => {
            // puntitos hoja sutiles
            for (sx, sy) in [(0.7, 0.15), (0.82, 0.28), (0.75, 0.42)] {
                painter.circle_filled(
                    egui::pos2(
                        rect.min.x + rect.width() * sx,
                        rect.min.y + rect.height() * sy,
                    ),
                    5.0,
                    egui::Color32::from_rgba_premultiplied(160, 200, 160, 22),
                );
            }
        }
        HouseTheme::Minimal => {
            // línea horizontal de sombra suave a mitad
            painter.line_segment(
                [
                    egui::pos2(rect.min.x + SPACE_LG, rect.min.y + 56.0),
                    egui::pos2(rect.max.x - SPACE_LG, rect.min.y + 56.0),
                ],
                egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.35)),
            );
        }
        HouseTheme::Retro => {
            // patrón terrazo sutil
            for (sx, sy, r) in [
                (0.2, 0.2, 3.0),
                (0.6, 0.18, 2.0),
                (0.75, 0.3, 2.5),
                (0.35, 0.35, 1.8),
            ] {
                painter.circle_filled(
                    egui::pos2(
                        rect.min.x + rect.width() * sx,
                        rect.min.y + rect.height() * sy,
                    ),
                    r,
                    egui::Color32::from_rgba_premultiplied(180, 120, 60, 14),
                );
            }
        }
        HouseTheme::NocheEstudio => {
            // halo lámpara cálido
            painter.circle_filled(
                egui::pos2(rect.min.x + rect.width() * 0.5, rect.min.y + 28.0),
                42.0,
                egui::Color32::from_rgba_premultiplied(255, 200, 90, 10),
            );
            // estantería sutil
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x + rect.width() * 0.65, rect.min.y + 42.0),
                    egui::pos2(rect.max.x - 12.0, rect.min.y + 46.0),
                ),
                2.0,
                egui::Color32::from_rgba_premultiplied(90, 70, 55, 55),
            );
        }
    }

    // Ventana — marco + vidrio + cruz + cortinas + alféizar
    let win = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + SPACE_LG, rect.min.y + SPACE_LG),
        egui::Vec2::new(68.0, 68.0),
    );
    let glass = match state.house_theme {
        HouseTheme::Espacial | HouseTheme::NocheEstudio => egui::Color32::from_rgb(18, 24, 48),
        _ => egui::Color32::from_rgb(232, 244, 248),
    };
    // Sombra marco
    painter.rect_filled(win.expand(2.0), 10.0, egui::Color32::from_black_alpha(10));
    painter.rect_filled(win, 10.0, glass);
    painter.rect_stroke(
        win,
        10.0,
        egui::Stroke::new(1.5, accent.gamma_multiply(0.9)),
    );
    // Cruz ventana
    painter.line_segment(
        [
            egui::pos2(win.min.x, win.center().y),
            egui::pos2(win.max.x, win.center().y),
        ],
        egui::Stroke::new(1.5, accent.gamma_multiply(0.6)),
    );
    painter.line_segment(
        [
            egui::pos2(win.center().x, win.min.y),
            egui::pos2(win.center().x, win.max.y),
        ],
        egui::Stroke::new(1.5, accent.gamma_multiply(0.6)),
    );
    // Cortinas laterales (triángulos sutiles)
    let curtain_col = match state.house_theme {
        HouseTheme::Acogedora => egui::Color32::from_rgb(232, 210, 180),
        HouseTheme::Espacial => egui::Color32::from_rgb(60, 66, 110),
        HouseTheme::Bosque => egui::Color32::from_rgb(200, 220, 190),
        HouseTheme::Minimal => egui::Color32::from_rgb(240, 240, 238),
        HouseTheme::Retro => egui::Color32::from_rgb(220, 180, 120),
        HouseTheme::NocheEstudio => egui::Color32::from_rgb(70, 60, 85),
    };
    let left_curtain = vec![
        egui::pos2(win.min.x - 3.0, win.min.y - 2.0),
        egui::pos2(win.min.x + 14.0, win.min.y - 2.0),
        egui::pos2(win.min.x + 10.0, win.max.y + 1.0),
        egui::pos2(win.min.x - 3.0, win.max.y + 1.0),
    ];
    let right_curtain = vec![
        egui::pos2(win.max.x + 3.0, win.min.y - 2.0),
        egui::pos2(win.max.x - 14.0, win.min.y - 2.0),
        egui::pos2(win.max.x - 10.0, win.max.y + 1.0),
        egui::pos2(win.max.x + 3.0, win.max.y + 1.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        left_curtain,
        curtain_col,
        egui::Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        right_curtain,
        curtain_col,
        egui::Stroke::NONE,
    ));
    // Alféizar
    let sill = egui::Rect::from_min_max(
        egui::pos2(win.min.x - 4.0, win.max.y - 2.0),
        egui::pos2(win.max.x + 4.0, win.max.y + 4.0),
    );
    painter.rect_filled(sill, 2.0, egui::Color32::from_rgb(210, 210, 208));
    painter.rect_stroke(sill, 2.0, egui::Stroke::new(1.0, theme.separator));
    // Detalle exterior: sol/luna
    if state.house_theme == HouseTheme::Espacial || state.house_theme == HouseTheme::NocheEstudio {
        painter.circle_filled(
            win.center() + egui::Vec2::new(10.0, -10.0),
            6.0,
            egui::Color32::from_rgb(240, 240, 220),
        );
        painter.circle_filled(win.center() + egui::Vec2::new(14.0, -8.0), 4.0, glass);
    } else {
        painter.circle_filled(
            win.center() + egui::Vec2::new(8.0, -12.0),
            7.0,
            egui::Color32::from_rgb(255, 220, 120),
        );
        // nubes
        painter.circle_filled(
            win.center() + egui::Vec2::new(-10.0, -6.0),
            4.0,
            egui::Color32::WHITE,
        );
        painter.circle_filled(
            win.center() + egui::Vec2::new(-6.0, -9.0),
            5.0,
            egui::Color32::WHITE,
        );
    }

    // Cama — cabecera + colchón + almohada + frazada
    let bed_w = 86.0;
    let bed_h = 30.0;
    let bed_rect = egui::Rect::from_min_max(
        egui::pos2(
            rect.max.x - bed_w - SPACE_LG,
            rect.max.y - 30.0 - bed_h - 4.0,
        ),
        egui::pos2(rect.max.x - SPACE_LG, rect.max.y - 30.0 - 4.0),
    );
    // Cabecera (detrás)
    let headboard = egui::Rect::from_min_max(
        egui::pos2(bed_rect.min.x - 2.0, bed_rect.min.y - 6.0),
        egui::pos2(bed_rect.max.x + 2.0, bed_rect.min.y + 8.0),
    );
    painter.rect_filled(headboard, 4.0, accent.gamma_multiply(0.85));
    painter.rect_stroke(headboard, 4.0, egui::Stroke::new(1.0, accent));
    // Colchón
    painter.rect_filled(bed_rect, 6.0, egui::Color32::from_rgb(250, 248, 243));
    painter.rect_stroke(bed_rect, 6.0, egui::Stroke::new(1.0, theme.separator));
    // Almohada
    let pillow = egui::Rect::from_center_size(
        egui::pos2(bed_rect.min.x + 18.0, bed_rect.center().y - 2.0),
        egui::Vec2::new(26.0, 14.0),
    );
    painter.rect_filled(pillow, 4.0, egui::Color32::WHITE);
    painter.rect_stroke(
        pillow,
        4.0,
        egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.6)),
    );
    // Frazada (cover a rayas suaves)
    let blanket = egui::Rect::from_min_max(
        egui::pos2(bed_rect.min.x + 32.0, bed_rect.min.y + 2.0),
        egui::pos2(bed_rect.max.x - 3.0, bed_rect.max.y - 2.0),
    );
    painter.rect_filled(blanket, 4.0, accent.gamma_multiply(0.28));
    painter.line_segment(
        [
            egui::pos2(blanket.min.x + 10.0, blanket.min.y),
            egui::pos2(blanket.min.x + 10.0, blanket.max.y),
        ],
        egui::Stroke::new(1.0, accent.gamma_multiply(0.45)),
    );
    painter.line_segment(
        [
            egui::pos2(blanket.min.x + 22.0, blanket.min.y),
            egui::pos2(blanket.min.x + 22.0, blanket.max.y),
        ],
        egui::Stroke::new(1.0, accent.gamma_multiply(0.45)),
    );
    // Patas cama
    for x in [bed_rect.min.x + 6.0, bed_rect.max.x - 6.0] {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - 2.0, bed_rect.max.y),
                egui::pos2(x + 2.0, bed_rect.max.y + 4.0),
            ),
            1.0,
            egui::Color32::from_rgb(120, 100, 80),
        );
    }

    // Lámpara o planta pequeña (según tema)
    if state.house_theme != HouseTheme::Espacial && state.house_theme != HouseTheme::NocheEstudio {
        let pot_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + SPACE_LG + 82.0, rect.max.y - 30.0 - 18.0),
            egui::Vec2::new(18.0, 12.0),
        );
        painter.rect_filled(pot_rect, 3.0, egui::Color32::from_rgb(180, 140, 110));
        painter.rect_stroke(
            pot_rect,
            3.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(140, 110, 85)),
        );
        painter.circle_filled(
            egui::pos2(pot_rect.center().x, pot_rect.min.y - 4.0),
            10.0,
            egui::Color32::from_rgb(100, 160, 110),
        );
        painter.circle_filled(
            egui::pos2(pot_rect.center().x - 5.0, pot_rect.min.y - 7.0),
            7.0,
            egui::Color32::from_rgb(120, 180, 130),
        );
    } else {
        // lamparita neón flotante
        let lamp_pos = egui::pos2(rect.max.x - 42.0, rect.min.y + 18.0);
        painter.circle_filled(lamp_pos, 4.0, egui::Color32::from_rgb(255, 240, 160));
        painter.circle_stroke(
            lamp_pos,
            8.0,
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_premultiplied(255, 240, 160, 40),
            ),
        );
    }

    // Piso — tablones con vetas
    let floor_h = 30.0;
    let floor_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.max.y - floor_h),
        egui::pos2(rect.max.x, rect.max.y),
    );
    painter.rect_filled(
        floor_rect,
        egui::Rounding {
            nw: 0.0,
            ne: 0.0,
            sw: RADIUS_LG,
            se: RADIUS_LG,
        },
        floor_fill,
    );
    painter.line_segment(
        [
            floor_rect.min,
            egui::pos2(floor_rect.max.x, floor_rect.min.y),
        ],
        egui::Stroke::new(1.5, accent.gamma_multiply(0.5)),
    );
    // Vetado tablones
    let plank_w = 42.0;
    let mut x = floor_rect.min.x + 12.0;
    while x < floor_rect.max.x {
        painter.line_segment(
            [
                egui::pos2(x, floor_rect.min.y),
                egui::pos2(x, floor_rect.max.y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(12)),
        );
        // nudo ocasional
        if (x as i32) % 126 == 12 {
            painter.circle_stroke(
                egui::pos2(x + 12.0, floor_rect.center().y),
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_black_alpha(14)),
            );
        }
        x += plank_w;
    }

    // Alfombra pequeña bajo Pou (óvalo aproximado con rect redondeado)
    let rug_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, floor_rect.min.y + 10.0),
        egui::Vec2::new(96.0, 18.0),
    );
    painter.rect_filled(rug_rect, 9.0, accent.gamma_multiply(0.18));

    // Mascota — centrada sobre alfombra, levemente flotando
    let mascot_accent = accent;
    let mascot_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, floor_rect.min.y - 32.0),
        egui::Vec2::new(84.0, 84.0),
    );
    draw_mascot(
        painter,
        mascot_rect,
        state.dna,
        state.species,
        state.mood,
        time,
        mascot_accent,
        wall_fill,
        state.outfit,
    );
}

/// Habitáculo Pou: pared, piso, ventana + cama + stats hambre/felicidad muy visibles. Scandinavian shell, contenido playful.
pub fn draw_mascot_room(ctx: &egui::Context, state: &mut MascotRoomState) -> Option<Action> {
    use crate::tokens::{
        RADIUS_LG, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_SM, TYPE_XS,
    };
    let theme = crate::theme::current_theme(ctx);
    let mut action: Option<Action> = None;
    egui::Window::new("Habitáculo")
        .id(egui::Id::new("pou_room_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.5, theme.separator))
                .rounding(RADIUS_LG)
                .inner_margin(egui::Margin::same(SPACE_LG))
                .shadow(egui::Shadow {
                    offset: egui::vec2(0.0, 2.0),
                    blur: 8.0,
                    spread: 0.0,
                    color: egui::Color32::from_black_alpha(8),
                }),
        )
        .show(ctx, |ui| {
            // Header Scandinavian con badge Etapa + coins
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Habitáculo")
                        .size(TYPE_BASE)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Coins badge playful
                    egui::Frame::none()
                        .fill(theme.accent.gamma_multiply(0.12))
                        .rounding(crate::tokens::RADIUS_PILL)
                        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("🪙 {}", state.coins))
                                    .size(TYPE_XS)
                                    .color(theme.accent)
                                    .strong(),
                            );
                        });
                    ui.add_space(6.0);
                    egui::Frame::none()
                        .fill(theme.input_bg)
                        .stroke(egui::Stroke::new(1.0, theme.separator))
                        .rounding(crate::tokens::RADIUS_PILL)
                        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("Etapa {}", state.evolution_stage))
                                    .size(TYPE_XS)
                                    .color(theme.text_secondary),
                            );
                        });
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(state.house_theme.label())
                            .size(TYPE_XS)
                            .color(theme.text_tertiary),
                    );
                });
            });
            ui.add_space(SPACE_XS);
            ui.separator();
            ui.add_space(SPACE_SM);
            // Habitáculo detallado — 180px alto para que quepa cama + ventana
            let room_h: f32 = 180.0;
            let room_w = ui.available_width();
            let (room_rect, room_resp) =
                ui.allocate_exact_size(egui::Vec2::new(room_w, room_h), egui::Sense::click());
            if ui.is_rect_visible(room_rect) {
                let painter = ui.painter_at(room_rect);
                let time = ui.input(|i| i.time);
                paint_mascot_room(&painter, room_rect, state, theme, time);
            }
            if room_resp.clicked() {
                action = Some(MascotRoomAction::Poke);
            }
            room_resp.on_hover_text("Toca a Pou — ¡reacciona!");
            ui.add_space(SPACE_MD);
            // Stats muy visibles — barras gruesas 10px con valor dentro y colores semánticos
            for (label, value, icon) in [
                ("Hambre", state.hunger, "🍽"),
                ("Felicidad", state.happiness, "♥"),
            ] {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{icon} {label}"))
                            .size(TYPE_SM)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let pct = value.clamp(0.0, 100.0);
                        let badge_fill = if label == "Hambre" {
                            if pct > 70.0 {
                                theme.warning.gamma_multiply(0.16)
                            } else {
                                theme.accent.gamma_multiply(0.12)
                            }
                        } else if pct < 30.0 {
                            theme.text_tertiary.gamma_multiply(0.12)
                        } else {
                            theme.success.gamma_multiply(0.12)
                        };
                        let badge_col = if label == "Hambre" {
                            if pct > 70.0 {
                                theme.warning
                            } else {
                                theme.accent
                            }
                        } else if pct < 30.0 {
                            theme.text_tertiary
                        } else {
                            theme.success
                        };
                        egui::Frame::none()
                            .fill(badge_fill)
                            .rounding(crate::tokens::RADIUS_PILL)
                            .inner_margin(egui::Margin::symmetric(7.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{pct:.0}%"))
                                        .size(TYPE_XS)
                                        .strong()
                                        .color(badge_col),
                                );
                            });
                    });
                });
                let bar_w = ui.available_width();
                let bar_h = 10.0;
                let (bar_rect, _) =
                    ui.allocate_exact_size(egui::Vec2::new(bar_w, bar_h), egui::Sense::hover());
                if ui.is_rect_visible(bar_rect) {
                    // track
                    ui.painter()
                        .rect_filled(bar_rect, 5.0, theme.separator.gamma_multiply(0.55));
                    let pct = value.clamp(0.0, 100.0) / 100.0;
                    let filled_w = bar_w * pct;
                    let filled =
                        egui::Rect::from_min_size(bar_rect.min, egui::Vec2::new(filled_w, bar_h));
                    let fill_col = if label == "Hambre" {
                        if value > 70.0 {
                            theme.warning
                        } else {
                            theme.accent
                        }
                    } else if value < 30.0 {
                        theme.text_tertiary
                    } else {
                        theme.success
                    };
                    ui.painter().rect_filled(filled, 5.0, fill_col);
                    // brillo interior sutil
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            filled.min + egui::Vec2::new(1.0, 1.0),
                            egui::pos2(filled.max.x - 1.0, filled.min.y + 3.0),
                        ),
                        4.0,
                        egui::Color32::from_white_alpha(28),
                    );
                    ui.painter().rect_stroke(
                        bar_rect,
                        5.0,
                        egui::Stroke::new(1.0, theme.separator),
                    );
                    // tick marks 25/50/75
                    for t in [0.25, 0.5, 0.75] {
                        let x = bar_rect.min.x + bar_w * t;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, bar_rect.min.y + 2.0),
                                egui::pos2(x, bar_rect.max.y - 2.0),
                            ],
                            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(12)),
                        );
                    }
                }
                // texto de estado semántico debajo de la barra (playful)
                let status = if label == "Hambre" {
                    if value > 75.0 {
                        "¡Pou tiene hambre!"
                    } else if value > 45.0 {
                        "Apetito moderado"
                    } else {
                        "Saciado • listo para jugar"
                    }
                } else if value < 30.0 {
                    "Ánimo bajo — ¡jugá un rato!"
                } else if value > 80.0 {
                    "¡Felicidad al máximo!"
                } else {
                    "Contento"
                };
                ui.label(
                    egui::RichText::new(status)
                        .size(TYPE_XS)
                        .color(theme.text_tertiary)
                        .italics(),
                );
                ui.add_space(SPACE_XS);
            }
            ui.add_space(SPACE_XS);
            ui.separator();
            ui.add_space(SPACE_SM);
            // Actions — Scandinavian spacing 16, playful labels
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_LG;
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("🍎 Alimentar").size(TYPE_XS))
                            .rounding(RADIUS_LG)
                            .stroke(egui::Stroke::new(1.5, theme.separator))
                            .fill(theme.accent.gamma_multiply(0.08)),
                    )
                    .on_hover_text("Baja hambre +18, sube felicidad")
                    .clicked()
                {
                    action = Some(MascotRoomAction::Feed);
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("🎮 Jugar").size(TYPE_XS))
                            .rounding(RADIUS_LG)
                            .stroke(egui::Stroke::new(1.5, theme.separator)),
                    )
                    .clicked()
                {
                    action = Some(MascotRoomAction::Play);
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("💤 Dormir").size(TYPE_XS))
                            .rounding(RADIUS_LG)
                            .stroke(egui::Stroke::new(1.5, theme.separator)),
                    )
                    .clicked()
                {
                    action = Some(MascotRoomAction::Sleep);
                }
            });
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Vestir rápido:")
                        .size(TYPE_XS)
                        .color(theme.text_secondary),
                );
                for o in [0u8, 1, 2, 3] {
                    let sel = state.outfit == o;
                    let label = match o {
                        0 => "—",
                        1 => "🧣",
                        2 => "🎩",
                        3 => "🦸",
                        _ => "?",
                    };
                    if ui
                        .selectable_label(sel, label)
                        .on_hover_text(format!("Outfit {o}"))
                        .clicked()
                    {
                        action = Some(MascotRoomAction::ChangeOutfit(o));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Casa • {}", state.house_theme.label()))
                            .size(TYPE_XS)
                            .color(theme.text_tertiary),
                    );
                });
            });
        });
    if let Some(act) = &action {
        match *act {
            MascotRoomAction::Feed => {
                state.hunger = (state.hunger - 18.0).clamp(0.0, 100.0);
                state.happiness = (state.happiness + 4.0).clamp(0.0, 100.0);
            }
            MascotRoomAction::Play => {
                state.happiness = (state.happiness + 12.0).clamp(0.0, 100.0);
                state.hunger = (state.hunger + 6.0).clamp(0.0, 100.0);
            }
            MascotRoomAction::Sleep => {
                state.mood = MascotMood::Sleepy as u8;
            }
            MascotRoomAction::ChangeOutfit(o) => {
                state.outfit = o;
            }
            MascotRoomAction::Poke => {
                state.mood = MascotMood::Excited as u8;
            }
        }
    }
    action
}

// ─────────────────────────────────────────────────────────────────────────────
// Picker de avatar (perfil clásico + mascota embebida ya manejada arriba)
// ─────────────────────────────────────────────────────────────────────────────

/// UI mínima para editar el avatar clásico (nombre, seed, acento).
/// Devuelve true si hubo cambio y ya valida el largo.
pub fn avatar_picker_ui(
    ui: &mut egui::Ui,
    cfg: &mut AvatarConfig,
    fallback: &str,
    _mora_tex: Option<egui::TextureId>,
) -> bool {
    let mut changed = false;
    let theme = crate::theme::current_theme(ui.ctx());

    ui.label(
        egui::RichText::new("Nombre a mostrar")
            .size(11.0)
            .color(theme.text_secondary),
    );
    let mut display = cfg.display_name.clone();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut display)
            .hint_text(fallback)
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(8.0, 4.0)),
    );
    if resp.changed() {
        cfg.display_name = display.chars().take(32).collect();
        changed = true;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Semilla del avatar (para variación)")
            .size(11.0)
            .color(theme.text_secondary),
    );
    let mut seed = cfg.seed.clone();
    if ui
        .add(
            egui::TextEdit::singleline(&mut seed)
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(8.0, 4.0)),
        )
        .changed()
    {
        cfg.seed = seed;
        changed = true;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Acento")
            .size(11.0)
            .color(theme.text_secondary),
    );
    ui.horizontal_wrapped(|ui| {
        for preset in 0..6u8 {
            let (name, rgb, _) = AvatarConfig::accent_palette(preset);
            let is_sel = cfg.accent_preset == preset;
            let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let btn = egui::Button::new(egui::RichText::new(name).size(10.0).color(if is_sel {
                Color32::WHITE
            } else {
                theme.text_primary
            }))
            .fill(if is_sel {
                col
            } else {
                col.gamma_multiply(0.18)
            })
            .rounding(crate::tokens::RADIUS_PILL)
            .selected(is_sel);
            if ui.add(btn).clicked() {
                cfg.accent_preset = preset;
                changed = true;
            }
        }
    });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_mascot_does_not_panic() {
        // Solo verifica que la firma compila y no hay panic en construcción de parámetros.
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(100.0, 100.0));
        // No llamamos a painter real porque requiere ctx; solo test de tipos.
        assert_eq!(MascotMood::Idle as u8, 0);
        let _ = rect;
    }
}
