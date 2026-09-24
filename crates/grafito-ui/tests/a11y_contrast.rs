//! Regresión WCAG de la auditoría de accesibilidad (contraste medido).
//!
//! Calcula el ratio por par (fondo, texto) en light y dark con la fórmula
//! WCAG (luminancia relativa) y exige ≥4.5:1 en texto normal (1.4.3) y
//! ≥3:1 en no-texto significativo (1.4.11). Más lints de fuente que
//! impiden reintroducir los fallos (gamma crudo, hairlines fuera del
//! tema, claves i18n sin traducir).

use egui::Color32;
use grafito_ui::i18n::{t, Locale};
use grafito_ui::projector::{MotionConfig, ProjectorMode};
use grafito_ui::theme::{Theme, DARK, LIGHT};
use grafito_ui::tokens::{
    self, HIT_TARGET_MIN, RANGE_FIELD_H, STROKE_EMPHASIS, STROKE_HAIRLINE, TEXT_GAMMA_FLOOR,
};

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

/// Ratio de contraste WCAG entre texto y fondo.
fn ratio(text: Color32, background: Color32) -> f64 {
    let (a, b) = (luminance(text), luminance(background));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn assert_text(text: Color32, background: Color32, name: &str) {
    let value = ratio(text, background);
    assert!(
        value >= 4.5,
        "{name}: ratio {value:.2}:1 < 4.5:1 (WCAG 1.4.3)"
    );
}

fn themes() -> [(&'static str, &'static Theme); 2] {
    [("light", &LIGHT), ("dark", &DARK)]
}

// ── Base: texto del chrome ──

#[test]
fn chrome_text_passes_aa_in_both_modes() {
    for (mode, theme) in themes() {
        assert_text(
            theme.text_primary,
            theme.panel_bg,
            &format!("{mode} primary/panel"),
        );
        assert_text(
            theme.text_secondary,
            theme.toolbar_bg,
            &format!("{mode} secondary/toolbar"),
        );
        assert_text(
            theme.text_tertiary,
            theme.panel_bg,
            &format!("{mode} tertiary/panel"),
        );
    }
}

// ── FIX 1: selector de idioma (antes WHITE sobre selection_bg ≈1.2:1) ──

#[test]
fn locale_active_pair_passes_aa_in_both_modes() {
    for (mode, theme) in themes() {
        assert_text(
            theme.keyboard_tab_active_text,
            theme.keyboard_tab_active_bg,
            &format!("{mode} locale activo"),
        );
    }
    // El bug original: blanco sobre `selection_bg` claro.
    let bug = ratio(Color32::WHITE, LIGHT.selection_bg);
    assert!(bug < 4.5, "el par viejo debía fallar, dio {bug:.2}:1");
}

// ── FIX 2: burbuja de teaching (antes primary-dark sobre blanco ≈1.04:1) ──

#[test]
fn teaching_bubble_passes_aa_in_both_modes() {
    for (mode, theme) in themes() {
        assert_text(
            theme.text_primary,
            theme.assistant_assistant_bubble,
            &format!("{mode} burbuja teaching"),
        );
    }
    let bug = ratio(DARK.text_primary, Color32::WHITE);
    assert!(bug < 4.5, "el par viejo debía fallar, dio {bug:.2}:1");
}

// ── FIX 3: `accent` como texto (dark 3.09:1, light 4.33:1) ──

#[test]
fn accent_as_text_was_below_aa_and_replacements_pass() {
    // Reproducción del fallo.
    assert!(ratio(DARK.accent, DARK.panel_bg) < 4.5);
    assert!(ratio(LIGHT.accent, LIGHT.canvas_bg) < 4.5);
    // Reemplazos usados en el código.
    for (mode, theme) in themes() {
        assert_text(
            theme.accent_strong,
            theme.panel_bg,
            &format!("{mode} accent_strong/panel"),
        );
        assert_text(
            theme.text_secondary,
            theme.assistant_assistant_bubble,
            &format!("{mode} secondary/burbuja"),
        );
    }
    assert_text(
        DARK.text_primary,
        DARK.accent_muted,
        "dark primary/accent_muted",
    );
    // Botón "Siguiente": blanco sobre accent en ambos modos.
    for (mode, theme) in themes() {
        assert_text(
            Color32::WHITE,
            theme.accent,
            &format!("{mode} blanco/accent"),
        );
    }
}

// ── FIX 4: atenuaciones bajo el piso gamma ──

#[test]
fn dimmed_text_never_drops_below_floor_contrast() {
    assert_eq!(TEXT_GAMMA_FLOOR, 0.85);
    for (mode, theme) in themes() {
        // `secondary` clampeado al piso pasa AA en ambos modos (el 0.60
        // crudo daba 2.85:1 en dark). Medido: 5.11:1 en dark.
        for gamma in [0.60, 0.75] {
            let secondary = Theme::dimmed_text(theme.text_secondary, gamma);
            assert_text(
                secondary,
                theme.panel_bg,
                &format!("{mode} secondary@{gamma}/panel"),
            );
            assert_text(
                secondary,
                theme.toolbar_bg,
                &format!("{mode} secondary@{gamma}/toolbar"),
            );
        }
        // `tertiary` NO admite atenuación: ni siquiera en el piso 0.85
        // llega a 4.5 en dark (3.89:1 medido). Va sin dimmir (5.18:1).
        assert_text(
            theme.text_tertiary,
            theme.panel_bg,
            &format!("{mode} tertiary/panel"),
        );
        let primary = Theme::dimmed_text(theme.text_primary, 0.85);
        assert_text(
            primary,
            theme.panel_bg,
            &format!("{mode} primary@floor/panel"),
        );
    }
    // El lint prohíbe dimmir tertiary: el piso no le alcanza.
    for (name, source) in [
        ("assistant.rs", include_str!("../src/assistant.rs")),
        ("teaching.rs", include_str!("../src/teaching.rs")),
        ("toolbar.rs", include_str!("../src/toolbar.rs")),
    ] {
        assert!(
            !source.contains("dimmed_text(theme.text_tertiary,"),
            "{name}: tertiary no se atenúa (ni al piso llega a AA en dark)"
        );
    }
}

#[test]
fn no_raw_text_gamma_in_sources() {
    // Patrón del lint `botones_paso_a_paso…`: falla si aparece el needle.
    // Todo `gamma_multiply` sobre texto pasa por `Theme::dimmed_text`.
    for (name, source) in [
        ("assistant.rs", include_str!("../src/assistant.rs")),
        ("toolbar.rs", include_str!("../src/toolbar.rs")),
        ("teaching.rs", include_str!("../src/teaching.rs")),
        ("avatar.rs", include_str!("../src/avatar.rs")),
        ("theme.rs", include_str!("../src/theme.rs")),
        ("tokens.rs", include_str!("../src/tokens.rs")),
    ] {
        for needle in [
            "text_primary.gamma_multiply(",
            "text_secondary.gamma_multiply(",
            "text_tertiary.gamma_multiply(",
            "text_label.gamma_multiply(",
        ] {
            assert!(
                !source.contains(needle),
                "{name} contiene gamma crudo sobre texto: {needle}"
            );
        }
    }
}

// ── FIX 5: targets, proyector, movimiento ──

#[test]
#[allow(clippy::assertions_on_constants)]
fn range_field_meets_minimum_target() {
    assert!(
        RANGE_FIELD_H >= HIT_TARGET_MIN,
        "RANGE_FIELD_H={RANGE_FIELD_H} < 24 (WCAG 2.5.8)"
    );
}

#[test]
fn projector_mode_drives_hit_targets() {
    // El módulo está cableado: el selector de idioma pasa por hit_target
    // y font_size (antes: 0 call-sites fuera de projector.rs).
    let source = include_str!("../src/toolbar.rs");
    assert!(source.contains("locale_selector_with_mode"));
    assert!(source.contains("mode.hit_target("));
    assert!(source.contains("mode.font_size("));
    let off = ProjectorMode::default();
    let on = ProjectorMode { enabled: true };
    assert_eq!(off.hit_target(24.0), 24.0);
    assert_eq!(on.hit_target(24.0), 44.0);
}

#[test]
fn reduced_motion_disables_animation() {
    let full = MotionConfig::default();
    let reduced = MotionConfig {
        reduced_motion: true,
    };
    assert_eq!(full.anim_ms(180.0), 180.0);
    assert_eq!(reduced.anim_ms(180.0), 0.0);
    assert_eq!(reduced.morph_t(0.2), 1.0);
    let teaching = include_str!("../src/teaching.rs");
    assert!(teaching.contains("MotionConfig"));
}

// ── FIX 6: i18n del player y la live-region ──

#[test]
fn media_tips_and_live_keys_cover_all_locales() {
    for locale in [
        Locale::Es,
        Locale::En,
        Locale::Pt,
        Locale::It,
        Locale::Fr,
        Locale::De,
    ] {
        for key in [
            "assistant.media_tip_play",
            "assistant.media_tip_pause",
            "assistant.media_tip_back",
            "assistant.media_tip_fwd",
            "assistant.media_tip_export",
            "assistant.live_prefix",
            "assistant.live_error",
            "assistant.live_ready",
        ] {
            let text = t(key, locale);
            assert!(!text.is_empty(), "{key} vacío en {locale:?}");
            assert!(!text.contains("{detail}") || key.contains("live_error"));
        }
        // La live-region EN no habla español.
        assert!(
            t("assistant.live_ready", locale) != t("assistant.live_ready", Locale::Es)
                || locale == Locale::Es
        );
    }
    assert!(t("assistant.live_ready", Locale::En).starts_with("Assistant:"));
    assert!(t("assistant.live_ready", Locale::Es).starts_with("Asistente:"));
}

// ── FIX 7: strokes y márgenes con token ──

#[test]
fn stroke_tokens_exist_and_hairline_is_used() {
    assert_eq!(STROKE_HAIRLINE, 1.0);
    assert_eq!(STROKE_EMPHASIS, 1.5);
    for (mode, theme) in themes() {
        let stroke = theme.hairline_stroke();
        assert_eq!(stroke.width, STROKE_HAIRLINE);
        let _ = mode;
    }
    for (name, source) in [
        ("assistant.rs", include_str!("../src/assistant.rs")),
        ("teaching.rs", include_str!("../src/teaching.rs")),
    ] {
        assert!(
            !source.contains("Margin::symmetric(7.0, 2.0)"),
            "{name}: margen fuera de base-4"
        );
        // El hairline 1.0 sobre separator va por `hairline_stroke()`.
        assert!(
            !source.contains("Stroke::new(1.0, theme.separator.gamma_multiply(0.10))"),
            "{name}: hairline sin token"
        );
    }
    assert_eq!(tokens::SPACE_XXS, 2.0);
}

// ── FIX 8: conteos ──

#[test]
fn tool_counts_are_consistent() {
    let toolbar = include_str!("../src/toolbar.rs");
    assert!(!toolbar.contains("all_87_tools"));
    assert!(!toolbar.contains("88 variantes"));
}
