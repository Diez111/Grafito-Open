//! Removable side panels: CAS, view settings, spreadsheet, and value table.

use crate::{commands, GrafitoApp, ViewMode};
use egui::Color32;
use grafito_core::{GeoObject, PointObj};
use grafito_geometry::Point2;
use grafito_ui::theme::{current_theme, DARK, LIGHT};

type FuncInfo = (String, String, String, Option<f64>, Option<f64>);

/// Helper de retrocompatibilidad. Devuelve la tupla histórica
/// `(is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, hdr_col)`
/// usando el Theme activo.
#[allow(clippy::type_complexity)]
fn panel_theme_local(
    ctx: &egui::Context,
) -> (bool, Color32, Color32, Color32, Color32, Color32, Color32) {
    let t = current_theme(ctx);
    let is_dark = t.canvas_bg.r() < 100;
    (
        is_dark,
        t.accent,
        t.panel_bg,
        t.separator,
        t.text_primary,
        t.text_tertiary,
        t.text_secondary,
    )
}

pub(crate) fn draw_cas_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let _ = app;
    let theme = current_theme(ctx);
    let accent = theme.accent;
    let alg_fill = theme.panel_bg;
    let sep_col = theme.separator;
    let txt_col = theme.text_primary;
    let txt_dim = theme.text_tertiary;
    let _hdr_col = theme.text_secondary;

    // ── CAS PANEL (tab 1) ──
    egui::SidePanel::left("cas_panel")
        .default_width(260.0).min_width(180.0).resizable(true)
        .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Cálculo Simbólico (CAS)").color(accent).strong().size(16.0));
            });
            ui.add_space(4.0);
            ui.separator();

            egui::Frame::none().inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Derivar").clicked() { app.input_text = "Derivative[".to_string(); }
                    if ui.button("Integrar").clicked() { app.input_text = "Integral[".to_string(); }
                    if ui.button("Resolver").clicked() { app.input_text = "Solve[".to_string(); }
                    if ui.button("Límite").clicked() { app.input_text = "Limit[".to_string(); }
                });
            });
            ui.separator();
            // CAS Input
            ui.label(egui::RichText::new("Entrada CAS:").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut execute_cas = false;
                if ui.add_sized([28.0, 24.0], egui::Button::new("▶")).clicked() {
                    execute_cas = true;
                }

                let r = ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::TextEdit::singleline(&mut app.input_text)
                        .hint_text("Comando CAS...")
                );

                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    execute_cas = true;
                }

                if execute_cas && !app.input_text.is_empty() {
                    app.save_state();
                    let mut cmd = app.input_text.clone();
                    let input_was = app.input_text.clone();
                    let outcome = crate::commands::process_input(&mut app.document, &mut cmd);
                    let time = ui.ctx().input(|i| i.time);
                    app.handle_command_outcome(outcome, time, &input_was);
                    app.input_text.clear();
                }
            });

            // Show CAS history
            egui::ScrollArea::vertical().max_height(ui.available_height() - 8.0).show(ui, |ui| {
                egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                    if app.cas_history.is_empty() {
                        ui.label(egui::RichText::new("Escribe comandos CAS...\n\nEj: Derivative[x², x]\nEj: Integral[sin(x), x]\nEj: Solve[x²-4, x]\nEj: Limit[sin(x)/x, x, 0]").size(12.0).color(txt_dim));
                    } else {
                        for (i, entry) in app.cas_history.iter().enumerate() {
                            egui::Frame::none()
                                .fill(if app.dark_mode { Color32::from_rgb(45, 45, 50) } else { Color32::from_rgb(240, 240, 245) })
                                .rounding(6.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(format!("{}", i+1)).color(accent).strong());
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new(entry).size(13.0).color(txt_col));
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    }
                });
            });
        });
}

