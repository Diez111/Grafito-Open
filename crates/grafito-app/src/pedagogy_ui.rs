//! Panel pedagógico — ejercicios y devolución socrática (sin bloquear la UI).
//!
//! Piel pura: `fn render(&Estado) -> Frame`. Sin I/O ni spawn en `Ui::`:
//! - `draw_pedagogy_panel` y `draw_tarjeta_ejercicio` sólo leen `&Estado` y
//!   llaman motores puros en memoria (`ExerciseGenerator`, `FeedbackEngine`,
//!   `ScaffoldEngine`). Sin `std::fs`, sin `std::net`, sin `Command`, sin
//!   `thread::spawn` ni `tokio::spawn` en ningún `draw_*`.
//! - Todo texto visible en rioplatense (voseo: mirá, probá, elegí).
//! - Layout acotado: `wrap` + `clip` + recorte por caracteres; sin
//!   `ScrollArea` gigante (el panel no envuelve al compositor).
//!
//! NOTA: este archivo aún no está cableado como `mod` en `lib.rs` (huérfano).
//! Se mantiene autocontenido y con `cargo fmt` limpio; los helpers puros
//! (`acotar_texto`, `opciones_para_ejercicio`) duplican a propósito los de
//! `teaching_ui.rs` hasta que el DAG permita compartirlos sin romper
//! el ownership por archivo.

use egui::{Color32, Stroke};
use grafito_pedagogy::curriculum::{Curriculum, LearningObjective};
use grafito_pedagogy::{
    Exercise, ExerciseGenerator, FeedbackEngine, PedagogicalLevel, ScaffoldEngine,
};
use grafito_ui::tokens::{
    RADIUS_MD, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BASE, TYPE_MD, TYPE_SM, TYPE_XS,
};

/// Tope de caracteres del enunciado para que la tarjeta no desborde.
pub const MAX_ENUNCIADO_CHARS: usize = 280;
/// Tope de caracteres por opción múltiple.
pub const MAX_OPCION_CHARS: usize = 120;
/// Tope del campo de respuesta libre.
pub const MAX_RESPUESTA_CHARS: usize = 200;
/// Alto máximo del bloque de andamiaje antes de recortar (sin scroll gigante).
pub const MAX_ANDAMIAJE_CHARS: usize = 600;

#[derive(Debug, Clone, Default)]
pub struct PedagogyPanelState {
    pub concept: String,
    pub level: PedagogicalLevel,
    pub exercise: Option<Exercise>,
    pub answer: String,
    pub feedback: Option<grafito_pedagogy::Feedback>,
    pub scaffold: Option<grafito_pedagogy::Scaffold>,
    /// Índice elegido en las opciones múltiples (si hay).
    pub opcion_elegida: Option<usize>,
    /// Opciones múltiples generadas junto al ejercicio.
    pub opciones: Vec<String>,
}

