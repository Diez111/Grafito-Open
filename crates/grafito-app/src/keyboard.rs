//! On-screen math keyboard docked at the bottom of the central area.
//!
//! Provides tabbed key pads (numeric, function, alphabetic, 3D shortcuts) and
//! dispatches typed text to the active input field.

use crate::GrafitoApp;
use egui::Color32;
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::{current_theme, KeyboardKeyRole};

pub(crate) const MATH_KEYBOARD_HEIGHT: f32 = 208.0;
pub(crate) const MATH_KEYBOARD_COMPACT_HEIGHT: f32 = 36.0;
const MATH_KEYBOARD_EDGE_MARGIN: f32 = 12.0;
const MATH_KEYBOARD_KEY_GAP: f32 = 4.0;
const MATH_KEYBOARD_COLUMNS: f32 = 8.0;

const ALPHABETIC_KEY_ROWS: &[&[(&str, &str)]] = &[
    &[
        ("q", "q"),
        ("w", "w"),
        ("e", "e"),
        ("r", "r"),
        ("t", "t"),
        ("y", "y"),
        ("u", "u"),
        ("i", "i"),
    ],
    &[
        ("a", "a"),
        ("s", "s"),
        ("d", "d"),
        ("f", "f"),
        ("g", "g"),
        ("h", "h"),
        ("j", "j"),
        ("k", "k"),
    ],
    &[
        ("z", "z"),
        ("x", "x"),
        ("c", "c"),
        ("v", "v"),
        ("b", "b"),
        ("n", "n"),
        ("m", "m"),
        (",", ","),
    ],
];

pub(crate) fn keyboard_insertion(tab: usize, label: &str) -> Option<&'static str> {
    if tab != 2 {
        return None;
    }
    ALPHABETIC_KEY_ROWS
        .iter()
        .flat_map(|row| row.iter())
        .find_map(|(key_label, insertion)| (*key_label == label).then_some(*insertion))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathKeyboardLayout {
    Hidden,
    Compact,
    Full,
}

impl MathKeyboardLayout {
    pub(crate) const fn height(self) -> f32 {
        match self {
            Self::Hidden => 0.0,
            Self::Compact => MATH_KEYBOARD_COMPACT_HEIGHT,
            Self::Full => MATH_KEYBOARD_HEIGHT,
        }
    }
}

pub(crate) const fn math_keyboard_layout(
    keyboard_visible: bool,
    keyboard_expanded: bool,
    _viewport_height: f32,
) -> MathKeyboardLayout {
    if !keyboard_visible {
        MathKeyboardLayout::Hidden
    } else if keyboard_expanded {
        MathKeyboardLayout::Full
    } else {
        MathKeyboardLayout::Compact
    }
}

fn keyboard_button_width(available_width: f32) -> f32 {
    let usable_width = (available_width - (MATH_KEYBOARD_EDGE_MARGIN * 2.0)).max(0.0);
    ((usable_width - ((MATH_KEYBOARD_COLUMNS - 1.0) * MATH_KEYBOARD_KEY_GAP) - 10.0)
        / MATH_KEYBOARD_COLUMNS)
        .clamp(18.0, 65.0)
}

fn keyboard_grid_padding(available_width: f32, button_width: f32) -> f32 {
    let usable_width = (available_width - (MATH_KEYBOARD_EDGE_MARGIN * 2.0)).max(0.0);
    let grid_width = (button_width * MATH_KEYBOARD_COLUMNS)
        + (MATH_KEYBOARD_KEY_GAP * (MATH_KEYBOARD_COLUMNS - 1.0));
    MATH_KEYBOARD_EDGE_MARGIN + ((usable_width - grid_width) / 2.0).max(0.0)
}

fn draw_compact_math_keyboard(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    egui::TopBottomPanel::bottom("math_keyboard")
        .exact_height(MATH_KEYBOARD_COMPACT_HEIGHT)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    egui::RichText::new("Teclado")
                        .size(11.0)
                        .color(theme.text_secondary),
                );
                for (label, insertion) in [
                    ("x", "x"),
                    ("y", "y"),
                    ("π", "π"),
                    ("(", "("),
                    (")", ")"),
                    ("+", "+"),
                ] {
                    if ui.small_button(label).clicked() {
                        app.input_text.push_str(insertion);
                    }
                }
                if action_icon_button(ui, Icon::Delete, theme.text_secondary, "Borrar entrada")
                    .clicked()
                {
                    app.input_text.pop();
                }
                if action_icon_button(ui, Icon::Play, theme.accent, "Ejecutar entrada").clicked() {
                    app.submit_input_text(ui.ctx().input(|input| input.time));
                }
                if action_icon_button(
                    ui,
                    Icon::ChevronUp,
                    theme.accent,
                    "Expandir teclado completo",
                )
                .clicked()
                {
                    app.keyboard_expanded = true;
                }
            });
        });
}

