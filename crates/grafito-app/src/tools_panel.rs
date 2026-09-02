use crate::GrafitoApp;
use egui::Ui;
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::current_theme;
use grafito_ui::tokens::{
    CARD_SPACING, PANEL_LEFT_DEFAULT, PANEL_LEFT_MAX_FRACTION, PANEL_LEFT_MIN, RADIUS_SM, SPACE_SM,
    SPACE_XS, TYPE_LG, TYPE_SM, TYPE_XS, ZOOM_ICON_HIT,
};
use grafito_ui::toolbar::draw_tool_icon;
use grafito_ui::Tool;

pub fn draw_tools_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let panel_fill = theme.panel_bg;

    egui::SidePanel::left("tools_panel")
        .show_separator_line(false)
        .default_width(PANEL_LEFT_DEFAULT)
        .min_width(PANEL_LEFT_MIN)
        .max_width((ctx.available_rect().width() * PANEL_LEFT_MAX_FRACTION).max(PANEL_LEFT_DEFAULT - 40.0))
        .resizable(true)
        .frame(egui::Frame::none().fill(panel_fill).inner_margin(egui::Margin::same(SPACE_SM)))
        .show(ctx, |ui| {
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_SM;
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new("Herramientas")
                        .strong()
                        .size(TYPE_LG)
                        .color(theme.accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(SPACE_SM);
                    if action_icon_button(
                        ui,
                        Icon::Close,
                        theme.text_secondary,
                        "Cerrar panel de Herramientas",
                    )
                    .clicked()
                    {
                        app.sidebar_tab = 0; // Return to Algebra
                    }
                });
            });
            ui.add_space(SPACE_SM);
            ui.painter().line_segment(
                [
                    ui.cursor().min,
                    ui.cursor().min + egui::vec2(ui.available_width(), 0.0),
                ],
                theme.hairline_stroke(),
            );
            ui.add_space(SPACE_SM);

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

                // ── CÓNICAS Y COMPÁS ──
                // Calm: descarga Cónicas, añade construcciones de compás.
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Cónicas y Compás",
                        &[
                            (Tool::Circle, "Circunferencia", "Centro y punto — Circle[centro, radio]"),
                            (
                                Tool::EllipseByFoci,
                                "Elipse",
                                "Dos focos y punto — EllipseByFoci[F1,F2,P]",
                            ),
                            (
                                Tool::ParabolaByFocusDirectrix,
                                "Parábola",
                                "Foco y directriz — ParabolaByFocusDirectrix[F,d]",
                            ),
                            (
                                Tool::HyperbolaByFoci,
                                "Hipérbola",
                                "Dos focos y punto — HyperbolaByFoci[F1,F2,P]",
                            ),
                            (
                                Tool::ConicByFivePoints,
                                "Cónica 5 ptos",
                                "Por 5 puntos — ConicByFivePoints[A,B,C,D,E]",
                            ),
                            (
                                Tool::Circle,
                                "Incírculo",
                                "Incircle[A,B,C] — círculo inscrito en triángulo",
                            ),
                            (
                                Tool::Circle,
                                "Circuncírculo",
                                "Circumcircle[A,B,C] — círculo circunscrito",
                            ),
                            (
                                Tool::Circle,
                                "Compás",
                                "Compasses[centro, punto] — traza círculo con compás",
                            ),
                        ],
                    );
                }

                // ── CURVAS ESPECIALES ──
                // Nuevo grupo calm 2 cols que aligera Líneas/Cónicas.
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Curvas especiales",
                        &[
                            (
                                Tool::Circle,
                                "Arco",
                                "Arc[centro, radio, inicio, fin] o Arc[P1,P2,P3]",
                            ),
                            (
                                Tool::Circle,
                                "Sector",
                                "Sector[centro, radio, ángulo] — relleno circular",
                            ),
                            (
                                Tool::Circle,
                                "Semicírculo",
                                "Semicircle[centro, radio] o [P1,P2,P3]",
                            ),
                            (
                                Tool::ParametricCurve2D,
                                "Bezier",
                                "BezierCurve[P1,P2,...] — 2..64 puntos de control",
                            ),
                            (
                                Tool::PolarCurve,
                                "Spline",
                                "Spline[P1,P2,...] — Catmull-Rom 2..64 puntos",
                            ),
                        ],
                    );
                }

                // ── TRANSFORMACIONES ──
                // Shear/Stretch/Reflect no tienen Tool dedicado: se mapean a Select con tooltip.
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Transformaciones",
                        &[
                            (
                                Tool::Select,
                                "Reflect (espejo)",
                                "Reflect[obj, eje] o Reflect[obj, círculo] — inversión circular",
                            ),
                            (
                                Tool::Select,
                                "Cizalla (Shear)",
                                "Shear[obj, ángulo, eje] — x' = x + k·y, k=tan(ángulo)",
                            ),
                            (
                                Tool::Select,
                                "Estira (Stretch)",
                                "Stretch[obj, factor, eje] — estiramiento afín",
                            ),
                        ],
                    );
                }

                // ── TEXTO ──
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Texto",
                        &[
                            (
                                Tool::Select,
                                "FractionText",
                                "FractionText[valor] — 0.5 → \"1/2\" — vía comando",
                            ),
                            (
                                Tool::Select,
                                "SurdText",
                                "SurdText[valor] — 1.414 → \"√2\" — vía comando",
                            ),
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
                    // Sólidos avanzados — solo en 3D (progressive disclosure).
                    draw_tool_group(
                        ui,
                        app,
                        "Sólidos avanzados",
                        &[
                            (
                                Tool::Cube3D,
                                "Prisma",
                                "Prism[polígono, altura] o [polígono, dx,dy,dz]",
                            ),
                            (
                                Tool::Sphere3D,
                                "Cuádrica",
                                "Quadric[a,b,c,d,e,f,g,h,i,j] — a·x²+…+j=0",
                            ),
                            (
                                Tool::Plane3D,
                                "Intersección 3D",
                                "Intersection3D[a,b] — plano/plano, recta/plano, etc.",
                            ),
                        ],
                    );
                    draw_tool_group(
                        ui,
                        app,
                        "4D proyectado",
                        &[
                            (
                                Tool::Tesseract4D,
                                "Teseracto 4D",
                                "Crea un teseracto 4D centrado y proyectado",
                            ),
                            (
                                Tool::Hypercube5D,
                                "Hipercubo 5D",
                                "Crea un hipercubo 5D centrado y proyectado",
                            ),
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
                            (Tool::Root, "Raíces", "Cortes con el eje X — Root[f]"),
                            (Tool::Extremum, "Extremos", "Puntos máximos y mínimos — Extremum[f]"),
                            (
                                Tool::Intersect,
                                "Intersección",
                                "Intersección de 2 objetos — Intersect[a,b]",
                            ),
                            (Tool::Function, "Función Libre", "Crear f(x) libre — Function[expr]"),
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
                            (Tool::PolygonUnion, "Unión", "Unión de polígonos — PolygonUnion[A,B]"),
                            (
                                Tool::PolygonIntersection,
                                "Intersección",
                                "Intersección — PolygonIntersection[A,B]",
                            ),
                            (
                                Tool::PolygonDifference,
                                "Diferencia",
                                "A menos B — PolygonDifference[A,B]",
                            ),
                            (Tool::PolygonXor, "XOR", "Diferencia simétrica — PolygonXor[A,B]"),
                        ],
                    );
                }

                // ── DISCRETA Y LISTAS ──
                // Progressive disclosure: solo en 2D, vía comando (Financiera/CAS/Probabilidad en paleta).
                if !is_3d {
                    draw_tool_group(
                        ui,
                        app,
                        "Discreta y Listas",
                        &[
                            (
                                Tool::Select,
                                "ConvexHull",
                                "ConvexHull[puntos] — envolvente convexa",
                            ),
                            (Tool::Select, "Voronoi", "Voronoi[puntos] — diagrama aproximado"),
                            (
                                Tool::Select,
                                "Sequence",
                                "Sequence[expr, var, inicio, fin] — genera lista",
                            ),
                            (Tool::Select, "Sort", "Sort[lista] — ordena lista numérica"),
                        ],
                    );
                    ui.label(
                        egui::RichText::new(
                            "Financiera · CAS · Probabilidad vía paleta (Ctrl+K): Rate[], Derivative[], Normal[]…",
                        )
                        .size(TYPE_XS)
                        .color(theme.text_tertiary)
                        .italics(),
                    );
                    ui.add_space(SPACE_SM);
                }
            });
        });
}