/// Recorta por borde de carácter (nunca parte UTF-8) y agrega "…" si recorta.
///
/// Puro, sin I/O.
pub fn acotar_texto(texto: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if texto.chars().count() <= max {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(max.saturating_sub(1)).collect();
    format!("{recortado}…")
}

/// Intento simple de número (incluye fracción `a/b` y coma decimal).
///
/// Puro, sin I/O. Devuelve `None` si no es numérico simple.
fn numero_simple(texto: &str) -> Option<f64> {
    let t = texto.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    if let Some((a, b)) = t.split_once('/') {
        let a: f64 = a.trim().parse().ok()?;
        let b: f64 = b.trim().parse().ok()?;
        if b.abs() > 1e-12 {
            return Some(a / b);
        }
        return None;
    }
    t.parse::<f64>().ok()
}

fn formatear_numero(valor: f64) -> String {
    if (valor - valor.round()).abs() < 1e-9 && valor.abs() < 1e9 {
        format!("{}", valor.round() as i64)
    } else {
        format!("{valor:.2}")
    }
}

/// Opciones múltiples deterministas para un ejercicio (4, con la solución incluida).
///
/// Puro, sin I/O: los distractores se derivan de la solución y la semilla del
/// ejercicio rota la posición de la correcta para que no quede siempre primera.
pub fn opciones_para_ejercicio(ejercicio: &Exercise) -> Vec<String> {
    let solucion = ejercicio.solution.trim().to_string();
    let mut candidatas: Vec<String> = Vec::with_capacity(4);
    candidatas.push(solucion.clone());
    if let Some(n) = numero_simple(&solucion) {
        for d in [1.0, -1.0, 2.0] {
            let v = if d == 2.0 { n * 2.0 } else { n + d };
            let s = formatear_numero(v);
            if !candidatas.iter().any(|c| c == &s) {
                candidatas.push(s);
            }
            if candidatas.len() >= 4 {
                break;
            }
        }
        // Si la solución es 0, n*2 repite 0: completamos con 1, 2, -1.
        for extra in ["1", "2", "-1", "10"] {
            if candidatas.len() >= 4 {
                break;
            }
            if !candidatas.iter().any(|c| c == extra) {
                candidatas.push(extra.to_string());
            }
        }
    } else {
        for suf in [" + 1", " − 1", " (otra forma)"] {
            if candidatas.len() >= 4 {
                break;
            }
            let s = format!("{solucion}{suf}");
            if !candidatas.iter().any(|c| c == &s) {
                candidatas.push(s);
            }
        }
        if !candidatas.iter().any(|c| c == "Ninguna de estas") {
            candidatas.push("Ninguna de estas".to_string());
        }
    }
    candidatas.truncate(4);
    while candidatas.len() < 4 {
        let relleno = format!("Variante {}", candidatas.len() + 1);
        candidatas.push(relleno);
    }
    // Rotación determinista por semilla para no regalar la posición.
    let rot = (ejercicio.seed.unwrap_or(0) % 4) as usize;
    candidatas.rotate_left(rot);
    candidatas
}

/// Índice de la solución dentro de `opciones` (si está).
///
/// Puro, sin I/O.
pub fn indice_respuesta_correcta(ejercicio: &Exercise, opciones: &[String]) -> Option<usize> {
    let sol = ejercicio.solution.trim();
    opciones.iter().position(|o| o.trim() == sol)
}

/// Tarjeta de ejercicio inline: enunciado + opciones + devolución tras responder.
///
/// Layout acotado: ancho del panel, `wrap` en cada texto, recorte por
/// caracteres. No crea `ScrollArea`; el llamador decide el scroll externo.
/// Sin I/O ni spawn: sólo evalúa con `FeedbackEngine` (CPU en memoria).
pub fn draw_tarjeta_ejercicio(
    ui: &mut egui::Ui,
    ejercicio: &Exercise,
    opciones: &[String],
    respuesta: &mut String,
    opcion_elegida: &mut Option<usize>,
    devolucion: &mut Option<grafito_pedagogy::Feedback>,
) {
    let tema = grafito_ui::theme::current_theme(ui.ctx());
    egui::Frame::none()
        .fill(tema.panel_bg)
        .stroke(Stroke::new(1.0, tema.separator.gamma_multiply(0.10)))
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_MD))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Enunciado acotado con wrap: nunca desborda.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(acotar_texto(&ejercicio.prompt, MAX_ENUNCIADO_CHARS))
                        .strong()
                        .size(TYPE_BASE)
                        .color(tema.text_primary),
                )
                .wrap(),
            );
            ui.add_space(SPACE_XS);
            // Opciones múltiples (radio) con wrap.
            for (idx, op) in opciones.iter().enumerate() {
                let texto = acotar_texto(op, MAX_OPCION_CHARS);
                if ui.radio_value(opcion_elegida, Some(idx), texto).clicked() {
                    *respuesta = op.clone();
                    *devolucion = None;
                }
            }
            ui.add_space(SPACE_SM);
            // Respuesta libre (acotada) + botón para responder.
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Tu respuesta:")
                        .size(TYPE_SM)
                        .color(tema.text_secondary),
                );
                let edit = ui.text_edit_singleline(respuesta);
                if edit.changed() && respuesta.chars().count() > MAX_RESPUESTA_CHARS {
                    *respuesta = acotar_texto(respuesta, MAX_RESPUESTA_CHARS);
                    *opcion_elegida = None;
                }
                let boton =
                    egui::Button::new(egui::RichText::new("Responder").size(TYPE_SM).strong())
                        .fill(tema.accent)
                        .stroke(Stroke::NONE)
                        .rounding(RADIUS_MD);
                if ui.add(boton).clicked() {
                    let fb = FeedbackEngine.assess(ejercicio, respuesta);
                    *devolucion = Some(fb);
                }
            });
            // Devolución tras responder: marco acotado con wrap.
            if let Some(fb) = devolucion.as_ref() {
                ui.add_space(SPACE_SM);
                let (borde, fondo) = if fb.correct {
                    (
                        Color32::from_rgb(40, 180, 70),
                        Color32::from_rgb(232, 245, 233),
                    )
                } else {
                    (
                        Color32::from_rgb(200, 60, 60),
                        Color32::from_rgb(253, 237, 237),
                    )
                };
                egui::Frame::none()
                    .fill(fondo)
                    .stroke(Stroke::new(1.0, borde))
                    .rounding(RADIUS_MD)
                    .inner_margin(egui::Margin::same(SPACE_SM))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let titulo = if fb.correct {
                            "¡Bien ahí, che! Respuesta correcta."
                        } else {
                            "Casi… fijate de nuevo."
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(titulo)
                                    .strong()
                                    .size(TYPE_SM)
                                    .color(borde),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(acotar_texto(&fb.message, MAX_ENUNCIADO_CHARS))
                                    .size(TYPE_SM)
                                    .color(Color32::from_rgb(40, 40, 40)),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(acotar_texto(
                                    &fb.next_step,
                                    MAX_ENUNCIADO_CHARS,
                                ))
                                .size(TYPE_XS)
                                .color(Color32::from_rgb(90, 90, 90)),
                            )
                            .wrap(),
                        );
                    });
            }
        });
}

