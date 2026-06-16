use crate::GrafitoApp;
use egui::{Color32, Ui};
use grafito_ui::Tool;

pub fn draw_tools_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let is_dark = app.dark_mode;
    let panel_fill = if is_dark {
        Color32::from_rgb(30, 30, 38)
    } else {
        Color32::from_rgb(250, 250, 252)
    };

    egui::SidePanel::left("tools_panel")
        .default_width(320.0)
        .frame(egui::Frame::none().fill(panel_fill).inner_margin(12.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Herramientas").strong().size(18.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("X").clicked() {
                        app.sidebar_tab = 0; // Return to Algebra
                    }
                });
            });
            ui.add_space(8.0);

            let sep_col = if is_dark {
                Color32::from_gray(50)
            } else {
                Color32::from_gray(220)
            };
            ui.painter().line_segment(
                [
                    ui.cursor().min,
                    ui.cursor().min + egui::vec2(ui.available_width(), 0.0),
                ],
                egui::Stroke::new(1.0, sep_col),
            );
            ui.add_space(12.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let is_3d = app.current_view == crate::ViewMode::D3;

                // ── BÁSICAS ──
                let mut basic_tools = vec![(Tool::Select, "Mover", "Arrastra objetos")];
                if is_3d {
                    basic_tools.push((Tool::Point3D, "Punto 3D", "Punto en el espacio"));
                } else {
                    basic_tools.push((Tool::Point, "Punto", "Crea un punto nuevo"));
                }
                basic_tools.push((Tool::Slider, "Deslizador", "Variable numérica dinámica"));
                draw_tool_group(ui, app, "Básicas", &basic_tools);

                // ── EDICIÓN ──
                draw_tool_group(
                    ui,
                    app,
                    "Edición",
                    &[(Tool::Select, "Seleccionar", "Selecciona un objeto")],
                );

                // ── CONSTRUCCIÓN ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Construcción",
                        &[
                            (Tool::Midpoint, "Punto Medio", "Medio o centro"),
                            (Tool::Perpendicular, "Perpendicular", "Recta perpendicular"),
                            (Tool::Tangent, "Tangentes", "Tangentes a una curva"),
                            (Tool::Locus, "Lugar Geométrico", "Locus de un punto"),
                        ],
                    );
                }

                // ── MEDICIÓN ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Medición",
                        &[
                            (Tool::Angle, "Ángulo", "Ángulo entre 3 puntos"),
                            (Tool::Distance, "Distancia", "Distancia o longitud"),
                            (Tool::Area, "Área", "Área de un polígono/cónica"),
                            (Tool::Slope, "Pendiente", "Pendiente de recta"),
                        ],
                    );
                }

                // ── LÍNEAS Y POLÍGONOS ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Líneas y Polígonos",
                        &[
                            (Tool::Segment, "Segmento", "Segmento entre 2 puntos"),
                            (Tool::Line, "Recta", "Recta por 2 puntos"),
                            (Tool::Ray, "Semirrecta", "Semirrecta por 2 puntos"),
                            (Tool::Vector, "Vector", "Vector desde un origen"),
                            (Tool::Polygon, "Polígono", "Polígono libre"),
                            (Tool::RegularPolygon, "Polígono Reg.", "Polígono regular"),
                        ],
                    );
                }

                // ── CÍRCULOS Y CÓNICAS ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Circunferencias y Cónicas",
                        &[
                            (Tool::Circle, "Circunferencia", "Centro y punto"),
                            (Tool::EllipseByFoci, "Elipse", "Dos focos y punto"),
                            (
                                Tool::ParabolaByFocusDirectrix,
                                "Parábola",
                                "Foco y directriz",
                            ),
                            (Tool::HyperbolaByFoci, "Hipérbola", "Dos focos y punto"),
                            (Tool::ConicByFivePoints, "Cónica 5 ptos", "Por 5 puntos"),
                        ],
                    );
                }

                // ── 3D ──
                if is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Sólidos 3D",
                        &[
                            (Tool::Sphere3D, "Esfera", "Centro y punto en borde"),
                            (Tool::Cube3D, "Cubo", "Centro y radio (aprox)"),
                        ],
                    );
                }

                // ── ANÁLISIS ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Análisis",
                        &[
                            (Tool::Root, "Raíces", "Cortes con el eje X"),
                            (Tool::Extremum, "Extremos", "Puntos máximos y mínimos"),
                            (Tool::Intersect, "Intersección", "Intersección de 2 objetos"),
                            (Tool::Function, "Función Libre", "Crear f(x) libre"),
                        ],
                    );
                }

                // ── BOOL. POLÍGONOS ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Operaciones Booleanas",
                        &[
                            (Tool::PolygonUnion, "Unión", "Unión de polígonos"),
                            (Tool::PolygonIntersection, "Intersección", "Intersección"),
                            (Tool::PolygonDifference, "Diferencia", "A menos B"),
                            (Tool::PolygonXor, "XOR", "Diferencia simétrica"),
                        ],
                    );
                }
            });
        });
}