fn draw_tool_group(ui: &mut Ui, app: &mut GrafitoApp, title: &str, tools: &[(Tool, &str, &str)]) {
    let theme = current_theme(ui.ctx());
    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(TYPE_SM)
            .color(theme.text_secondary),
    );
    ui.add_space(SPACE_SM);

    // We will use a grid to lay them out in 2 columns
    let num_cols = 2;
    egui::Grid::new(title)
        .num_columns(num_cols)
        .spacing(egui::vec2(SPACE_SM, SPACE_SM))
        .show(ui, |ui| {
            for (i, (tool, name, desc)) in tools.iter().enumerate() {
                let is_selected = app.current_tool == *tool;

                let btn_fill = if is_selected {
                    theme.accent_muted
                } else {
                    theme.button_bg
                };

                let border = if is_selected {
                    egui::Stroke::new(1.0, theme.accent)
                } else {
                    egui::Stroke::NONE
                };

                let (rect, resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(140.0), ZOOM_ICON_HIT),
                    egui::Sense::click(),
                );

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        RADIUS_SM,
                        if resp.hovered() && !is_selected {
                            theme.button_hover
                        } else {
                            btn_fill
                        },
                    );
                    painter.rect_stroke(rect, RADIUS_SM, border);

                    let icon_rect = egui::Rect::from_center_size(
                        rect.left_center() + egui::vec2(20.0, 0.0),
                        egui::vec2(24.0, 24.0),
                    );

                    painter.circle_filled(icon_rect.center(), 12.0, theme.input_bg);
                    draw_tool_icon(
                        painter,
                        icon_rect.shrink(4.0),
                        *tool,
                        if is_selected {
                            theme.accent
                        } else {
                            theme.text_primary
                        },
                    );

                    // Texto de la herramienta
                    painter.text(
                        rect.left_center() + egui::vec2(44.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        *name,
                        egui::FontId::proportional(TYPE_SM),
                        theme.text_primary,
                    );
                }

                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("{name}: {desc}"),
                    )
                });
                let resp = resp.on_hover_text(*desc);

                if resp.clicked() {
                    app.current_tool = *tool;
                    app.clear_pending_action();
                    app.tool_ghost = None;
                    app.reset_tool_input();
                }

                if (i + 1) % num_cols == 0 {
                    ui.end_row();
                }
            }
        });
    ui.add_space(CARD_SPACING);
}