/// Panel del tutor: concepto + nivel + andamiaje + tarjeta de ejercicio.
///
/// Sin I/O ni spawn en `Ui::`: genera y evalúa sólo con motores puros.
/// Textos en rioplatense; sin `ScrollArea` envolvente gigante.
pub fn draw_pedagogy_panel(ui: &mut egui::Ui, state: &mut PedagogyPanelState) {
    let tema = grafito_ui::theme::current_theme(ui.ctx());
    ui.add(
        egui::Label::new(
            egui::RichText::new("Tutor pedagógico")
                .strong()
                .size(TYPE_MD)
                .color(tema.text_primary),
        )
        .wrap(),
    );
    ui.add_space(SPACE_XS);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_SM;
        ui.label(
            egui::RichText::new("Concepto:")
                .size(TYPE_SM)
                .color(tema.text_secondary),
        );
        ui.text_edit_singleline(&mut state.concept);
        if ui.button("Preguntar").clicked() {
            let motor = ScaffoldEngine;
            state.scaffold = Some(motor.scaffold(&state.concept, state.level, &[]));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_SM;
        ui.label(
            egui::RichText::new("Nivel:")
                .size(TYPE_SM)
                .color(tema.text_secondary),
        );
        egui::ComboBox::from_id_salt("pedagogy_level")
            .selected_text(state.level.label())
            .show_ui(ui, |ui| {
                for lvl in [
                    PedagogicalLevel::Primary,
                    PedagogicalLevel::Secondary,
                    PedagogicalLevel::University,
                ] {
                    ui.selectable_value(&mut state.level, lvl, lvl.label());
                }
            });
    });
    if let Some(andamiaje) = &state.scaffold {
        ui.add_space(SPACE_SM);
        egui::CollapsingHeader::new(
            egui::RichText::new("Paso a paso socrático")
                .size(TYPE_SM)
                .color(tema.text_secondary),
        )
        .id_salt("andamiaje_socratico")
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(acotar_texto(&andamiaje.question, MAX_ANDAMIAJE_CHARS))
                        .strong()
                        .size(TYPE_BASE)
                        .color(tema.text_primary),
                )
                .wrap(),
            );
            if let Some(pista) = &andamiaje.hint {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "Pista: {}",
                            acotar_texto(pista, MAX_ENUNCIADO_CHARS)
                        ))
                        .size(TYPE_SM)
                        .color(tema.text_secondary),
                    )
                    .wrap(),
                );
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(acotar_texto(&andamiaje.explanation, MAX_ANDAMIAJE_CHARS))
                        .size(TYPE_SM)
                        .color(tema.text_primary),
                )
                .wrap(),
            );
        });
    }
    ui.add_space(SPACE_SM);
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_SM;
        if ui.button("Armar ejercicio").clicked() {
            let objetivo = Curriculum::find_for_concept(&state.concept)
                .into_iter()
                .next()
                .unwrap_or_else(|| {
                    LearningObjective::new("custom", &state.concept, &state.concept, None)
                });
            let ejercicio = ExerciseGenerator.generate(&objetivo, state.level);
            state.opciones = opciones_para_ejercicio(&ejercicio);
            state.exercise = Some(ejercicio);
            state.answer.clear();
            state.opcion_elegida = None;
            state.feedback = None;
        }
        if ui.button("Empezar de nuevo").clicked() {
            state.exercise = None;
            state.opciones.clear();
            state.opcion_elegida = None;
            state.feedback = None;
        }
    });
    if let Some(ejercicio) = state.exercise.clone() {
        ui.add_space(SPACE_SM);
        draw_tarjeta_ejercicio(
            ui,
            &ejercicio,
            &state.opciones.clone(),
            &mut state.answer,
            &mut state.opcion_elegida,
            &mut state.feedback,
        );
    } else {
        ui.add_space(SPACE_XS);
        ui.add(
            egui::Label::new(
                egui::RichText::new("Tocá «Armar ejercicio» y practicamos juntos, che.")
                    .size(TYPE_SM)
                    .color(tema.text_secondary),
            )
            .wrap(),
        );
    }
    ui.add_space(SPACE_LG);
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "Consejo: el nivel se sincroniza con tu racha y tus puntos y con el concepto del lienzo.",
            )
            .size(TYPE_XS)
            .color(tema.text_secondary),
        )
        .wrap(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acotar_no_rompe_utf8() {
        let s = acotar_texto("áéíóúñ", 4);
        assert!(s.contains('…'));
        assert!(s.chars().count() <= 4);
    }

    #[test]
    fn opciones_incluyen_solucion_y_son_cuatro() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let ex = ExerciseGenerator.generate(&lo, PedagogicalLevel::Secondary);
        let ops = opciones_para_ejercicio(&ex);
        assert_eq!(ops.len(), 4);
        assert!(ops.iter().any(|o| o.trim() == ex.solution.trim()));
        let mut unicas = ops.clone();
        unicas.sort();
        unicas.dedup();
        assert_eq!(unicas.len(), 4);
    }

    #[test]
    fn indice_correcto_encuentra_solucion() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let ex = ExerciseGenerator.generate(&lo, PedagogicalLevel::Secondary);
        let ops = opciones_para_ejercicio(&ex);
        assert!(indice_respuesta_correcta(&ex, &ops).is_some());
    }
}
