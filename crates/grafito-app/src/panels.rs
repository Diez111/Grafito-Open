//! Removable side panels: CAS, view settings, spreadsheet, and value table.

use crate::{commands, GrafitoApp};
use egui::Color32;
use grafito_core::{GeoObject, ObjectId, PointObj};
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

fn draw_object_cards_where(
    ui: &mut egui::Ui,
    app: &mut GrafitoApp,
    title: &str,
    empty_text: &str,
    predicate: impl Fn(&GeoObject) -> bool,
) {
    let theme = current_theme(ui.ctx());
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .color(theme.text_secondary)
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);

    let ids: Vec<ObjectId> = app
        .document
        .objects_iter()
        .filter_map(|(id, obj)| predicate(obj).then_some(*id))
        .collect();

    if ids.is_empty() {
        ui.label(
            egui::RichText::new(empty_text)
                .color(theme.text_tertiary)
                .size(11.0),
        );
        return;
    }

    for id in ids {
        crate::algebra::draw_object_card(ui, app, id);
    }
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
                    let time = ui.ctx().input(|i| i.time);
                    app.submit_input_text(time);
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
                // El toggle 2D/3D vive en el selector de perspectivas del sidebar
                // (Geometría 2D / Geometría 3D). No duplicar aquí, era fuente de
                // estado Frankenstein (canvas 3D + toolbar 2D).

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

