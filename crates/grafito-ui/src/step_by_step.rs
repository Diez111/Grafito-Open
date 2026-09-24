//! Visor de desarrollo por pasos — tarjeta del transcript del asistente.
//!
//! Superficie de UI para el motor `CasStepper` (`grafito-geometry::cas_steps`):
//! el comando `StepByStep[op]` ya devuelve la traza como texto y esta tarjeta
//! la muestra con revelado progresivo (`revealed` pasos visibles de `total`,
//! acotado a `MAX_CAS_STEPS` 32).
//!
//! - Piel pura: `fn render(&Estado) -> Frame`. Cero I/O, cero spawn en el
//!   dibujado. Los botones emiten [`AssistantUiAction`] y la app muta el
//!   contador fuera del draw.
//! - Fuente de los pasos: bloques ```grafito-steps del transcript (mismo
//!   patrón que `AssistantExerciseCard::to_code_block` / `from_code_block`).
//! - Tokens de [`crate::tokens`] como única fuente de verdad; i18n vía
//!   [`crate::i18n::t`]; animación de aparición gobernada por
//!   [`MotionConfig`] (`reduced_motion` salta el fade).

use crate::assistant::{draw_math, AssistantUiAction};
use crate::i18n::{t, Locale};
use crate::projector::MotionConfig;
use crate::tokens::{
    hit_target_size, HIT_TARGET_MIN, RADIUS_MD, SPACE_SM, SPACE_XS, TYPE_SM, TYPE_XS,
};
use grafito_geometry::cas_steps::{CasStep, MAX_CAS_STEPS, MAX_STEP_BYTES};

/// Lenguaje del fence que el transcript reconoce como visor de pasos.
pub const STEPS_FENCE_LANGUAGE: &str = "grafito-steps";

/// Apertura exacta del fence (primera línea).
const STEPS_FENCE_LANGUAGE_FENCE: &str = "```grafito-steps";

/// Un paso pedagógico serializable (espejo de `CasStep` sin acoplar el
/// dibujado al tipo del motor: `rule` guarda el `Display` estable de
/// `RewriteRule`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepByStepItem {
    /// Nombre estable de la regla (`PowerRule`, `Simplify`, …).
    pub rule: String,
    /// Expresión antes de aplicar la regla (≤ `MAX_STEP_BYTES`).
    pub before: String,
    /// Expresión después de aplicar la regla (≤ `MAX_STEP_BYTES`).
    pub after: String,
    /// Descripción rioplatense lista para mostrar (≤ `MAX_STEP_BYTES`).
    pub description: String,
}

impl From<&CasStep> for StepByStepItem {
    fn from(step: &CasStep) -> Self {
        Self {
            rule: step.rule.to_string(),
            before: truncate_str(&step.before, MAX_STEP_BYTES),
            after: truncate_str(&step.after, MAX_STEP_BYTES),
            description: truncate_str(&step.description, MAX_STEP_BYTES),
        }
    }
}

/// Estado del visor: pasos + cuántos están revelados.
///
/// `revealed` siempre ≤ `steps.len()` ≤ `MAX_CAS_STEPS`. El panel
/// (`AssistantPanelState::steps_revealed`) conserva el contador por turno;
/// esta struct es el valor efímero que se dibuja cada frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepByStepCardState {
    /// Etiqueta de la operación (`Derivative[x^2, x]`). Puede estar vacía.
    pub title: String,
    /// Pasos acotados a `MAX_CAS_STEPS`.
    pub steps: Vec<StepByStepItem>,
    /// Pasos visibles (revelado progresivo).
    pub revealed: usize,
}

impl StepByStepCardState {
    /// Construye desde la salida del motor. Con pasos no vacíos arranca con
    /// el primero revelado; sin pasos queda vacía (`revealed` 0).
    pub fn from_cas_steps(title: impl Into<String>, steps: &[CasStep]) -> Self {
        let items: Vec<StepByStepItem> = steps
            .iter()
            .take(MAX_CAS_STEPS)
            .map(StepByStepItem::from)
            .collect();
        let revealed = if items.is_empty() { 0 } else { 1 };
        Self {
            title: title.into(),
            steps: items,
            revealed,
        }
    }

