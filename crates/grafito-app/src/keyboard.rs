//! On-screen math keyboard docked at the bottom of the central area.
//!
//! Provides tabbed key pads (numeric, function, alphabetic, 3D shortcuts) and
//! dispatches typed text to the active input field.

use crate::GrafitoApp;
use egui::Color32;
use grafito_ui::theme::current_theme;

pub(crate) fn draw_math_keyboard(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let is_dark = app.dark_mode;
    let accent = theme.accent;
    let sep_col = theme.separator;
    let panel_bg = theme.panel_bg;

    // ─── 4. MATH KEYBOARD — docked bottom panel (central area only) ──────────────
    if app.keyboard_visible {
        egui::TopBottomPanel::bottom("math_keyboard")
            .min_height(180.0)
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
                                    accent
                                } else {
                                    Color32::from_gray(110)
                                };
                                let fbg = if active {
                                    theme.accent_muted
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
                                if ui
                                    .interact(r.rect, ui.id().with(i), egui::Sense::click())
                                    .clicked()
                                {
                                    app.keyboard_tab = i;
                                }
                                ui.add_space(4.0);
                            }
                        });
                        ui.add_space(5.0);

                        let avail_w = ui.available_width();
                        let sp = 4.0_f32;
                        // Bajamos el mínimo de btn_w de 24 a 18 para ventanas
                        // angostas; envolveremos toda la fila en ScrollArea
                        // abajo para que no se corte.
                        let btn_w = ((avail_w - (7.0 * sp) - 10.0) / 8.0).clamp(18.0, 65.0);
                        let total_w = (btn_w * 8.0) + (sp * 7.0);
                        let pad = ((avail_w - total_w) / 2.0).max(0.0);

                        macro_rules! kb {
                            ($ui:expr, $t:expr, $i:expr) => {{
                                let (r, resp) = $ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                if $ui.is_rect_visible(r) {
                                    let bg = if resp.hovered() {
                                        if is_dark {
                                            Color32::from_gray(70)
                                        } else {
                                            Color32::from_gray(215)
                                        }
                                    } else {
                                        if is_dark {
                                            Color32::from_gray(48)
                                        } else {
                                            Color32::WHITE
                                        }
                                    };
                                    $ui.painter().rect(
                                        r,
                                        4.0,
                                        bg,
                                        egui::Stroke::new(
                                            1.0,
                                            Color32::from_gray(if is_dark { 65 } else { 210 }),
                                        ),
                                    );
                                    $ui.painter().text(
                                        r.center(),
                                        egui::Align2::CENTER_CENTER,
                                        $t,
                                        egui::FontId::proportional((btn_w * 0.4).clamp(10.0, 15.0)),
                                        if is_dark {
                                            Color32::WHITE
                                        } else {
                                            Color32::BLACK
                                        },
                                    );
                                }
                                if resp.clicked() {
                                    app.input_text.push_str($i);
                                }
                            }};
                        }

                        let key_rows: &[&[(&str, &str)]] = match app.keyboard_tab {
                            0 => &[
                                &[
                                    ("x", "x"),
                                    ("y", "y"),
                                    ("π", "π"),
                                    ("e", "e"),
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
                            2 => &[
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
                                    (",", ""),
                                ],
                            ],
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
                                ui.add_space(pad);
                                for (t, i) in *row {
                                    kb!(ui, *t, *i);
                                    ui.add_space(sp);
                                }
                            });
                            ui.add_space(sp);
                        }
                        ui.horizontal(|ui| {
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
                                let (r, resp) = ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                let bg = if resp.hovered() {
                                    theme.danger
                                } else {
                                    Color32::from_gray(if is_dark { 48 } else { 230 })
                                };
                                ui.painter().rect(
                                    r,
                                    4.0,
                                    bg,
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_gray(if is_dark { 65 } else { 210 }),
                                    ),
                                );
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Del",
                                    egui::FontId::proportional(14.0),
                                    if is_dark {
                                        Color32::WHITE
                                    } else {
                                        Color32::BLACK
                                    },
                                );
                                if resp.clicked() {
                                    app.input_text.pop();
                                }
                            }
                            ui.add_space(sp);
                            // Enter
                            {
                                let (r, resp) = ui.allocate_exact_size(
                                    egui::vec2(btn_w, 32.0),
                                    egui::Sense::click(),
                                );
                                let bg = if resp.hovered() {
                                    theme.accent
                                } else {
                                    // Versión más oscura del accent: gamma un 50%.
                                    let a = theme.accent;
                                    Color32::from_rgb(
                                        (a.r() as f32 * 0.7) as u8,
                                        (a.g() as f32 * 0.7) as u8,
                                        (a.b() as f32 * 0.7) as u8,
                                    )
                                };
                                ui.painter().rect(r, 4.0, bg, egui::Stroke::NONE);
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Enter",
                                    egui::FontId::proportional(13.0),
                                    Color32::WHITE,
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
}