/// Panel derecho: Animación trigonométrica (círculo unitario + gráfico sin/cos).
///
/// Muestra un círculo unitario a la derecha con un vector radio en ángulo `t`
/// y debajo el gráfico 2D de sin(t) o cos(t) con una línea vertical marcando
/// el ángulo actual. La animación se controla con play/pause y slider de velocidad.
pub(crate) fn draw_trig_animation_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_trig_animation")
        .default_width(300.0)
        .min_width(240.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Animación Trigonométrica")
                    .color(accent)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(6.0);

            // Selector de función
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Función:").color(hdr_col).size(12.0));
                if ui
                    .selectable_label(app.trig_function == 0, "sin(t)")
                    .clicked()
                {
                    app.trig_function = 0;
                }
                if ui
                    .selectable_label(app.trig_function == 1, "cos(t)")
                    .clicked()
                {
                    app.trig_function = 1;
                }
                if ui
                    .selectable_label(app.trig_function == 2, "tan(t)")
                    .clicked()
                {
                    app.trig_function = 2;
                }
            });

            ui.add_space(4.0);

            // Controles de animación
            ui.horizontal(|ui| {
                if ui
                    .button(if app.trig_animating {
                        "⏸ Pausar"
                    } else {
                        "▶ Iniciar"
                    })
                    .clicked()
                {
                    app.trig_animating = !app.trig_animating;
                }
                ui.label(egui::RichText::new("Velocidad:").color(txt_dim).size(11.0));
                ui.add(
                    egui::Slider::new(&mut app.trig_speed, -3.0..=3.0)
                        .fixed_decimals(1)
                        .suffix(" rad/s"),
                );
            });

            ui.add_space(4.0);

            // Slider manual del ángulo
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Ángulo:").color(txt_dim).size(11.0));
                ui.add(
                    egui::Slider::new(
                        &mut app.trig_angle,
                        -2.0 * std::f64::consts::PI..=2.0 * std::f64::consts::PI,
                    )
                    .fixed_decimals(2)
                    .suffix(" rad"),
                );
            });

            // Valor actual
            let t = app.trig_angle;
            let (fn_name, fn_val) = match app.trig_function {
                0 => ("sin", t.sin()),
                1 => ("cos", t.cos()),
                _ => ("tan", t.tan()),
            };
            ui.label(
                egui::RichText::new(format!("{}({:.2}) = {:.4}", fn_name, t, fn_val))
                    .color(accent)
                    .size(12.0),
            );

            ui.add_space(8.0);

            // Dibujar círculo unitario
            let circle_size = 160.0;
            let (circle_resp, painter) = ui.allocate_painter(
                egui::Vec2::new(circle_size, circle_size),
                egui::Sense::hover(),
            );
            let center = circle_resp.rect.center();
            let radius = circle_size * 0.4;

            // Ejes
            painter.line_segment(
                [
                    egui::pos2(center.x - radius - 10.0, center.y),
                    egui::pos2(center.x + radius + 10.0, center.y),
                ],
                egui::Stroke::new(1.0, sep_col),
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y - radius - 10.0),
                    egui::pos2(center.x, center.y + radius + 10.0),
                ],
                egui::Stroke::new(1.0, sep_col),
            );

            // Círculo
            painter.circle_stroke(center, radius, egui::Stroke::new(1.5, accent));

            // Punto en el círculo
            let t_f32 = t as f32;
            let px = center.x + radius * t_f32.cos();
            let py = center.y - radius * t_f32.sin(); // invertir Y (pantalla vs matemática)
            let point_color = egui::Color32::from_rgb(255, 100, 100);

            // Vector radio
            painter.line_segment(
                [center, egui::pos2(px, py)],
                egui::Stroke::new(2.0, point_color),
            );

            // Línea de proyección a eje X (coseno)
            painter.line_segment(
                [egui::pos2(px, py), egui::pos2(px, center.y)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 100)),
            );

            // Línea de proyección a eje Y (seno)
            painter.line_segment(
                [egui::pos2(px, py), egui::pos2(center.x, py)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 255)),
            );

            // Punto
            painter.circle_filled(egui::pos2(px, py), 4.0, point_color);

            // Etiquetas
            painter.text(
                egui::pos2(center.x + radius + 12.0, center.y),
                egui::Align2::LEFT_CENTER,
                "cos",
                egui::FontId::proportional(10.0),
                txt_dim,
            );
            painter.text(
                egui::pos2(center.x, center.y - radius - 12.0),
                egui::Align2::CENTER_BOTTOM,
                "sin",
                egui::FontId::proportional(10.0),
                txt_dim,
            );

            ui.add_space(8.0);

            // Dibujar gráfico 2D de la función
            let graph_h = 120.0;
            let graph_w = circle_size;
            let (graph_resp, graph_painter) =
                ui.allocate_painter(egui::Vec2::new(graph_w, graph_h), egui::Sense::hover());
            let graph_rect = graph_resp.rect;
            let gx_min = graph_rect.left();
            let gx_max = graph_rect.right();
            let gy_min = graph_rect.top();
            let gy_max = graph_rect.bottom();
            let gcy = graph_rect.center().y;

            // Eje X
            graph_painter.line_segment(
                [egui::pos2(gx_min, gcy), egui::pos2(gx_max, gcy)],
                egui::Stroke::new(1.0, sep_col),
            );

            // Mapear t ∈ [-2π, 2π] a x ∈ [gx_min, gx_max]
            let two_pi = 2.0 * std::f64::consts::PI;
            let graph_w_f64 = (gx_max - gx_min) as f64;
            let graph_h_f64 = graph_h as f64;
            let t_to_x =
                |tt: f64| -> f32 { gx_min + ((tt + two_pi) / (2.0 * two_pi) * graph_w_f64) as f32 };
            // Mapear y ∈ [-1, 1] a [gy_max, gy_min] (invertido)
            let y_to_screen = |yy: f64| -> f32 { gcy - (yy * graph_h_f64 * 0.4) as f32 };

            // Dibujar curva
            let mut prev: Option<egui::Pos2> = None;
            for i in 0..=200 {
                let tt = -two_pi + i as f64 / 200.0 * 2.0 * two_pi;
                let yy = match app.trig_function {
                    0 => tt.sin(),
                    1 => tt.cos(),
                    _ => {
                        let v = tt.tan();
                        if v.abs() > 3.0 {
                            f64::NAN
                        } else {
                            v
                        }
                    }
                };
                if yy.is_finite() {
                    let p = egui::pos2(t_to_x(tt), y_to_screen(yy));
                    if let Some(pp) = prev {
                        graph_painter.line_segment([pp, p], egui::Stroke::new(1.5, accent));
                    }
                    prev = Some(p);
                } else {
                    prev = None;
                }
            }

            // Línea vertical en t actual
            let tx = t_to_x(t);
            graph_painter.line_segment(
                [egui::pos2(tx, gy_min), egui::pos2(tx, gy_max)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 100, 100)),
            );

            // Punto en la curva
            if fn_val.is_finite() && fn_val.abs() <= 3.0 {
                graph_painter.circle_filled(
                    egui::pos2(tx, y_to_screen(fn_val)),
                    3.0,
                    egui::Color32::from_rgb(255, 100, 100),
                );
            }

            // Etiqueta del eje
            graph_painter.text(
                egui::pos2(gx_max - 5.0, gcy + 5.0),
                egui::Align2::RIGHT_TOP,
                "t",
                egui::FontId::proportional(10.0),
                txt_dim,
            );
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

// ══════════════════════════════════════════════════════════════════════════
// Paneles izquierdos específicos por perspectiva (Fase 2)
// ══════════════════════════════════════════════════════════════════════════