    /// Pasos visibles ahora (nunca más que los disponibles).
    pub fn revealed_steps(&self) -> &[StepByStepItem] {
        let end = self.revealed.min(self.steps.len());
        &self.steps[..end]
    }

    /// Avanza un paso, acotado al total. Idempotente en el tope.
    pub fn reveal_next(&mut self) {
        self.revealed = next_revealed(self.revealed, self.steps.len());
    }

    /// Revela todo, acotado a `MAX_CAS_STEPS` vía `steps.len()`.
    pub fn reveal_all(&mut self) {
        self.revealed = self.steps.len();
    }

    /// Contador `revelados/total` (numérico, sin i18n: los dígitos no se
    /// traducen).
    pub fn counter_text(&self) -> String {
        format!(
            "{}/{}",
            self.revealed.min(self.steps.len()),
            self.steps.len()
        )
    }

    /// Serializa a bloque ```grafito-steps (compatible con el renderer de
    /// bloques de código del transcript).
    pub fn to_code_block(&self) -> String {
        let mut out = String::from("```grafito-steps\n");
        if !self.title.trim().is_empty() {
            out.push_str("# ");
            out.push_str(&sanitize_line(&self.title));
            out.push('\n');
        }
        for item in self.steps.iter().take(MAX_CAS_STEPS) {
            out.push_str("- rule: ");
            out.push_str(&sanitize_line(&item.rule));
            out.push('\n');
            out.push_str("  desc: ");
            out.push_str(&sanitize_line(&item.description));
            out.push('\n');
            out.push_str("  before: ");
            out.push_str(&sanitize_line(&item.before));
            out.push('\n');
            out.push_str("  after: ");
            out.push_str(&sanitize_line(&item.after));
            out.push('\n');
        }
        out.push_str("```");
        out
    }

    /// Parsea un bloque ```grafito-steps. Acepta el fence completo o el
    /// cuerpo pelado (el parser del transcript entrega `text` sin las
    /// líneas ```). Trunca a `MAX_CAS_STEPS` y suelta el paso trailing
    /// incompleto (sin `rule`/`before`/`after`). Nunca falla con pánico:
    /// el fence malformado da `None`.
    pub fn from_code_block(text: &str) -> Option<Self> {
        let body = strip_fence(text);
        let mut title = String::new();
        let mut steps = Vec::new();
        let mut current: Option<StepByStepItem> = None;
        let mut invalid = false;
        for raw_line in body.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rule) = line.strip_prefix("- rule:") {
                if steps.len() >= MAX_CAS_STEPS {
                    break;
                }
                if let Some(pending) = current.take() {
                    push_complete(&mut steps, pending);
                }
                current = Some(StepByStepItem {
                    rule: truncate_str(rule.trim(), MAX_STEP_BYTES),
                    before: String::new(),
                    after: String::new(),
                    description: String::new(),
                });
                continue;
            }
            if title.is_empty() {
                if let Some(head) = line.strip_prefix('#') {
                    title = truncate_str(head.trim(), MAX_STEP_BYTES);
                    continue;
                }
            }
            let Some(slot) = current.as_mut() else {
                // Línea fuera de paso: fence ajeno, se aborta a vacía.
                invalid = true;
                break;
            };
            if let Some(value) = line.strip_prefix("desc:") {
                slot.description = truncate_str(value.trim(), MAX_STEP_BYTES);
            } else if let Some(value) = line.strip_prefix("before:") {
                slot.before = truncate_str(value.trim(), MAX_STEP_BYTES);
            } else if let Some(value) = line.strip_prefix("after:") {
                slot.after = truncate_str(value.trim(), MAX_STEP_BYTES);
            } else {
                invalid = true;
                break;
            }
        }
        if invalid {
            return None;
        }
        if let Some(pending) = current.take() {
            push_complete(&mut steps, pending);
        }
        if title.is_empty() && steps.is_empty() {
            return None;
        }
        let revealed = if steps.is_empty() { 0 } else { 1 };
        Some(Self {
            title,
            steps,
            revealed,
        })
    }
}