pub(crate) fn draw_view_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    // ── VIEW/SETTINGS PANEL (tab 4) ──
    egui::SidePanel::left("view_panel")
        .default_width(220.0)
        .min_width(160.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Vista")
                        .color(accent)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(4.0);
            ui.separator();

            egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                ui.label(
                    egui::RichText::new("General")
                        .color(txt_dim)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.checkbox(&mut app.show_grid, "Mostrar cuadrícula");
                ui.checkbox(&mut app.dark_mode, "Modo oscuro")
                    .changed()
                    .then(|| {
                        if app.dark_mode {
                            DARK.apply(ui.ctx());
                        } else {
                            LIGHT.apply(ui.ctx());
                        }
                    });
                ui.checkbox(&mut app.snap_to_grid, "Ajustar a cuadrícula");
                ui.checkbox(&mut app.exam_mode, "Modo examen");
                let mut is_3d = app.current_view == ViewMode::D3;
                if ui.checkbox(&mut is_3d, "Vista 3D").changed() {
                    app.current_view = if is_3d { ViewMode::D3 } else { ViewMode::D2 };
                }

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("Ejes")
                        .color(txt_dim)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.checkbox(&mut app.document.view_mut().x_log, "Eje X logarítmico");
                ui.checkbox(&mut app.document.view_mut().y_log, "Eje Y logarítmico");

                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("Exportación")
                        .color(txt_dim)
                        .size(11.0)
                        .strong(),
                );
                ui.add_space(4.0);
                if ui.button("Exportar SVG").clicked() {
                    let svg = crate::export::export_svg(&app.document, 800.0, 600.0);
                    let path = "grafito_export.svg";
                    let _ = std::fs::write(path, svg);
                    app.cas_result = format!("SVG saved to {}", path);
                }
            });
        });
}