/// Panel izquierdo de Estadística. Permite ingresar datos y ver resumen.
pub(crate) fn draw_statistics_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("stats_panel")
        .default_width(240.0)
        .min_width(180.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Estadística")
                    .color(accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(8.0);

            draw_object_cards_where(
                ui,
                app,
                "Objetos estadísticos",
                "Sin gráficos estadísticos.\nProbá Histogram[...] o ScatterPlot[...].",
                |obj| {
                    matches!(
                        obj,
                        GeoObject::Histogram(_)
                            | GeoObject::ScatterPlot(_)
                            | GeoObject::BoxPlot(_)
                            | GeoObject::RegressionLine(_)
                            | GeoObject::Function(_)
                    )
                },
            );
            ui.add_space(8.0);

            // ── Datos: TextEdit vinculado al buffer persistente ──
            // El buffer sólo se parsea al perder foco o al apretar "Aplicar"
            // — antes, el editor reconstruí el string cada frame desde los
            // valores parseados y destruía la entrada del usuario por cada
            // coma en blanco o no-número temporal.
            ui.label(
                egui::RichText::new("Datos (uno por línea o coma):")
                    .color(hdr_col)
                    .size(12.0),
            );
            let te_resp = ui.add_sized(
                [ui.available_width(), 80.0],
                egui::TextEdit::multiline(&mut app.statistics_input_buf).desired_rows(3),
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let apply_clicked = ui.button("Aplicar").clicked();
                let lost_focus =
                    te_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if apply_clicked || lost_focus {
                    let parsed: Vec<f64> = app
                        .statistics_input_buf
                        .split([',', '\n'])
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    if parsed != app.statistics_data {
                        app.statistics_data = parsed;
                        app.document.bump_version();
                    }
                }
                if ui.button("Limpiar").clicked() {
                    app.statistics_input_buf.clear();
                    app.statistics_data.clear();
                    app.document.bump_version();
                }
            });

            ui.add_space(8.0);
            if app.statistics_data.is_empty() {
                // Empty-state
                ui.label(
                    egui::RichText::new(
                        "Ingresá datos arriba (uno por línea o comas)\n\
                         y pulsá «Aplicar» para ver el resumen y el\n\
                         histograma.\n\
                         Ejemplo: 1, 2, 3, 5, 4, 6",
                    )
                    .color(txt_dim)
                    .size(11.0),
                );
            } else {
                let data = &app.statistics_data;
                let n = data.len() as f64;
                let sum: f64 = data.iter().sum();
                let mean = sum / n;
                let var = data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
                let std = var.sqrt();
                let mut sorted = data.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = if sorted.len() % 2 == 0 {
                    let m = sorted.len() / 2;
                    (sorted[m - 1] + sorted[m]) / 2.0
                } else {
                    sorted[sorted.len() / 2]
                };
                let mn = data.iter().cloned().fold(f64::INFINITY, f64::min);
                let mx = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let range = mx - mn;
                // Cuartiles por interpolación lineal.
                let q = |p: f64| -> f64 {
                    let pos = p * (sorted.len() as f64 - 1.0);
                    let lo = pos.floor() as usize;
                    let hi = (lo + 1).min(sorted.len() - 1);
                    let frac = pos - lo as f64;
                    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
                };
                let q1 = q(0.25);
                let q3 = q(0.75);
                let iqr = q3 - q1;

                ui.label(
                    egui::RichText::new("Resumen")
                        .color(hdr_col)
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(4.0);
                egui::Grid::new("stats_grid")
                    .num_columns(2)
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        let mut row = |k: &str, v: String| {
                            ui.label(egui::RichText::new(k).color(txt_dim).size(12.0));
                            ui.label(egui::RichText::new(v).color(txt_col).size(12.0).strong());
                            ui.end_row();
                        };
                        row("N", format!("{}", data.len()));
                        row("Suma", format!("{:.3}", sum));
                        row("Media", format!("{:.3}", mean));
                        row("Mediana", format!("{:.3}", median));
                        row("Desvío", format!("{:.3}", std));
                        row("Varianza", format!("{:.3}", var));
                        row("Mín", format!("{:.3}", mn));
                        row("Máx", format!("{:.3}", mx));
                        row("Rango", format!("{:.3}", range));
                        row("Q1", format!("{:.3}", q1));
                        row("Q3", format!("{:.3}", q3));
                        row("IQR", format!("{:.3}", iqr));
                    });

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Histograma")
                        .color(hdr_col)
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(2.0);
                let bins = 10usize;
                let bw = range.max(1e-9) / bins as f64;
                let mut counts = vec![0u32; bins];
                for v in data {
                    let idx = (((v - mn) / bw).floor() as usize).min(bins - 1);
                    counts[idx] += 1;
                }
                let max_c = (*counts.iter().max().unwrap_or(&1)).max(1) as f32;
                let hist_h = 90.0;
                let (hist_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), hist_h + 14.0),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(hist_rect) {
                    let painter = ui.painter();
                    let plot = hist_rect.shrink2(egui::vec2(2.0, 2.0));
                    // Plot area interna
                    let plot_top = plot.min.y;
                    let plot_bot = plot.max.y - 14.0;
                    let plot_h = plot_bot - plot_top;
                    let plot_w = plot.width();
                    // Ejes: línea base
                    painter.line_segment(
                        [
                            egui::pos2(plot.min.x, plot_bot),
                            egui::pos2(plot.max.x, plot_bot),
                        ],
                        egui::Stroke::new(1.0, sep_col),
                    );
                    // Barras
                    let bar_w = plot_w / bins as f32;
                    for (i, c) in counts.iter().enumerate() {
                        let h = (*c as f32 / max_c) * plot_h;
                        let bar = egui::Rect::from_min_size(
                            egui::pos2(plot.min.x + i as f32 * bar_w + 2.0, plot_bot - h),
                            egui::vec2(bar_w - 4.0, h),
                        );
                        painter.rect_filled(bar, 2.0, accent);
                        // count label encima si > 0
                        if *c > 0 {
                            painter.text(
                                egui::pos2(bar.center().x, bar.min.y - 6.0),
                                egui::Align2::CENTER_BOTTOM,
                                c.to_string(),
                                egui::FontId::proportional(9.0),
                                txt_dim,
                            );
                        }
                    }
                    // Etiquetas min/max en el eje
                    painter.text(
                        egui::pos2(plot.min.x, plot_bot + 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{:.2}", mn),
                        egui::FontId::proportional(9.0),
                        txt_dim,
                    );
                    painter.text(
                        egui::pos2(plot.max.x, plot_bot + 2.0),
                        egui::Align2::RIGHT_TOP,
                        format!("{:.2}", mx),
                        egui::FontId::proportional(9.0),
                        txt_dim,
                    );
                }
            }
        });
}