/// Siguiente valor de revelado, acotado a `total` (puro y testeable; lo usan
/// el modelo y el mapa del panel para no duplicar la cota).
pub fn next_revealed(current: usize, total: usize) -> usize {
    current.saturating_add(1).min(total)
}

/// Cuenta los pasos de todos los fences ```grafito-steps en `content`
/// (0 si no hay ninguno o son inválidos). Lo usa la app para acotar el
/// avance sin re-parsear el transcript a mano.
pub fn count_steps_in_content(content: &str) -> usize {
    let mut total: usize = 0;
    let mut rest = content;
    while let Some(start) = rest.find("```grafito-steps") {
        let fence = &rest[start..];
        let body_end = fence
            .find("\n```")
            .map(|pos| pos + "\n```".len())
            .unwrap_or(fence.len());
        let fence_text = &fence[..body_end.min(fence.len())];
        if let Some(card) = StepByStepCardState::from_code_block(fence_text) {
            total = total.saturating_add(card.steps.len());
        }
        let advance = (start + body_end).min(rest.len());
        if advance == 0 {
            break;
        }
        rest = &rest[advance..];
        if rest.is_empty() {
            break;
        }
    }
    total.min(MAX_CAS_STEPS)
}

/// Trunca a `max_bytes` respetando borde UTF-8 (igual que el motor).
fn truncate_str(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Una línea del fence no puede tener saltos: se colapsan a espacio para
/// que el bloque haga roundtrip.
fn sanitize_line(value: &str) -> String {
    let colapsado = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_str(colapsado.trim(), MAX_STEP_BYTES)
}

/// Quita el fence ```grafito-steps si está (primera línea exacta y cierre
/// final con ```) y devuelve el cuerpo. Sin fences, devuelve el texto tal
/// cual: el parser del transcript ya pela las cercas del bloque `Code`.
fn strip_fence(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines
        .first()
        .is_some_and(|first| first.trim() == STEPS_FENCE_LANGUAGE_FENCE)
    {
        lines.remove(0);
    }
    if lines
        .last()
        .is_some_and(|last| last.trim_start().starts_with("```"))
    {
        lines.pop();
    }
    lines.join("\n")
}

/// Guarda el paso sólo si trae `rule`, `before` y `after` (la descripción
/// puede estar vacía).
fn push_complete(steps: &mut Vec<StepByStepItem>, item: StepByStepItem) {
    if !item.rule.is_empty() && !item.before.is_empty() && !item.after.is_empty() {
        steps.push(item);
    }
}

// ── Dibujado ──

/// Dibuja la tarjeta de pasos en el transcript.
///
/// `card.revealed` ya viene resuelto por el llamador (mapa del panel);
/// los botones emiten la acción y la app avanza fuera del draw. El último
/// paso revelado aparece con fade, salvo `motion.reduced_motion` (se muestra
/// directo, sin interpolación).
pub fn draw_step_by_step_card(
    ui: &mut egui::Ui,
    card: &StepByStepCardState,
    turn_index: usize,
    card_index: usize,
    motion: MotionConfig,
    locale: Locale,
) -> Option<AssistantUiAction> {
    let theme = crate::theme::current_theme(ui.ctx());
    let mut action = None;
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(theme.hairline_stroke())
        .rounding(RADIUS_MD)
        .inner_margin(egui::Margin::same(SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Cabecera: título + contador `i/total`.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if card.title.trim().is_empty() {
                        t("assistant.steps.title", locale)
                    } else {
                        card.title.trim()
                    })
                    .color(theme.text_primary)
                    .size(TYPE_SM)
                    .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let counter = ui.label(
                        egui::RichText::new(card.counter_text())
                            .color(theme.accent_strong)
                            .size(TYPE_XS)
                            .strong(),
                    );
                    crate::toolbar::tag_live_region(
                        &counter,
                        format!(
                            "{} {}",
                            t("assistant.steps.title", locale),
                            card.counter_text()
                        ),
                    );
                });
            });
            ui.add_space(SPACE_XS);
            let visible = card.revealed_steps();
            for (pos, item) in visible.iter().enumerate() {
                let step_no = pos + 1;
                // Fade de aparición sólo en el último revelado; con
                // movimiento reducido el alpha queda en 1.0 (sin transición).
                let reveal_id = ui.make_persistent_id((
                    "steps_card_reveal",
                    turn_index,
                    card_index,
                    card.revealed,
                ));
                let raw = ui.ctx().animate_bool(reveal_id, true);
                let alpha = if pos + 1 == visible.len() {
                    motion.morph_t(raw)
                } else {
                    1.0
                };
                let previous_opacity = ui.opacity();
                let staged = alpha < 0.999;
                if staged {
                    ui.set_opacity(previous_opacity * alpha.max(0.12));
                }
                egui::CollapsingHeader::new(format!(
                    "{step_no}/{} · {} {}",
                    card.steps.len(),
                    t("assistant.steps.rule", locale),
                    item.rule
                ))
                .default_open(true)
                .id_salt(ui.make_persistent_id(("steps_card_step", turn_index, card_index, pos)))
                .show(ui, |ui| {
                    if !item.description.trim().is_empty() {
                        ui.label(
                            egui::RichText::new(item.description.trim())
                                .color(theme.text_secondary)
                                .size(TYPE_SM),
                        );
                        ui.add_space(SPACE_XS);
                    }
                    let _ = draw_math(ui, &item.before);
                    ui.label(
                        egui::RichText::new("→")
                            .color(theme.text_tertiary)
                            .size(TYPE_SM),
                    );
                    let _ = draw_math(ui, &item.after);
                });
                if staged {
                    ui.set_opacity(previous_opacity);
                }
                ui.add_space(SPACE_XS);
            }
            // Fila de revelado: avanza de a un paso o muestra todo.
            if card.revealed < card.steps.len() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SPACE_SM;
                    let half = (ui.available_width() - SPACE_SM) / 2.0;
                    let reveal_label = t("assistant.steps.reveal", locale);
                    let reveal = ui.add_sized(
                        egui::vec2(half, hit_target_size(HIT_TARGET_MIN)),
                        egui::Button::new(egui::RichText::new(reveal_label).size(TYPE_SM)),
                    );
                    reveal.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            reveal_label.to_owned(),
                        )
                    });
                    if reveal.clicked() {
                        action = Some(AssistantUiAction::RevealStep { turn: turn_index });
                    }
                    let all_label = t("assistant.steps.reveal_all", locale);
                    let show_all = ui.add_sized(
                        egui::vec2(half, hit_target_size(HIT_TARGET_MIN)),
                        egui::Button::new(egui::RichText::new(all_label).size(TYPE_SM)),
                    );
                    show_all.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            all_label.to_owned(),
                        )
                    });
                    if show_all.clicked() {
                        action = Some(AssistantUiAction::RevealAllSteps { turn: turn_index });
                    }
                });
            }
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cas_step(rule: &str, before: &str, after: &str) -> CasStep {
        grafito_geometry::cas_steps::CasStep {
            index: 0,
            rule: rule_for(rule),
            before: before.to_string(),
            after: after.to_string(),
            description: format!("aplica {rule}"),
        }
    }

    fn rule_for(name: &str) -> grafito_geometry::cas_steps::RewriteRule {
        use grafito_geometry::cas_steps::RewriteRule as R;
        match name {
            "Simplify" => R::Simplify,
            "PowerRule" => R::PowerRule,
            _ => R::Generic,
        }
    }

    #[test]
    fn modelo_arranca_con_uno_revelado_y_avanza_acotado() {
        let steps = vec![
            cas_step("PowerRule", "x^2", "2*x"),
            cas_step("Simplify", "2*x", "2*x"),
        ];
        let mut card = StepByStepCardState::from_cas_steps("Derivative[x^2, x]", &steps);
        assert_eq!(card.revealed, 1);
        assert_eq!(card.counter_text(), "1/2");
        assert_eq!(card.revealed_steps().len(), 1);
        card.reveal_next();
        assert_eq!(card.counter_text(), "2/2");
        card.reveal_next();
        assert_eq!(card.revealed, 2, "el avance es idempotente en el tope");
        card.reveal_all();
        assert_eq!(card.revealed, 2);
    }

    #[test]
    fn modelo_vacio_queda_en_cero() {
        let card = StepByStepCardState::from_cas_steps("Integral[1, x]", &[]);
        assert_eq!(card.revealed, 0);
        assert_eq!(card.counter_text(), "0/0");
        assert!(card.revealed_steps().is_empty());
    }

    #[test]
    fn fence_hace_roundtrip() {
        let steps = vec![
            cas_step("PowerRule", "x^2", "2*x"),
            cas_step("Simplify", "2*x", "2*x"),
        ];
        let card = StepByStepCardState::from_cas_steps("Derivative[x^2, x]", &steps);
        let fence = card.to_code_block();
        assert!(fence.starts_with("```grafito-steps\n"));
        let parsed = StepByStepCardState::from_code_block(&fence).expect("parsea su propio fence");
        assert_eq!(parsed.title, "Derivative[x^2, x]");
        assert_eq!(parsed.steps.len(), 2);
        assert_eq!(parsed.steps[0].rule, "PowerRule");
        assert_eq!(parsed.steps[0].before, "x^2");
        assert_eq!(parsed.steps[0].after, "2*x");
    }

    #[test]
    fn parseo_trunca_a_max_cas_steps() {
        let mut fence = String::from("```grafito-steps\n");
        for i in 0..(MAX_CAS_STEPS + 10) {
            fence.push_str(&format!(
                "- rule: Generic\n  desc: d{i}\n  before: b{i}\n  after: a{i}\n"
            ));
        }
        fence.push_str("```");
        let card = StepByStepCardState::from_code_block(&fence).expect("fence largo parsea");
        assert_eq!(card.steps.len(), MAX_CAS_STEPS);
    }

    #[test]
    fn fence_malformado_no_panica_y_da_none() {
        assert!(StepByStepCardState::from_code_block("```grafito\nx\n```").is_none());
        assert!(StepByStepCardState::from_code_block("texto plano").is_none());
        assert!(StepByStepCardState::from_code_block("```grafito-steps\nbasura\n```").is_none());
        // Paso incompleto (sin after) se suelta, no se inventa.
        let fence = "```grafito-steps\n- rule: Generic\n  desc: d\n  before: b\n```";
        assert!(StepByStepCardState::from_code_block(fence).is_none());
    }

    #[test]
    fn next_revealed_acota_al_total() {
        assert_eq!(next_revealed(0, 3), 1);
        assert_eq!(next_revealed(3, 3), 3);
        assert_eq!(next_revealed(9, 2), 2);
    }

    #[test]
    fn conteo_en_contenido_suma_fences_y_tolera_basura() {
        let steps = vec![cas_step("PowerRule", "x^2", "2*x")];
        let card = StepByStepCardState::from_cas_steps("op", &steps);
        let content = format!(
            "Hola\n{}\nchau\n{}",
            card.to_code_block(),
            card.to_code_block()
        );
        assert_eq!(count_steps_in_content(&content), 2);
        assert_eq!(count_steps_in_content("sin fences"), 0);
        assert_eq!(count_steps_in_content("```grafito-steps\nbasura\n```"), 0);
    }

    #[test]
    fn movimiento_reducido_salta_el_fade() {
        let reduced = MotionConfig {
            reduced_motion: true,
        };
        assert_eq!(reduced.morph_t(0.2), 1.0);
        assert_eq!(MotionConfig::default().morph_t(0.2), 0.2);
    }

    #[test]
    fn truncado_respeta_borde_utf8() {
        let value = "áéíóú".to_string();
        let cut = truncate_str(&value, 5);
        assert!(cut.len() <= 5);
        assert!(value.starts_with(&cut));
    }
}
