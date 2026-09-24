//! StepByStepCard: lints de Piel, i18n y contraste medido.
//!
//! - La tarjeta usa tokens (sin `Stroke`/`Margin` literales ni gamma crudo
//!   sobre texto) y el `draw_math` existente.
//! - Las claves visibles resuelven en los 6 idiomas vía `t` (nada
//!   hardcodeado en español).
//! - El contraste de los pares de la tarjeta se mide con la fórmula WCAG
//!   (mismo patrón que `a11y_contrast.rs`) y exige ≥4.5:1.

use egui::Color32;
use grafito_ui::i18n::{t, Locale};
use grafito_ui::theme::{DARK, LIGHT};

fn channel(value: u8) -> f64 {
    let value = f64::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(color: Color32) -> f64 {
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

fn ratio(text: Color32, background: Color32) -> f64 {
    let (a, b) = (luminance(text), luminance(background));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn assert_text(text: Color32, background: Color32, name: &str) {
    let value = ratio(text, background);
    eprintln!("contraste {name}: {value:.2}:1");
    assert!(
        value >= 4.5,
        "{name}: ratio {value:.2}:1 < 4.5:1 (WCAG 1.4.3)"
    );
}

#[test]
fn tarjeta_usa_tokens_y_patrones_de_piel() {
    let source = include_str!("../src/step_by_step.rs");
    // Sin literales de dibujo: tamaños y strokes vienen de tokens o tema
    // (`Margin::same(SPACE_SM)` y `vec2` con anchos vivos sí se permiten,
    // igual que en `assistant.rs`).
    for needle in [
        "Stroke::new(1.0",
        "Stroke::new(1.5",
        "Margin::symmetric(7.0",
    ] {
        assert!(
            !source.contains(needle),
            "step_by_step.rs usa literal de dibujo: {needle}"
        );
    }
    // Reúsa el render matemático y el catálogo i18n existentes.
    assert!(
        source.contains("draw_math("),
        "la tarjeta debe usar draw_math"
    );
    assert!(
        source.contains("crate::i18n::t(") || source.contains("t(\"assistant.steps."),
        "strings visibles vía i18n"
    );
    // Accesibilidad: targets y live-region con el patrón del repo.
    assert!(source.contains("HIT_TARGET_MIN"), "targets ≥24px");
    assert!(
        source.contains("WidgetInfo::labeled"),
        "botones con etiqueta"
    );
    assert!(
        source.contains("tag_live_region"),
        "contador en live-region"
    );
    // Movimiento reducido gobernado por MotionConfig.
    assert!(source.contains("morph_t("), "fade respeta reduced_motion");
    // Cotas del motor, sin redefinirlas.
    assert!(
        source.contains("MAX_CAS_STEPS"),
        "respeta la cota del motor"
    );
    assert!(!source.contains("unwrap("), "prohibido unwrap");
    assert!(!source.contains("expect("), "prohibido expect");
}

#[test]
fn claves_de_pasos_resuelven_en_seis_idiomas() {
    for locale in [
        Locale::Es,
        Locale::En,
        Locale::Pt,
        Locale::It,
        Locale::Fr,
        Locale::De,
    ] {
        for key in [
            "assistant.steps.title",
            "assistant.steps.reveal",
            "assistant.steps.reveal_all",
            "assistant.steps.rule",
            "palette.action.step_by_step",
        ] {
            let text = t(key, locale);
            assert!(!text.is_empty(), "{key} vacío en {locale:?}");
            assert_ne!(text, key, "{key} sin resolver en {locale:?}");
        }
    }
    // ES rioplatense idéntico al UI; EN distinto (no es fallback).
    assert_eq!(t("assistant.steps.reveal", Locale::Es), "Revelar paso");
    assert_eq!(t("assistant.steps.reveal_all", Locale::Es), "Ver todo");
    assert_eq!(t("assistant.steps.reveal", Locale::En), "Reveal step");
    assert_eq!(t("palette.action.step_by_step", Locale::En), "Step by Step");
}

#[test]
fn contraste_de_la_tarjeta_pasa_aa_en_ambos_modos() {
    for (mode, theme) in [("light", &LIGHT), ("dark", &DARK)] {
        // La tarjeta va en `panel_bg` sobre el turno (`input_bg`), igual que
        // los bloques DisplayMath: los 4 pares ya pasan AA (ver
        // `chrome_text_passes_aa…` y `dimmed_text…`; acá se mide e imprime).
        let bg = theme.panel_bg;
        assert_text(
            theme.text_primary,
            bg,
            &format!("{mode} tarjeta título/input"),
        );
        assert_text(
            theme.text_secondary,
            bg,
            &format!("{mode} tarjeta descripción/input"),
        );
        assert_text(
            theme.text_tertiary,
            bg,
            &format!("{mode} tarjeta flecha/input"),
        );
        assert_text(
            theme.accent_strong,
            bg,
            &format!("{mode} tarjeta contador/input"),
        );
    }
}