/// Panel izquierdo de Complejos. Lista objetos complejos y permite cambiar
/// el símbolo base.
pub(crate) fn draw_complex_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::{GeoObject, ObjectId};
    let (_is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("complex_panel")
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
            ui.label(
                egui::RichText::new("Números Complejos")
                    .color(accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(8.0);

            // ── Barra de entrada in-panel (igual que Álgebra) ──
            egui::Frame::none()
                .fill(current_theme(ctx).input_bg)
                .inner_margin(egui::Margin {
                    left: 8.0,
                    right: 8.0,
                    top: 6.0,
                    bottom: 6.0,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                        ui.add_space(3.0);
                        let r = ui.add_sized(
                            [ui.available_width(), 22.0],
                            egui::TextEdit::singleline(&mut app.input_text)
                                .hint_text("ComplexGrid[1/z]...")
                                .frame(false)
                                .text_color(txt_col),
                        );
                        if r.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !app.input_text.is_empty()
                        {
                            let time = ui.ctx().input(|i| i.time);
                            app.submit_input_text(time);
                        }
                    });
                });
            ui.add(egui::Separator::default().spacing(0.0));
            ui.add_space(8.0);

            // ── Símbolo base ──
            ui.label(
                egui::RichText::new("Símbolo base")
                    .color(hdr_col)
                    .size(12.0),
            );
            let mut sym = app.document.complex_base_symbol.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut sym)
                    .desired_width(ui.available_width())
                    .hint_text("z"),
            );
            if resp.lost_focus() && sym.trim() != app.document.complex_base_symbol {
                let new_sym = sym.trim().to_string();
                if !new_sym.is_empty() {
                    app.document.migrate_complex_symbol(&new_sym);
                    app.document.bump_version();
                }
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Objetos")
                    .color(hdr_col)
                    .size(12.0)
                    .strong(),
            );
            ui.add_space(4.0);
            let ids: Vec<ObjectId> = app.document.objects_iter().map(|(id, _)| *id).collect();
            let mut any_object = false;
            for id in &ids {
                let Some(obj) = app.document.get_object(*id) else {
                    continue;
                };
                if !matches!(
                    obj,
                    GeoObject::Function(_)
                        | GeoObject::ImplicitCurve(_)
                        | GeoObject::ParametricCurve2D(_)
                        | GeoObject::PolarCurve(_)
                        | GeoObject::VectorField2D(_)
                        | GeoObject::ComplexGrid(_)
                        | GeoObject::ComplexMapping(_)
                        | GeoObject::Point(_)
                        | GeoObject::Line(_)
                        | GeoObject::Circle(_)
                        | GeoObject::Polygon(_)
                        | GeoObject::Ellipse(_)
                        | GeoObject::Parabola(_)
                        | GeoObject::Hyperbola(_)
                ) {
                    continue;
                }
                any_object = true;
                crate::algebra::draw_object_card(ui, app, *id);
            }
            if !any_object {
                ui.label(
                    egui::RichText::new("Sin objetos.\nProbá: x^2 + y^2 < 1\no: ComplexGrid[1/z]")
                        .color(txt_dim)
                        .size(11.0),
                );
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Comandos rápidos")
                    .color(hdr_col)
                    .size(12.0)
                    .strong(),
            );
            ui.add_space(2.0);
            // Hints clickeables que rellenan la barra de entrada del panel.
            let hints: &[(&str, &str)] = &[
                ("ComplexGrid[1/z]", "ComplexGrid[1/z]"),
                ("ComplexMapping[1/z, I]", "ComplexMapping[1/z, I]"),
                ("ComplexGrid[exp(z)]", "ComplexGrid[exp(z)]"),
                ("ComplexSymbol[w]", "ComplexSymbol[w]"),
            ];
            for (label, payload) in hints {
                let b = ui.add(
                    egui::Button::new(
                        egui::RichText::new(*label)
                            .monospace()
                            .size(11.0)
                            .color(txt_col),
                    )
                    .frame(false),
                );
                if b.clicked() {
                    app.input_text = payload.to_string();
                }
                b.on_hover_text(format!("Click para cargar: {}", payload));
            }
        });
}