pub(crate) fn draw_spreadsheet_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("spreadsheet_panel")
        .default_width(260.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Hoja de Cálculo")
                        .color(accent)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(4.0);
            ui.separator();

            let (mut rows, mut cols) = app.document.spreadsheet_dim();
            // Assure at least 15 rows and 6 columns for nice UI, but expand infinitely if needed
            rows = rows.max(15);
            cols = cols.max(6);

            egui::ScrollArea::both().show(ui, |ui| {
                egui::Frame::none()
                    .stroke(egui::Stroke::new(1.0, sep_col))
                    .show(ui, |ui| {
                        egui::Grid::new("mini_sheet")
                            .striped(true)
                            .min_col_width(60.0)
                            .spacing(egui::vec2(0.0, 0.0))
                            .show(ui, |ui| {
                                // Header row
                                ui.add_sized([28.0, 28.0], egui::Label::new(""));
                                for c in 0..cols {
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(8.0);
                                        let col_name = if c < 26 {
                                            format!("{}", (b'A' + c as u8) as char)
                                        } else {
                                            format!(
                                                "{}{}",
                                                (b'A' + (c / 26 - 1) as u8) as char,
                                                (b'A' + (c % 26) as u8) as char
                                            )
                                        };
                                        ui.label(
                                            egui::RichText::new(col_name)
                                                .size(12.0)
                                                .strong()
                                                .color(accent),
                                        );
                                    });
                                }
                                ui.end_row();

                                // Data rows
                                for r in 0..rows {
                                    ui.horizontal_centered(|ui| {
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(format!("{}", r + 1))
                                                .size(11.0)
                                                .color(txt_dim),
                                        );
                                    });
                                    for c in 0..cols {
                                        let mut val = app.document.get_spreadsheet_cell(r, c);

                                        let cell_frame = egui::Frame::none()
                                            .stroke(egui::Stroke::new(0.5, sep_col))
                                            .inner_margin(egui::Margin::symmetric(4.0, 4.0));

                                        cell_frame.show(ui, |ui| {
                                            let r2 = ui.add_sized(
                                                [60.0, 20.0],
                                                egui::TextEdit::singleline(&mut val)
                                                    .font(egui::FontId::proportional(12.0))
                                                    .frame(false),
                                            ); // No pill frame!

                                            if r2.changed() {
                                                if let Err(e) =
                                                    app.document.set_spreadsheet_cell(r, c, val)
                                                {
                                                    log::warn!("set_spreadsheet_cell: {}", e);
                                                }
                                                if let Some(ev) =
                                                    app.document.eval_spreadsheet_cell(r, c)
                                                {
                                                    app.document.set_variable(
                                                        format!(
                                                            "{}{}",
                                                            (b'A' + c as u8) as char,
                                                            r + 1
                                                        ),
                                                        ev,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });

            ui.add_space(8.0);
            if ui.button("Abrir hoja completa →").clicked() {
                app.show_spreadsheet = !app.show_spreadsheet;
            }
        });
}

pub(crate) fn draw_table_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("table_panel")
        .default_width(240.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            let functions: Vec<FuncInfo> = app
                .document
                .objects_iter()
                .filter_map(|(_, obj)| match obj {
                    GeoObject::Function(f) => Some((
                        f.label.clone(),
                        f.expr.clone(),
                        "f(x)".to_string(),
                        f.domain_min,
                        f.domain_max,
                    )),
                    GeoObject::ParametricCurve2D(pc) => Some((
                        pc.label.clone(),
                        format!("x={}, y={}", pc.expr_x, pc.expr_y),
                        "(x,y)".to_string(),
                        Some(pc.t_min),
                        Some(pc.t_max),
                    )),
                    GeoObject::PolarCurve(pc) => Some((
                        pc.label.clone(),
                        pc.expr_r.clone(),
                        "r(θ)".to_string(),
                        Some(pc.t_min),
                        Some(pc.t_max),
                    )),
                    _ => None,
                })
                .collect();

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Tabla de Valores")
                        .color(accent)
                        .strong()
                        .size(16.0),
                );
            });
            ui.add_space(4.0);
            ui.separator();

            if functions.is_empty() {
                egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Sin funciones\nEscribe f(x)=... en la entrada")
                            .size(12.0)
                            .color(txt_dim),
                    );
                });
            } else {
                if app.table_func_idx >= functions.len() {
                    app.table_func_idx = 0;
                }
                let (_, expr, ftype, dmin, dmax) = &functions[app.table_func_idx];
                let var = match ftype.as_str() {
                    "(x,y)" | "r(θ)" => "t",
                    _ => "x",
                };
                let name_labels: Vec<String> =
                    functions.iter().map(|(l, _, _, _, _)| l.clone()).collect();

                egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Función:").strong());
                        let selected = name_labels
                            .get(app.table_func_idx)
                            .cloned()
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("func_dropdown")
                            .selected_text(&selected)
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for (i, name) in name_labels.iter().enumerate() {
                                    if ui.selectable_label(app.table_func_idx == i, name).clicked()
                                    {
                                        app.table_func_idx = i;
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);
                    egui::Grid::new("table_config_grid")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Desde:");
                            ui.add_sized(
                                [80.0, 18.0],
                                egui::TextEdit::singleline(&mut app.table_x_min)
                                    .font(egui::FontId::proportional(12.0)),
                            );
                            ui.end_row();

                            ui.label("Hasta:");
                            ui.add_sized(
                                [80.0, 18.0],
                                egui::TextEdit::singleline(&mut app.table_x_max)
                                    .font(egui::FontId::proportional(12.0)),
                            );
                            ui.end_row();

                            ui.label("Paso:");
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [50.0, 18.0],
                                    egui::TextEdit::singleline(&mut app.table_step)
                                        .font(egui::FontId::proportional(12.0)),
                                );
                                if ui
                                    .button("📍")
                                    .on_hover_text("Agregar puntos al canvas")
                                    .clicked()
                                {
                                    let x_min: f64 = app.table_x_min.parse().unwrap_or(-5.0);
                                    let x_max: f64 = app.table_x_max.parse().unwrap_or(5.0);
                                    let step: f64 = app.table_step.parse().unwrap_or(1.0);
                                    let is_polar = ftype == "r(θ)";
                                    let mut x = x_min;
                                    while x <= x_max + 1e-9 {
                                        let vars = vec![(var.to_string(), x)];
                                        if let Ok(y) = grafito_geometry::expr::evaluate(expr, &vars)
                                        {
                                            if y.is_finite() {
                                                let pt = if is_polar {
                                                    Point2::new(y * x.cos(), y * x.sin())
                                                } else {
                                                    Point2::new(x, y)
                                                };
                                                app.document.add_object(GeoObject::Point(
                                                    PointObj::new(pt),
                                                ));
                                            }
                                        }
                                        x += step;
                                    }
                                }
                            });
                            ui.end_row();
                        });
                });
                ui.separator();

                // Table display
                let x_min: f64 = dmin.unwrap_or(app.table_x_min.parse().unwrap_or(-5.0));
                let x_max: f64 = dmax.unwrap_or(app.table_x_max.parse().unwrap_or(5.0));
                let step: f64 = app.table_step.parse().unwrap_or(1.0);
                let max_rows = 50;
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 8.0)
                    .show(ui, |ui| {
                        egui::Frame::none().inner_margin(8.0).show(ui, |ui| {
                            egui::Grid::new("tbl_grid")
                                .striped(true)
                                .min_col_width(80.0)
                                .spacing([16.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(var).strong().color(accent));
                                    ui.label(
                                        egui::RichText::new(&functions[app.table_func_idx].0)
                                            .strong()
                                            .color(accent),
                                    );
                                    ui.end_row();

                                    let mut x = x_min;
                                    let mut count = 0;
                                    while x <= x_max + 1e-9 && count < max_rows {
                                        let vars = vec![(var.to_string(), x)];
                                        if let Ok(y) = grafito_geometry::expr::evaluate(expr, &vars)
                                        {
                                            if y.is_finite() {
                                                ui.label(
                                                    egui::RichText::new(format!("{:.3}", x))
                                                        .size(12.0),
                                                );
                                                let out = format!("{:.4}", y);
                                                ui.label(egui::RichText::new(out).size(12.0));
                                                ui.end_row();
                                            }
                                        }
                                        x += step;
                                        count += 1;
                                    }
                                });
                        });
                    });
            }
        });
}