fn draw_tool_group(ui: &mut Ui, app: &mut GrafitoApp, title: &str, tools: &[(Tool, &str, &str)]) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(14.0)
            .color(Color32::from_gray(140)),
    );
    ui.add_space(8.0);

    // We will use a grid to lay them out in 2 columns
    let num_cols = 2;
    egui::Grid::new(title)
        .num_columns(num_cols)
        .spacing(egui::vec2(8.0, 8.0))
        .show(ui, |ui| {
            for (i, (tool, name, desc)) in tools.iter().enumerate() {
                let is_selected = app.current_tool == *tool;

                let btn_fill = if is_selected {
                    Color32::from_rgba_unmultiplied(53, 132, 228, 40)
                } else if app.dark_mode {
                    Color32::from_gray(38)
                } else {
                    Color32::from_rgb(240, 240, 245)
                };

                let border = if is_selected {
                    egui::Stroke::new(1.0, Color32::from_rgb(53, 132, 228))
                } else {
                    egui::Stroke::NONE
                };

                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(140.0), 38.0),
                    egui::Sense::click(),
                );

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        6.0,
                        if resp.hovered() && !is_selected {
                            Color32::from_gray(if app.dark_mode { 45 } else { 220 })
                        } else {
                            btn_fill
                        },
                    );
                    painter.rect_stroke(rect, 6.0, border);

                    let icon = match tool {
                        Tool::Select => "^",
                        Tool::Point | Tool::Point3D => ".",
                        Tool::Slider => "O-",
                        Tool::Midpoint => "M",
                        Tool::Perpendicular => "|_",
                        Tool::Tangent => "/",
                        Tool::Locus => "~",
                        Tool::Angle => "<",
                        Tool::Distance => "|x|",
                        Tool::Area => "A",
                        Tool::Slope => "m",
                        Tool::Segment => "-",
                        Tool::Line => "<->",
                        Tool::Ray => "->",
                        Tool::Vector => "=>",
                        Tool::Polygon => "P",
                        Tool::RegularPolygon => "RP",
                        Tool::Circle => "O",
                        Tool::EllipseByFoci => "E",
                        Tool::ParabolaByFocusDirectrix => "U",
                        Tool::HyperbolaByFoci => ")(",
                        Tool::ConicByFivePoints => "C5",
                        Tool::Sphere3D => "(O)",
                        Tool::Cube3D => "[ ]",
                        Tool::Root => "Rz",
                        Tool::Extremum => "Ex",
                        Tool::Intersect => "X",
                        Tool::Function => "f(x)",
                        Tool::PolygonUnion => "U",
                        Tool::PolygonIntersection => "n",
                        Tool::PolygonDifference => "\\",
                        Tool::PolygonXor => "^",
                        _ => "?",
                    };

                    // Emular un ícono
                    let icon_rect = egui::Rect::from_center_size(
                        rect.left_center() + egui::vec2(20.0, 0.0),
                        egui::vec2(24.0, 24.0),
                    );

                    painter.circle_filled(
                        icon_rect.center(),
                        12.0,
                        if app.dark_mode {
                            Color32::from_gray(60)
                        } else {
                            Color32::WHITE
                        },
                    );
                    painter.text(
                        icon_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(14.0),
                        if app.dark_mode {
                            Color32::WHITE
                        } else {
                            Color32::BLACK
                        },
                    );

                    // Texto de la herramienta
                    let text_color = if app.dark_mode {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    };
                    painter.text(
                        rect.left_center() + egui::vec2(44.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        *name,
                        egui::FontId::proportional(13.0),
                        text_color,
                    );
                }

                let resp = resp.on_hover_text(*desc);

                if resp.clicked() {
                    app.current_tool = *tool;
                    app.clear_pending_action();
                    app.tool_ghost = None;
                    app.pending_points.clear();
                }

                if (i + 1) % num_cols == 0 {
                    ui.end_row();
                }
            }
        });
    ui.add_space(16.0);
}