/// Panel izquierdo de Atractores y Dinámica.
pub(crate) fn draw_attractor_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::GeoObject;
    let (_is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::left("attractor_panel")
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
            ui.label(
                egui::RichText::new("Dinámica y Atractores")
                    .color(accent)
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(8.0);

            draw_object_cards_where(
                ui,
                app,
                "Objetos dinámicos",
                "Sin objetos dinámicos.\nProbá Attractor[10, 28, 8/3].",
                |obj| {
                    matches!(
                        obj,
                        GeoObject::Attractor3D(_)
                            | GeoObject::PhasePortrait(_)
                            | GeoObject::VectorField2D(_)
                            | GeoObject::VectorField3D(_)
                    )
                },
            );
            ui.add_space(8.0);

            let ids: Vec<_> = app.document.objects_iter().map(|(id, _)| *id).collect();
            let mut attractor_id = None;
            for id in &ids {
                if let Some(GeoObject::Attractor3D(_)) = app.document.get_object(*id) {
                    attractor_id = Some(*id);
                    break;
                }
            }

            if let Some(id) = attractor_id {
                ui.label(egui::RichText::new("Attractor activo").color(hdr_col).size(12.0).strong());
                if let Some(GeoObject::Attractor3D(a)) = app.document.get_object(id) {
                    let sigma = a.params.first().copied().unwrap_or(0.0);
                    let rho = a.params.get(1).copied().unwrap_or(0.0);
                    let beta = a.params.get(2).copied().unwrap_or(0.0);
                    ui.label(format!("σ = {:.3}", sigma));
                    ui.label(format!("ρ = {:.3}", rho));
                    ui.label(format!("β = {:.3}", beta));
                    ui.label(format!("dt = {:.4}", a.dt));
                    ui.label(format!("pasos = {}", a.steps));
                }
            } else {
                ui.label(
                    egui::RichText::new(
                        "Sin attractor activo.\nCreá uno con:\n  Attractor[σ, ρ, β]\n(Lorenz por defecto)",
                    )
                    .color(txt_dim)
                    .size(11.0),
                );
                ui.add_space(6.0);
                if ui
                    .button(egui::RichText::new("Crear Lorenz por defecto").color(accent).strong())
                    .clicked()
                {
                    app.save_state();
                    app.execute_command_and_record("Attractor[10, 28, 8/3]", 0.0);
                }
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Comandos").color(hdr_col).size(12.0).strong());
            ui.label(egui::RichText::new("• Lorenz: Attractor[σ, ρ, β]").color(txt_dim).size(11.0).monospace());
            let _ = txt_col;
        });
}

// ══════════════════════════════════════════════════════════════════════════
// Paneles derechos (Fase 3)
// ══════════════════════════════════════════════════════════════════════════