pub(crate) fn draw_empty_panel(_app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, _accent, alg_fill, sep_col, _txt_col, _txt_dim, _hdr_col) =
        panel_theme_local(ctx);

    egui::SidePanel::left("empty_panel")
        .default_width(220.0)
        .min_width(160.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(egui::RichText::new("En construcción...").color(Color32::from_gray(150)));
            });
        });
}

pub(crate) fn draw_right_spreadsheet(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (is_dark, _accent, alg_fill, sep_col, _txt_col, _txt_dim, _hdr_col) =
        panel_theme_local(ctx);

    // ─── 5. SPREADSHEET (optional right panel) ────────────────────────────
    if app.show_spreadsheet {
        egui::SidePanel::right("spreadsheet")
            .resizable(true)
            .default_width(280.0)
            .frame(
                egui::Frame::none()
                    .fill(alg_fill)
                    .stroke(egui::Stroke::new(1.0, sep_col)),
            )
            .show(ctx, |ui| {
                ui.heading("Hoja de Cálculo");
                ui.separator();
                let (rows, cols) = app.document.spreadsheet_dim();
                let text_col = if is_dark {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };
                let hdr_col = if is_dark {
                    Color32::from_gray(160)
                } else {
                    Color32::from_gray(80)
                };

                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        egui::Grid::new("sp_grid")
                            .min_col_width(52.0)
                            .spacing(egui::vec2(1.0, 1.0))
                            .striped(true)
                            .show(ui, |ui| {
                                // Header row
                                ui.label(""); // corner
                                for c in 0..cols {
                                    let letter = if c < 26 {
                                        format!("{}", (b'A' + c as u8) as char)
                                    } else {
                                        format!("{}", c + 1)
                                    };
                                    ui.centered_and_justified(|ui| {
                                        ui.label(
                                            egui::RichText::new(letter)
                                                .monospace()
                                                .strong()
                                                .color(hdr_col),
                                        );
                                    });
                                }
                                ui.end_row();

                                // Data rows
                                for r in 0..rows {
                                    ui.label(
                                        egui::RichText::new(format!("{}", r + 1))
                                            .monospace()
                                            .strong()
                                            .color(hdr_col),
                                    );
                                    for c in 0..cols {
                                        let mut val = app.document.get_spreadsheet_cell(r, c);
                                        let resp = ui.add_sized(
                                            [52.0, 18.0],
                                            egui::TextEdit::singleline(&mut val)
                                                .font(egui::TextStyle::Monospace)
                                                .text_color(text_col)
                                                .horizontal_align(egui::Align::Center),
                                        );
                                        if resp.changed() {
                                            app.save_state();
                                            if let Err(e) =
                                                app.document.set_spreadsheet_cell(r, c, val.clone())
                                            {
                                                log::warn!("set_spreadsheet_cell: {}", e);
                                            }
                                            if let Ok((x, y)) = commands::parse_point_str(&val) {
                                                app.document.add_object(GeoObject::Point(
                                                    PointObj::new(Point2::new(x, y)).with_label(
                                                        format!(
                                                            "{}{}",
                                                            (b'A' + c as u8) as char,
                                                            r + 1
                                                        ),
                                                    ),
                                                ));
                                            }
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
    }
}