pub(crate) fn draw_math_keyboard(
    app: &mut GrafitoApp,
    ctx: &egui::Context,
    layout: MathKeyboardLayout,
) {
    if layout == MathKeyboardLayout::Hidden {
        return;
    }
    if layout == MathKeyboardLayout::Compact {
        draw_compact_math_keyboard(app, ctx);
        return;
    }

    let theme = current_theme(ctx);
    let sep_col = theme.separator;
    let panel_bg = theme.panel_bg;

    // ─── 4. MATH KEYBOARD — docked bottom panel (central area only) ──────────────
    egui::TopBottomPanel::bottom("math_keyboard")
        .exact_height(MATH_KEYBOARD_HEIGHT)
        .frame(
            egui::Frame::none()
                .fill(panel_bg)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_centered(|ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    // Tab bar
                    ui.horizontal(|ui| {
                        for (i, lbl) in ["123", "f(x)", "ABC", "3D"].iter().enumerate() {
                            let active = app.keyboard_tab == i;
                            let c = if active {
                                theme.keyboard_tab_active_text
                            } else {
                                theme.keyboard_tab_inactive
                            };
                            let fbg = if active {
                                theme.keyboard_tab_active_bg
                            } else {
                                Color32::TRANSPARENT
                            };
                            let r = egui::Frame::none()
                                .fill(fbg)
                                .rounding(6.0)
                                .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(*lbl).size(12.0).color(c).strong(),
                                    );
                                })
                                .response;
                            let response =
                                ui.interact(r.rect, ui.id().with(i), egui::Sense::click());
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *lbl)
                            });
                            if response.clicked() {
                                app.keyboard_tab = i;
                            }
                            ui.add_space(4.0);
                        }
                        if app.keyboard_expanded
                            && action_icon_button(
                                ui,
                                Icon::ChevronDown,
                                theme.text_secondary,
                                "Usar teclado compacto",
                            )
                            .clicked()
                        {
                            app.keyboard_expanded = false;
                        }
                    });
                    ui.add_space(5.0);

                    let btn_w = keyboard_button_width(ui.available_width());
                    let sp = MATH_KEYBOARD_KEY_GAP;
                    // Centra la grilla dentro del canvas y conserva un margen mínimo
                    // antes del asistente acoplado.
                    let pad = keyboard_grid_padding(ui.available_width(), btn_w);

                    macro_rules! kb {
                        ($ui:expr, $t:expr, $i:expr) => {{
                            let (r, resp) = $ui
                                .allocate_exact_size(egui::vec2(btn_w, 32.0), egui::Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, $t)
                            });
                            if $ui.is_rect_visible(r) {
                                let visuals = theme.keyboard_key_visuals(
                                    KeyboardKeyRole::Standard,
                                    resp.hovered(),
                                );
                                $ui.painter()
                                    .rect(r, 4.0, visuals.background, visuals.border);
                                $ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    $t,
                                    egui::FontId::proportional((btn_w * 0.4).clamp(10.0, 15.0)),
                                    visuals.text,
                                );
                            }
                            if resp.clicked() {
                                app.input_text.push_str(
                                    keyboard_insertion(app.keyboard_tab, $t).unwrap_or($i),
                                );
                            }
                        }};
                    }

                    let key_rows: &[&[(&str, &str)]] = match app.keyboard_tab {
                        0 => &[
                            &[
                                ("x", "x"),
                                ("y", "y"),
                                ("z", "z"),
                                ("i", "i"),
                                ("7", "7"),
                                ("8", "8"),
                                ("9", "9"),
                                ("/", "/"),
                            ],
                            &[
                                ("x²", "^2"),
                                ("v/", "sqrt("),
                                ("^", "^"),
                                ("|", "abs("),
                                ("4", "4"),
                                ("5", "5"),
                                ("6", "6"),
                                ("*", "*"),
                            ],
                            &[
                                ("π", "π"),
                                ("e", "e"),
                                ("<", "<"),
                                ("+", "+"),
                                ("1", "1"),
                                ("2", "2"),
                                ("3", "3"),
                                ("-", "-"),
                            ],
                        ],
                        1 => &[
                            &[
                                ("sin", "sin("),
                                ("cos", "cos("),
                                ("tan", "tan("),
                                ("asin", "asin("),
                                ("acos", "acos("),
                                ("atan", "atan("),
                                ("log", "log("),
                                ("ln", "ln("),
                            ],
                            &[
                                ("sec", "sec("),
                                ("csc", "csc("),
                                ("cot", "cot("),
                                ("!", "!"),
                                ("deg", "deg"),
                                ("rad", "rad"),
                                ("f", "f"),
                                ("g", "g"),
                            ],
                            &[
                                ("<", "<"),
                                (">", ">"),
                                ("(", "("),
                                (")", ")"),
                                ("1", "1"),
                                ("2", "2"),
                                ("3", "3"),
                                ("-", "-"),
                            ],
                        ],
                        2 => ALPHABETIC_KEY_ROWS,
                        _ => &[
                            &[
                                ("Lor", "Lorenz[10, 28, 2.66]"),
                                ("Roe", "Rossler[0.2, 0.2, 5.7]"),
                                ("Aiz", "Aizawa[0.95, 0.7, 0.6, 3.5, 0.25, 0.1]"),
                                ("Rab", "Dadras[3, 2.7, 1.7, 2, 9]"),
                                ("Sph", "Sphere[0,0,0,5]"),
                                ("Cub", "Cube[0,0,0,5]"),
                                ("P3D", "Point3D[1,1,1]"),
                                ("S3D", "Segment3D[0,0,0,1,1,1]"),
                            ],
                            &[
                                ("Hal", "Halvorsen[2.0]"),
                                ("Tho", "Thomas[0.208186]"),
                                ("Che", "Chen[35, 3, 28]"),
                                ("Spr", "Chua[15.6, 28, -1.14, -0.71]"),
                                ("Cyl", "Cylinder[0,0,0,2,5]"),
                                ("Con", "Cone[0,0,0,3,5]"),
                                ("Tor", "Torus[0,0,0,4,1]"),
                                ("Moe", "Moebius[2,1]"),
                            ],
                            &[
                                ("<", "<"),
                                (">", ">"),
                                ("(", "("),
                                (")", ")"),
                                ("[", "["),
                                ("]", "]"),
                                ("{", "{"),
                                ("}", "}"),
                            ],
                        ],
                    };
                    for row in key_rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(pad);
                            for (t, i) in *row {
                                kb!(ui, *t, *i);
                                ui.add_space(sp);
                            }
                        });
                        ui.add_space(sp);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(pad);
                        kb!(ui, "ans", "ans");
                        ui.add_space(sp);
                        kb!(ui, ".", ".");
                        ui.add_space(sp);
                        kb!(ui, "0", "0");
                        ui.add_space(sp);
                        kb!(ui, "(", "(");
                        ui.add_space(sp);
                        kb!(ui, ")", ")");
                        ui.add_space(sp);
                        kb!(ui, "=", "=");
                        ui.add_space(sp);
                        // Backspace
                        {
                            let (r, resp) = ui
                                .allocate_exact_size(egui::vec2(btn_w, 32.0), egui::Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    "Borrar entrada",
                                )
                            });
                            let visuals =
                                theme.keyboard_key_visuals(KeyboardKeyRole::Delete, resp.hovered());
                            ui.painter()
                                .rect(r, 4.0, visuals.background, visuals.border);
                            ui.painter().text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                "Del",
                                egui::FontId::proportional(14.0),
                                visuals.text,
                            );
                            if resp.clicked() {
                                app.input_text.pop();
                            }
                        }
                        ui.add_space(sp);
                        // Enter
                        {
                            let (r, resp) = ui
                                .allocate_exact_size(egui::vec2(btn_w, 32.0), egui::Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    "Ejecutar entrada",
                                )
                            });
                            let visuals =
                                theme.keyboard_key_visuals(KeyboardKeyRole::Enter, resp.hovered());
                            ui.painter()
                                .rect(r, 4.0, visuals.background, visuals.border);
                            ui.painter().text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                "Enter",
                                egui::FontId::proportional(13.0),
                                visuals.text,
                            );
                            if resp.clicked() {
                                let time = ui.ctx().input(|i| i.time);
                                app.submit_input_text(time);
                            }
                        }
                    });
                    ui.add_space(12.0);
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::{
        keyboard_button_width, keyboard_grid_padding, keyboard_insertion, math_keyboard_layout,
        MathKeyboardLayout, MATH_KEYBOARD_COMPACT_HEIGHT, MATH_KEYBOARD_EDGE_MARGIN,
        MATH_KEYBOARD_HEIGHT,
    };

    #[test]
    fn keyboard_collapses_before_it_crowds_short_viewports() {
        assert_eq!(
            math_keyboard_layout(false, false, 900.0),
            MathKeyboardLayout::Hidden
        );
        assert_eq!(
            math_keyboard_layout(true, false, 759.0),
            MathKeyboardLayout::Compact
        );
        assert_eq!(
            math_keyboard_layout(true, false, 1_080.0),
            MathKeyboardLayout::Compact
        );
        assert_eq!(
            math_keyboard_layout(true, true, 600.0),
            MathKeyboardLayout::Full
        );
        assert_eq!(
            math_keyboard_layout(true, false, 600.0).height(),
            MATH_KEYBOARD_COMPACT_HEIGHT
        );
    }

    #[test]
    fn alphabetic_comma_key_inserts_a_comma() {
        assert_eq!(keyboard_insertion(2, ","), Some(","));
    }

    #[test]
    fn keyboard_full_layout_uses_only_its_required_control_height() {
        assert_eq!(MATH_KEYBOARD_HEIGHT, 208.0);
    }

    #[test]
    fn keyboard_centers_the_grid_with_a_docked_panel_margin() {
        assert_eq!(MATH_KEYBOARD_EDGE_MARGIN, 12.0);
        for width in [480.0, 640.0, 960.0] {
            let button_width = keyboard_button_width(width);
            assert!(keyboard_grid_padding(width, button_width) >= MATH_KEYBOARD_EDGE_MARGIN);
        }
    }
}