/// Panel derecho: Propiedades del objeto seleccionado (Geometry3D).
pub(crate) fn draw_right_properties_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::GeoObject;
    let (is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_properties")
        .default_width(280.0)
        .min_width(200.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Propiedades")
                    .color(accent)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(6.0);

            let Some(id) = app.selected_object else {
                ui.label(
                    egui::RichText::new(
                        "Seleccioná un objeto del canvas para ver/editar sus propiedades.",
                    )
                    .color(txt_dim)
                    .size(11.0),
                );
                return;
            };
            let Some(obj) = app.document.get_object_mut(id) else {
                ui.label(egui::RichText::new("Objeto inexistente.").color(txt_dim));
                return;
            };

            let label_col = if is_dark {
                Color32::from_gray(180)
            } else {
                Color32::from_gray(60)
            };
            match obj {
                GeoObject::Cube3D(c) => {
                    ui.label(egui::RichText::new("Cubo 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", c.label)).color(txt_col));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Centro").color(txt_dim));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut c.center.x)
                                .speed(0.1)
                                .prefix("x="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut c.center.y)
                                .speed(0.1)
                                .prefix("y="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut c.center.z)
                                .speed(0.1)
                                .prefix("z="),
                        );
                    });
                    ui.add(egui::Slider::new(&mut c.size, 0.1..=10.0).text("tamaño"));
                }
                GeoObject::Sphere3D(s) => {
                    ui.label(egui::RichText::new("Esfera 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", s.label)).color(txt_col));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Centro").color(txt_dim));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut s.center.x)
                                .speed(0.1)
                                .prefix("x="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut s.center.y)
                                .speed(0.1)
                                .prefix("y="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut s.center.z)
                                .speed(0.1)
                                .prefix("z="),
                        );
                    });
                    ui.add(egui::Slider::new(&mut s.radius, 0.1..=10.0).text("radio"));
                }
                GeoObject::Point3D(p) => {
                    ui.label(egui::RichText::new("Punto 3D").color(label_col).strong());
                    ui.label(egui::RichText::new(format!("Etiqueta: {}", p.label)).color(txt_col));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut p.position.x)
                                .speed(0.1)
                                .prefix("x="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut p.position.y)
                                .speed(0.1)
                                .prefix("y="),
                        );
                        ui.add(
                            egui::DragValue::new(&mut p.position.z)
                                .speed(0.1)
                                .prefix("z="),
                        );
                    });
                    ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).text("tamaño"));
                }
                other => {
                    ui.label(format!("Tipo: {}", other.name()));
                    ui.label(
                        egui::RichText::new("Propiedades dedicadas en panel de Álgebra.")
                            .color(txt_dim)
                            .size(11.0),
                    );
                }
            }
            app.document.bump_version();
        });
}

/// Panel derecho: Tabla de valores x|f(x) (AlgebraCas, Calculus).
pub(crate) fn draw_right_table_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let (_is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_table")
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
            ui.label(
                egui::RichText::new("Tabla de valores")
                    .color(accent)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(6.0);

            let funcs: Vec<_> = app
                .document
                .objects_iter()
                .filter_map(|(_, o)| {
                    if let grafito_core::GeoObject::Function(f) = o {
                        Some((f.label.clone(), f.expr.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            if funcs.is_empty() {
                ui.label(egui::RichText::new("Sin funciones en el documento.").color(txt_dim));
                return;
            }

            // Selector de función
            ui.horizontal(|ui| {
                ui.label("Función:");
                let max_idx = funcs.len().saturating_sub(1);
                app.table_func_idx = app.table_func_idx.min(max_idx);
                ui.add(
                    egui::Slider::new(&mut app.table_func_idx, 0..=max_idx)
                        .text("")
                        .max_decimals(0),
                );
                ui.label(&funcs[app.table_func_idx].0);
            });
            ui.add_space(6.0);

            egui::Grid::new("right_table_params").show(ui, |ui| {
                ui.label("x min");
                ui.text_edit_singleline(&mut app.table_x_min);
                ui.end_row();
                ui.label("x max");
                ui.text_edit_singleline(&mut app.table_x_max);
                ui.end_row();
                ui.label("step");
                ui.text_edit_singleline(&mut app.table_step);
                ui.end_row();
            });
            ui.add_space(6.0);

            let x_min: f64 = app.table_x_min.trim().parse().unwrap_or(-5.0);
            let x_max: f64 = app.table_x_max.trim().parse().unwrap_or(5.0);
            let step: f64 = app.table_step.trim().parse().unwrap_or(0.5);

            if x_max <= x_min || step <= 0.0 {
                ui.label(egui::RichText::new("Rango/step inválidos.").color(txt_dim));
                return;
            }

            let expr = funcs[app.table_func_idx].1.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("right_table_values")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("x").strong().color(txt_col));
                        ui.label(egui::RichText::new("f(x)").strong().color(txt_col));
                        ui.end_row();
                        let mut x = x_min;
                        while x <= x_max + 1e-9 {
                            let y =
                                grafito_geometry::expr::evaluate(&expr, &[("x".to_string(), x)])
                                    .ok();
                            ui.label(format!("{:.3}", x));
                            ui.label(match y {
                                Some(v) => format!("{:.3}", v),
                                None => "—".to_string(),
                            });
                            ui.end_row();
                            x += step;
                        }
                    });
            });
        });
}

/// Panel derecho: Coloración de dominio (Complejos).
pub(crate) fn draw_right_domain_coloring_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::GeoObject;
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_domain_coloring")
        .default_width(280.0)
        .min_width(200.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(alg_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Coloración de dominio")
                    .color(accent)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(6.0);

            let mut has_grid = false;
            for (_, obj) in app.document.objects_iter() {
                if matches!(obj, GeoObject::ComplexGrid(_)) {
                    has_grid = true;
                    break;
                }
            }

            if !has_grid {
                ui.label(
                    egui::RichText::new(
                        "Sin ComplexGrid. Creá uno con:\n  ComplexGrid[1/z]\nColoración por fase de f(z).",
                    )
                    .color(txt_dim)
                    .size(11.0),
                );
            } else {
                ui.label(egui::RichText::new("Coloración por fase habilitada").color(hdr_col).strong());
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Tono = arg(f(z)).").color(txt_dim).size(11.0));
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Símbolo base").color(hdr_col).size(12.0).strong());
            let mut sym = app.document.complex_base_symbol.clone();
            let r = ui.add(
                egui::TextEdit::singleline(&mut sym)
                    .desired_width(ui.available_width())
                    .hint_text("z"),
            );
            if r.lost_focus() && sym.trim() != app.document.complex_base_symbol {
                let new_sym = sym.trim().to_string();
                if !new_sym.is_empty() {
                    app.document.migrate_complex_symbol(&new_sym);
                    app.document.bump_version();
                }
            }
        });
}

/// Panel derecho: Parámetros del attractor activo (Dynamics).
pub(crate) fn draw_right_parameters_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    use grafito_core::{GeoObject, ObjectId};
    let (_is_dark, accent, alg_fill, sep_col, _txt_col, txt_dim, hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("right_parameters")
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
            ui.label(
                egui::RichText::new("Parámetros dinámicos")
                    .color(accent)
                    .size(14.0)
                    .strong(),
            );
            ui.add_space(6.0);

            let mut attractor_id: Option<ObjectId> = None;
            for (id, obj) in app.document.objects_iter() {
                if matches!(obj, GeoObject::Attractor3D(_)) {
                    attractor_id = Some(*id);
                    break;
                }
            }

            let Some(id) = attractor_id else {
                ui.label(
                    egui::RichText::new(
                        "Sin attractor activo. Creá uno con:\n  Attractor[10, 28, 8/3]",
                    )
                    .color(txt_dim)
                    .size(11.0),
                );
                return;
            };

            let Some(obj) = app.document.get_object_mut(id) else {
                return;
            };
            if let GeoObject::Attractor3D(a) = obj {
                // params: [sigma, rho, beta] para Lorenz. Lo dejamos genérico.
                while a.params.len() < 3 {
                    a.params.push(0.0);
                }
                let mut sigma = a.params[0];
                let mut rho = a.params[1];
                let mut beta = a.params[2];
                ui.label(
                    egui::RichText::new("Lorenz σ, ρ, β")
                        .color(hdr_col)
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.add(
                    egui::Slider::new(&mut sigma, 0.1..=30.0)
                        .text("σ")
                        .trailing_fill(true),
                );
                ui.add(
                    egui::Slider::new(&mut rho, 0.1..=60.0)
                        .text("ρ")
                        .trailing_fill(true),
                );
                ui.add(
                    egui::Slider::new(&mut beta, 0.1..=10.0)
                        .text("β")
                        .trailing_fill(true),
                );
                a.params[0] = sigma;
                a.params[1] = rho;
                a.params[2] = beta;
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Integración")
                        .color(hdr_col)
                        .size(12.0)
                        .strong(),
                );
                ui.add(
                    egui::Slider::new(&mut a.dt, 0.001..=0.05)
                        .text("dt")
                        .trailing_fill(true),
                );
                ui.add(
                    egui::Slider::new(&mut a.steps, 100..=20000)
                        .text("pasos")
                        .trailing_fill(true)
                        .integer(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("El canvas se regenera cada cambio.")
                        .color(txt_dim)
                        .size(11.0),
                );
                app.document.bump_version();
            }
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

// ─────────────────────────────────────────────────────────────────────────
// Protocolo de Construcción (panel derecho, perspectiva Geometry2D)
// ─────────────────────────────────────────────────────────────────────────

/// Escapa caracteres especiales de LaTeX.
fn escape_latex(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('_', "\\_")
        .replace('%', "\\%")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('$', "\\$")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

/// Genera una lista enumerada en LaTeX a partir del registro de construcción.
fn construction_log_to_latex(log: &[crate::app::ConstructionStep]) -> String {
    let mut s = String::new();
    s.push_str("% Protocolo de Construcción — Grafito\n");
    s.push_str("\\begin{enumerate}\n");
    for step in log {
        let inputs = if step.inputs.is_empty() {
            "\\textemdash".to_string()
        } else {
            step.inputs.join(", ")
        };
        let output = if step.output.is_empty() {
            "\\textemdash".to_string()
        } else {
            step.output.clone()
        };
        let disabled = if step.disabled {
            " (deshabilitado)"
        } else {
            ""
        };
        s.push_str(&format!(
            "  \\item {}{}: {} $\\rightarrow$ {}\n",
            escape_latex(&step.action),
            disabled,
            escape_latex(&inputs),
            escape_latex(&output),
        ));
    }
    s.push_str("\\end{enumerate}\n");
    s
}

pub(crate) fn draw_construction_protocol(app: &mut GrafitoApp, ctx: &egui::Context) {
    if !app.show_construction_protocol {
        return;
    }
    let (_is_dark, accent, alg_fill, sep_col, txt_col, txt_dim, _hdr_col) = panel_theme_local(ctx);

    egui::SidePanel::right("construction_protocol")
        .resizable(true)
        .default_width(300.0)
        .min_width(200.0)
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
                    egui::RichText::new("Protocolo de Construcción")
                        .color(accent)
                        .strong()
                        .size(15.0),
                );
            });
            ui.add_space(2.0);
            ui.separator();

            // Toolbar: exportar LaTeX + limpiar
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Exportar LaTeX").clicked() {
                            let latex = construction_log_to_latex(&app.construction_log);
                            if let Some(path) =
                                rfd::FileDialog::new().add_filter("TeX", &["tex"]).save_file()
                            {
                                if let Err(e) = std::fs::write(&path, latex) {
                                    app.toasts.push(
                                        format!("Error LaTeX: {}", e),
                                        grafito_ui::toast::ToastKind::Error,
                                        5.0,
                                    );
                                } else {
                                    app.toasts.push(
                                        "Protocolo exportado a LaTeX".to_string(),
                                        grafito_ui::toast::ToastKind::Success,
                                        3.0,
                                    );
                                }
                            }
                        }
                        if ui.button("Limpiar").clicked() {
                            app.construction_log.clear();
                        }
                    });
                });
            ui.separator();

            // Lista de pasos con botones up/down y habilitar/deshabilitar.
            let mut move_idx: Option<(usize, i32)> = None;
            let mut toggle_idx: Option<usize> = None;
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 8.0)
                .show(ui, |ui| {
                    if app.construction_log.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Sin pasos de construcción.\nCrea objetos o restricciones para verlos aquí.",
                            )
                            .size(12.0)
                            .color(txt_dim),
                        );
                    } else {
                        let total = app.construction_log.len();
                        for i in 0..total {
                            let (n, action, inputs, output, disabled) = {
                                let step = &app.construction_log[i];
                                (
                                    step.n,
                                    step.action.clone(),
                                    step.inputs.clone(),
                                    step.output.clone(),
                                    step.disabled,
                                )
                            };
                            let inputs_str =
                                if inputs.is_empty() { "—".to_string() } else { inputs.join(", ") };
                            let output_str =
                                if output.is_empty() { "—".to_string() } else { output };
                            let bg = if disabled {
                                Color32::from_gray(60)
                            } else {
                                Color32::TRANSPARENT
                            };
                            egui::Frame::none()
                                .fill(bg)
                                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{}", n))
                                                .color(accent)
                                                .strong(),
                                        );
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&action)
                                                        .color(txt_col)
                                                        .strong()
                                                        .size(12.0),
                                                );
                                                if disabled {
                                                    ui.label(
                                                        egui::RichText::new("(deshabilitado)")
                                                            .color(txt_dim)
                                                            .size(10.0),
                                                    );
                                                }
                                            });
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} → {}",
                                                    inputs_str, output_str
                                                ))
                                                .size(11.0)
                                                .color(txt_dim),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let up_enabled = i > 0;
                                                let down_enabled = i + 1 < total;
                                                if ui
                                                    .add_enabled(
                                                        up_enabled,
                                                        egui::Button::new("↑"),
                                                    )
                                                    .clicked()
                                                {
                                                    move_idx = Some((i, -1));
                                                }
                                                if ui
                                                    .add_enabled(
                                                        down_enabled,
                                                        egui::Button::new("↓"),
                                                    )
                                                    .clicked()
                                                {
                                                    move_idx = Some((i, 1));
                                                }
                                                if ui
                                                    .button(if disabled { "✓" } else { "✕" })
                                                    .clicked()
                                                {
                                                    toggle_idx = Some(i);
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(2.0);
                        }
                    }
                });

            // Aplicar reordenar / toggle tras iterar (no se puede mutar
            // mientras se itera con borrow inmutable).
            if let Some((i, dir)) = move_idx {
                let j = (i as i32 + dir).max(0) as usize;
                if j < app.construction_log.len() {
                    app.construction_log.swap(i, j);
                    for (k, step) in app.construction_log.iter_mut().enumerate() {
                        step.n = k + 1;
                    }
                }
            }
            if let Some(i) = toggle_idx {
                if let Some(step) = app.construction_log.get_mut(i) {
                    step.disabled = !step.disabled;
                }
            }
        });
}
