//! Algebra side panel: object list, inline property editors, variables,
//! and command input preview.

use crate::{commands, GrafitoApp, ViewMode};
use egui::Color32;
use grafito_core::{GeoObject, ObjectId};
use grafito_ui::icons::{action_icon_button, draw_icon, Icon};
use grafito_ui::theme::current_theme;
use grafito_ui::tokens::{
    ICON_SM, ICON_XL, PANEL_LEFT_DEFAULT, PANEL_LEFT_MAX_FRACTION, PANEL_LEFT_MIN, RADIUS_SM,
    SPACE_LG, SPACE_SM, SPACE_XS, TYPE_LG, TYPE_SM, TYPE_XS, ZOOM_ICON_HIT,
};

pub(crate) const OBJECT_COLOR_TARGET_SIZE: egui::Vec2 =
    egui::Vec2::new(ZOOM_ICON_HIT, ZOOM_ICON_HIT);

const _ASSERT_HIT_SQUARE_32: () = assert!(ICON_XL == ZOOM_ICON_HIT && ZOOM_ICON_HIT == 32.0);

pub(crate) fn variable_meta_for_display(
    document: &grafito_core::Document,
    name: &str,
) -> grafito_core::VariableMeta {
    document
        .variable_meta(name)
        .cloned()
        .unwrap_or(grafito_core::VariableMeta {
            position: grafito_geometry::Point2::new(0.0, 0.0),
            min: -5.0,
            max: 5.0,
            step: 0.1,
            visible: true,
            animating: false,
            animation_speed: 1.0,
            animation_mode: grafito_core::AnimationMode::PingPong,
        })
}

pub(crate) fn apply_variable_meta_panel_edit(
    document: &mut grafito_core::Document,
    name: &str,
    candidate: grafito_core::VariableMeta,
    snapshot: &mut crate::app::DeferredPanelSnapshot,
) -> Result<bool, String> {
    let Some(before) = document.try_replace_variable_meta_with_previous(name, candidate)? else {
        return Ok(false);
    };
    snapshot.capture_successful_replacement(before);
    Ok(true)
}

fn color32_from_object_color(color: grafito_geometry::Color, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color.r * 255.0).clamp(0.0, 255.0) as u8,
        (color.g * 255.0).clamp(0.0, 255.0) as u8,
        (color.b * 255.0).clamp(0.0, 255.0) as u8,
        alpha,
    )
}

fn is_internal_trig_name(name: &str) -> bool {
    name == "TrigGraph" || name == "TrigValue" || name == "trig_angle" || name.starts_with("trig_")
}

fn regular_polychoron_summary(polychoron: &grafito_core::RegularPolychoron4DObj) -> String {
    let name = match polychoron.kind {
        grafito_geometry::RegularPolychoron::Pentachoron => "Pentácoron 4D",
        grafito_geometry::RegularPolychoron::Tesseract => "Teseracto 4D",
        grafito_geometry::RegularPolychoron::SixteenCell => "16-celda 4D",
        grafito_geometry::RegularPolychoron::TwentyFourCell => "24-celda 4D",
        grafito_geometry::RegularPolychoron::OneTwentyCell => "120-celda 4D",
        grafito_geometry::RegularPolychoron::SixHundredCell => "600-celda 4D",
    };
    format!(
        "{name} centrado y proyectado · escala={:.2}",
        polychoron.scale
    )
}

fn regular_polytope_nd_summary(polytope: &grafito_core::RegularPolytopeNDObj) -> String {
    let family = match polytope.family {
        grafito_geometry::RegularPolytopeFamily::Simplex => "Símplex",
        grafito_geometry::RegularPolytopeFamily::Hypercube => "Hipercubo",
        grafito_geometry::RegularPolytopeFamily::CrossPolytope => "Politopo cruzado",
    };
    format!(
        "{family} {}D centrado y proyectado · escala={:.2}",
        polytope.dimension, polytope.scale
    )
}

pub(crate) fn object_expression_summary(obj: &GeoObject) -> String {
    match obj {
        GeoObject::Function(f) => f.fit.as_ref().map_or_else(
            || f.expr.clone(),
            |fit| {
                format!(
                    "{} · {} · RMSE={:.3} · R²={:.3}",
                    f.expr,
                    fit.kind.display_name(),
                    fit.diagnostics.rmse,
                    fit.diagnostics.r_squared
                )
            },
        ),
        GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
        GeoObject::Line(l) => {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            let len = (dx * dx + dy * dy).sqrt();
            format!(
                "({:.2}, {:.2}) <-> ({:.2}, {:.2})  L={:.3}",
                l.start.x, l.start.y, l.end.x, l.end.y, len
            )
        }
        GeoObject::Circle(c) => {
            let area = std::f64::consts::PI * c.radius * c.radius;
            let perim = 2.0 * std::f64::consts::PI * c.radius;
            format!("r={:.2}  A={:.3}  P={:.3}", c.radius, area, perim)
        }
        GeoObject::Ellipse(e) => {
            let area = std::f64::consts::PI * e.rx * e.ry;
            format!("rx={:.2} ry={:.2}  A={:.3}", e.rx, e.ry, area)
        }
        GeoObject::Polygon(p) => {
            let n = p.vertices.len();
            let perim = if n >= 2 {
                let mut sum = 0.0;
                for i in 0..n {
                    let a = p.vertices[i];
                    let b = p.vertices[(i + 1) % n];
                    let dx = b.x - a.x;
                    let dy = b.y - a.y;
                    sum += (dx * dx + dy * dy).sqrt();
                }
                sum
            } else {
                0.0
            };
            let area = if n >= 3 {
                let mut s = 0.0;
                for i in 0..n {
                    let j = (i + 1) % n;
                    s += p.vertices[i].x * p.vertices[j].y - p.vertices[j].x * p.vertices[i].y;
                }
                s.abs() * 0.5
            } else {
                0.0
            };
            format!("{} vértices  P={:.3}  A={:.3}", n, perim, area)
        }
        GeoObject::Pencil(p) if p.is_dynamic_locus() => {
            format!("Locus: {} puntos", p.points.len())
        }
        GeoObject::Pencil(p) => format!("{} puntos", p.points.len()),
        GeoObject::Point3D(p) => format!(
            "({:.2}, {:.2}, {:.2})",
            p.position.x, p.position.y, p.position.z
        ),
        GeoObject::Sphere3D(s) => {
            let area = 4.0 * std::f64::consts::PI * s.radius * s.radius;
            let vol = 4.0 / 3.0 * std::f64::consts::PI * s.radius * s.radius * s.radius;
            format!("r={:.2}  A={:.3}  V={:.3}", s.radius, area, vol)
        }
        GeoObject::Cube3D(c) => {
            let vol = c.size * c.size * c.size;
            format!("size={:.2}  V={:.3}", c.size, vol)
        }
        GeoObject::Tetrahedron3D(t) => {
            let vol = t.edge_length.powi(3) / (6.0 * 2.0_f64.sqrt());
            format!("edge={:.2}  V={:.3}", t.edge_length, vol)
        }
        GeoObject::RegularPolychoron4D(polychoron) => regular_polychoron_summary(polychoron),
        GeoObject::RegularPolytopeND(polytope) => regular_polytope_nd_summary(polytope),
        GeoObject::Cylinder3D(cy) => {
            let dx = cy.top_center.x - cy.base_center.x;
            let dy = cy.top_center.y - cy.base_center.y;
            let dz = cy.top_center.z - cy.base_center.z;
            let h = (dx * dx + dy * dy + dz * dz).sqrt();
            let vol = std::f64::consts::PI * cy.radius * cy.radius * h;
            format!("r={:.2} h={:.2}  V={:.3}", cy.radius, h, vol)
        }
        GeoObject::Cone3D(co) => {
            let dx = co.apex.x - co.base_center.x;
            let dy = co.apex.y - co.base_center.y;
            let dz = co.apex.z - co.base_center.z;
            let h = (dx * dx + dy * dy + dz * dz).sqrt();
            let vol = 1.0 / 3.0 * std::f64::consts::PI * co.radius * co.radius * h;
            format!("r={:.2} h={:.2}  V={:.3}", co.radius, h, vol)
        }
        GeoObject::Torus3D(t) => format!("R={:.2} r={:.2}", t.r_major, t.r_minor),
        GeoObject::Segment3D(s) => {
            let dx = s.b.x - s.a.x;
            let dy = s.b.y - s.a.y;
            let dz = s.b.z - s.a.z;
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            format!("L={:.3}", len)
        }
        GeoObject::ParametricCurve2D(p) => format!("({}, {})", p.expr_x, p.expr_y),
        GeoObject::PolarCurve(p) => format!("r = {}", p.expr_r),
        GeoObject::VectorField2D(v) => format!("({}, {})", v.expr_u, v.expr_v),
        GeoObject::ComplexGrid(c) => c.expr.clone(),
        GeoObject::ComplexMapping(c) => c.expr.clone(),
        GeoObject::ImplicitCurve(ic) => {
            let op = match ic.operator {
                grafito_core::RelationOperator::Eq => "=",
                grafito_core::RelationOperator::Less => "<",
                grafito_core::RelationOperator::LessEq => "<=",
                grafito_core::RelationOperator::Greater => ">",
                grafito_core::RelationOperator::GreaterEq => ">=",
            };
            format!("{} {} {}", ic.expr_lhs, op, ic.expr_rhs)
        }
        GeoObject::Histogram(h) => format!("{} datos · {} bins", h.data.len(), h.bins),
        GeoObject::ScatterPlot(s) => format!("{} puntos", s.xs.len().min(s.ys.len())),
        GeoObject::BoxPlot(b) => format!("{} datos", b.data.len()),
        GeoObject::RegressionLine(r) => format!("y = {:.3}x + {:.3}", r.slope, r.intercept),
        GeoObject::DataTable(table) => format!(
            "{} pares · {} / {}",
            table.xs.len(),
            table.x_name,
            table.y_name
        ),
        GeoObject::PhasePortrait(p) => format!("({}, {})", p.expr_dx, p.expr_dy),
        _ => String::new(),
    }
}

pub(crate) fn draw_object_card(ui: &mut egui::Ui, app: &mut GrafitoApp, oid: ObjectId) {
    let theme = current_theme(ui.ctx());
    let Some(obj) = app.document.get_object(oid) else {
        return;
    };
    let obj_label = obj.label().to_string();
    let obj_name = obj.name().to_string();
    let obj_vis = obj.is_visible();
    let obj_supports_visibility = !matches!(obj, GeoObject::DataTable(_));
    let obj_col = color32_from_object_color(
        obj.color(),
        (obj.color().a.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    let obj_expr = object_expression_summary(obj);

    let is_sel = app.selected_object == Some(oid);
    let frame_fill = if is_sel {
        theme.accent_muted
    } else {
        theme.button_bg
    };
    let border = if is_sel {
        egui::Stroke::new(1.0, theme.accent)
    } else {
        theme.hairline_stroke()
    };

    let mut row_clicked = false;
    let mut delete = false;
    ui.add_space(SPACE_XS);
    egui::Frame::none()
        .fill(frame_fill)
        .rounding(RADIUS_SM)
        .stroke(border)
        .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (del_rect, del_resp) = ui.allocate_exact_size(
                        egui::vec2(ZOOM_ICON_HIT, ZOOM_ICON_HIT),
                        egui::Sense::click(),
                    );
                    del_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Eliminar objeto")
                    });
                    if ui.is_rect_visible(del_rect) {
                        draw_icon(
                            ui.painter(),
                            del_rect.shrink(4.0),
                            Icon::Delete,
                            theme.text_secondary,
                        );
                    }
                    if del_resp.on_hover_text("Eliminar").clicked() {
                        delete = true;
                    }

                    if obj_supports_visibility {
                        let (eye_rect, eye_resp) = ui.allocate_exact_size(
                            egui::vec2(ZOOM_ICON_HIT, ZOOM_ICON_HIT),
                            egui::Sense::click(),
                        );
                        eye_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Cambiar visibilidad del objeto",
                            )
                        });
                        if ui.is_rect_visible(eye_rect) {
                            draw_icon(
                                ui.painter(),
                                eye_rect.shrink(4.0),
                                if obj_vis { Icon::Eye } else { Icon::EyeOff },
                                theme.text_secondary,
                            );
                        }
                        if eye_resp.on_hover_text("Visibilidad").clicked() {
                            app.save_state();
                            if let Some(o) = app.document.get_object_mut(oid) {
                                o.set_visible(!obj_vis);
                                app.document.bump_version();
                            }
                        }
                    } else {
                        ui.add_space(ZOOM_ICON_HIT);
                    }

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let dot_alpha = if obj_vis {
                            obj_col.a()
                        } else {
                            obj_col.a().min(80)
                        };
                        let dot_col = Color32::from_rgba_unmultiplied(
                            obj_col.r(),
                            obj_col.g(),
                            obj_col.b(),
                            dot_alpha,
                        );
                        let (dot_r, dot_resp) =
                            ui.allocate_exact_size(OBJECT_COLOR_TARGET_SIZE, egui::Sense::click());
                        dot_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Cambiar color del objeto",
                            )
                        });
                        ui.painter().circle_filled(dot_r.center(), 6.0, dot_col);
                        if dot_resp.hovered() {
                            ui.painter().circle_stroke(
                                dot_r.center(),
                                6.0,
                                egui::Stroke::new(1.0, Color32::WHITE),
                            );
                        }
                        if dot_resp.on_hover_text("Cambiar color").clicked() {
                            app.open_object_color_picker(oid);
                            row_clicked = true;
                        }
                        ui.add_space(SPACE_XS);

                        let txt = if !obj_expr.is_empty() {
                            format!("{}: {}", obj_label, obj_expr)
                        } else {
                            format!("{}: {}", obj_label, obj_name)
                        };
                        let lbl_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(txt)
                                    .size(TYPE_SM) // TYPE_SM (12) +1 = original 13
                                    .color(theme.text_primary),
                            )
                            .sense(egui::Sense::click())
                            .truncate(),
                        );
                        if lbl_resp.clicked() {
                            row_clicked = true;
                        }
                        if lbl_resp.double_clicked()
                            && !obj_expr.is_empty()
                            && (obj_name == "Function" || obj_name == "Point")
                        {
                            app.input_text = format!("{}={}", obj_label, obj_expr);
                        }
                    });
                });
            });
        });

    if row_clicked {
        app.selected_object = if is_sel { None } else { Some(oid) };
    }
    if delete {
        app.save_state();
        app.document.remove_object(oid);
        if app.selected_object == Some(oid) {
            app.selected_object = None;
        }
    }
    ui.add_space(SPACE_XS);
}

pub(crate) fn draw_algebra_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let mut snapshot = crate::app::DeferredPanelSnapshot::new(app.undo_stack.len());
    let theme = current_theme(ctx);
    let accent = theme.accent;
    let alg_fill = theme.panel_bg;
    let _sep_col = theme.separator;
    let txt_col = theme.text_primary;
    let _txt_dim = theme.text_tertiary;

    egui::SidePanel::left("algebra_panel").show_separator_line(false)
    .default_width(PANEL_LEFT_DEFAULT)
    .min_width(PANEL_LEFT_MIN)
    .max_width((ctx.available_rect().width() * PANEL_LEFT_MAX_FRACTION).max(PANEL_LEFT_DEFAULT - 40.0))
    .resizable(true)
    .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::NONE))
    .show(ctx, |ui| {
        // Header — harmonized with Vista/Herramientas: aire 8, item_spacing 8, left XS, TYPE_LG, XS gap to count, Close with right padding
        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_SM;
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new("Álgebra")
                    .color(accent)
                    .size(TYPE_LG)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new(format!("{} objetos", app.document.object_count()))
                    .color(theme.text_tertiary)
                    .size(TYPE_XS),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(SPACE_SM);
                if action_icon_button(
                    ui,
                    Icon::Close,
                    theme.text_secondary,
                    "Ocultar panel de Álgebra",
                )
                .clicked()
                {
                    app.left_drawer_open = false;
                    app.compact_drawer_open = false;
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
        // Input row — cuadrado RADIUS_SM 8 (no pill), aire 8, gap 8 entre +/input/botón, sin espacio vacío a la derecha
        egui::Frame::none()
            .fill(theme.input_bg)
            .stroke(theme.hairline_stroke())
            .rounding(egui::Rounding::same(RADIUS_SM))
            .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = SPACE_SM;
                    let (plus_rect, _) =
                        ui.allocate_exact_size(egui::vec2(ICON_SM, ICON_SM), egui::Sense::hover());
                    if ui.is_rect_visible(plus_rect) {
                        draw_icon(ui.painter(), plus_rect, Icon::Plus, accent);
                    }
                    // Input ocupa todo el ancho disponible entre + y botón — sin hueco a la derecha
                    let button_w = SPACE_XL + SPACE_XS / 2.0;
                    let input_width = (ui.available_width() - button_w - SPACE_SM).max(80.0);
                    let response = crate::ui::draw_command_input(
                        ui,
                        app,
                        "algebra_panel",
                        [input_width, ZOOM_ICON_HIT],
                        "Añadir…",
                        false,
                    );
                    if action_icon_button(
                        ui,
                        Icon::Grid,
                        if app.keyboard_visible {
                            accent
                        } else {
                            theme.text_secondary
                        },
                        if app.keyboard_visible {
                            "Ocultar teclado"
                        } else {
                            "Mostrar teclado"
                        },
                    )
                    .clicked()
                    {
                        app.keyboard_visible = !app.keyboard_visible;
                        if !app.keyboard_visible {
                            app.keyboard_expanded = false;
                        }
                    }
                    if response.changed {
                        app.preview_object = commands::parse_preview(&app.input_text);
                    }
                    if app.input_text.is_empty() {
                        app.preview_object = None;
                    }
                    if response.submitted {
                        let time = ui.ctx().input(|i| i.time);
                        app.submit_input_text(time);
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

        // ── Object list — compact, 1 line each ──────────────────────
        egui::Frame::none()
            .inner_margin(egui::Margin {
                left: SPACE_SM,
                right: SPACE_SM,
                top: SPACE_SM,
                bottom: SPACE_SM,
            })
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
            let mut delete_id: Option<ObjectId> = None;
            let ids: Vec<ObjectId> = app.document.objects_iter().map(|(id,_)| *id).collect();
            for oid in ids {
                let (obj_label, obj_name, obj_vis, obj_col, obj_expr) = {
                    let Some(obj) = app.document.get_object(oid) else { continue; };
                    if is_internal_trig_name(obj.label()) {
                        continue;
                    }

                    // Filtra por el espacio de render centralizado en GeoObject.
                    let is_3d_object = obj.is_3d();
                    let is_3d_view = app.current_view == ViewMode::D3;
                    if is_3d_object != is_3d_view {
                        continue;
                    }

                    // El Pencil libre no es analizable; un Locus usa el mismo
                    // almacenamiento de polilínea pero sí es una construcción.
                    if matches!(obj, grafito_core::GeoObject::Pencil(pencil) if !pencil.is_dynamic_locus()) {
                        continue;
                    }

                    let o_col = obj.color();
                    let col = Color32::from_rgba_unmultiplied(
                        (o_col.r * 255.0).clamp(0.0, 255.0) as u8,
                        (o_col.g * 255.0).clamp(0.0, 255.0) as u8,
                        (o_col.b * 255.0).clamp(0.0, 255.0) as u8,
                        (o_col.a.clamp(0.0, 1.0) * 255.0).round() as u8,
                    );
                    let expr = match obj {
                        grafito_core::GeoObject::Function(f) => f.expr.clone(),
                        grafito_core::GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
                        grafito_core::GeoObject::Line(l) => {
                            let dx = l.end.x - l.start.x;
                            let dy = l.end.y - l.start.y;
                            let len = (dx * dx + dy * dy).sqrt();
                            format!(
                                "({:.2}, {:.2}) <-> ({:.2}, {:.2})  L={:.3}",
                                l.start.x, l.start.y, l.end.x, l.end.y, len
                            )
                        }
                        grafito_core::GeoObject::Circle(c) => {
                            let area = std::f64::consts::PI * c.radius * c.radius;
                            let perim = 2.0 * std::f64::consts::PI * c.radius;
                            format!(
                                "r={:.2}  A={:.3}  P={:.3}",
                                c.radius, area, perim
                            )
                        }
                        grafito_core::GeoObject::Ellipse(e) => {
                            let area = std::f64::consts::PI * e.rx * e.ry;
                            format!(
                                "rx={:.2} ry={:.2}  A={:.3}",
                                e.rx, e.ry, area
                            )
                        }
                        grafito_core::GeoObject::Polygon(p) => {
                            let n = p.vertices.len();
                            let perim = if n >= 2 {
                                let mut sum = 0.0;
                                for i in 0..n {
                                    let a = p.vertices[i];
                                    let b = p.vertices[(i + 1) % n];
                                    let dx = b.x - a.x;
                                    let dy = b.y - a.y;
                                    sum += (dx * dx + dy * dy).sqrt();
                                }
                                sum
                            } else {
                                0.0
                            };
                            let area = if n >= 3 {
                                let mut s = 0.0;
                                for i in 0..n {
                                    let j = (i + 1) % n;
                                    s += p.vertices[i].x * p.vertices[j].y
                                        - p.vertices[j].x * p.vertices[i].y;
                                }
                                s.abs() * 0.5
                            } else {
                                0.0
                            };
                            format!("{} vértices  P={:.3}  A={:.3}", n, perim, area)
                        }
                        grafito_core::GeoObject::Pencil(p) if p.is_dynamic_locus() => {
                            format!("Locus: {} puntos", p.points.len())
                        }
                        grafito_core::GeoObject::Pencil(p) => format!("{} puntos", p.points.len()),
                        grafito_core::GeoObject::Point3D(p) => format!("({:.2}, {:.2}, {:.2})", p.position.x, p.position.y, p.position.z),
                        grafito_core::GeoObject::Sphere3D(s) => {
                            let area = 4.0 * std::f64::consts::PI * s.radius * s.radius;
                            let vol = 4.0 / 3.0 * std::f64::consts::PI * s.radius * s.radius * s.radius;
                            format!("r={:.2}  A={:.3}  V={:.3}", s.radius, area, vol)
                        }
                        grafito_core::GeoObject::Cube3D(c) => {
                            let vol = c.size * c.size * c.size;
                            format!("size={:.2}  V={:.3}", c.size, vol)
                        }
                        grafito_core::GeoObject::Tetrahedron3D(t) => {
                            let vol = t.edge_length.powi(3) / (6.0 * 2.0_f64.sqrt());
                            format!("edge={:.2}  V={:.3}", t.edge_length, vol)
                        }
                        grafito_core::GeoObject::RegularPolychoron4D(polychoron) => {
                            regular_polychoron_summary(polychoron)
                        }
                        grafito_core::GeoObject::RegularPolytopeND(polytope) => {
                            regular_polytope_nd_summary(polytope)
                        }
                        grafito_core::GeoObject::Cylinder3D(cy) => {
                            let dx = cy.top_center.x - cy.base_center.x;
                            let dy = cy.top_center.y - cy.base_center.y;
                            let dz = cy.top_center.z - cy.base_center.z;
                            let h = (dx * dx + dy * dy + dz * dz).sqrt();
                            let vol = std::f64::consts::PI * cy.radius * cy.radius * h;
                            format!("r={:.2} h={:.2}  V={:.3}", cy.radius, h, vol)
                        }
                        grafito_core::GeoObject::Cone3D(co) => {
                            let dx = co.apex.x - co.base_center.x;
                            let dy = co.apex.y - co.base_center.y;
                            let dz = co.apex.z - co.base_center.z;
                            let h = (dx * dx + dy * dy + dz * dz).sqrt();
                            let vol = 1.0 / 3.0 * std::f64::consts::PI * co.radius * co.radius * h;
                            format!("r={:.2} h={:.2}  V={:.3}", co.radius, h, vol)
                        }
                        grafito_core::GeoObject::Torus3D(t) => {
                            format!("R={:.2} r={:.2}", t.r_major, t.r_minor)
                        }
                        grafito_core::GeoObject::Segment3D(s) => {
                            let dx = s.b.x - s.a.x;
                            let dy = s.b.y - s.a.y;
                            let dz = s.b.z - s.a.z;
                            let len = (dx * dx + dy * dy + dz * dz).sqrt();
                            format!("L={:.3}", len)
                        }
                        _ => String::new(),
                    };
                    (obj.label().to_string(), obj.name().to_string(), obj.is_visible(), col, expr)
                };

                let is_sel = app.selected_object == Some(oid);
                let frame_fill = if is_sel {
                    theme.accent_muted
                } else {
                    theme.button_bg
                };
                let border = if is_sel {
                    egui::Stroke::new(1.0, theme.accent)
                } else {
                    theme.hairline_stroke()
                };

                let mut row_clicked = false;
                ui.add_space(SPACE_XS);
                egui::Frame::none()
                    .fill(frame_fill)
                    .rounding(RADIUS_SM)
                    .stroke(border)
                    .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // Right-side controls drawn first
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (del_rect, del_resp) = ui.allocate_exact_size(
                                    egui::vec2(ZOOM_ICON_HIT, ZOOM_ICON_HIT),
                                    egui::Sense::click(),
                                );
                                del_resp.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        "Eliminar objeto",
                                    )
                                });
                                if ui.is_rect_visible(del_rect) {
                                    draw_icon(ui.painter(), del_rect.shrink(4.0), Icon::Delete, theme.text_secondary);
                                }
                                if del_resp.on_hover_text("Eliminar").clicked() {
                                    delete_id = Some(oid);
                                }
                                let (eye_rect, eye_resp) = ui.allocate_exact_size(
                                    egui::vec2(ZOOM_ICON_HIT, ZOOM_ICON_HIT),
                                    egui::Sense::click(),
                                );
                                eye_resp.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        "Cambiar visibilidad del objeto",
                                    )
                                });
                                if ui.is_rect_visible(eye_rect) {
                                    draw_icon(
                                        ui.painter(),
                                        eye_rect.shrink(4.0),
                                        if obj_vis { Icon::Eye } else { Icon::EyeOff },
                                        theme.text_secondary,
                                    );
                                }
                                if eye_resp.on_hover_text("Visibilidad").clicked() {
                                    snapshot.capture(&app.document);
                                    if let Some(o) = app.document.get_object_mut(oid) {
                                        let v = o.is_visible(); o.set_visible(!v);
                                    }
                                }

                                // Left-side controls in remaining space
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let dot_alpha = if obj_vis { obj_col.a() } else { obj_col.a().min(80) };
                                    let dot_col = Color32::from_rgba_unmultiplied(
                                        obj_col.r(), obj_col.g(), obj_col.b(), dot_alpha);
                                    let (dot_r, dot_resp) = ui.allocate_exact_size(OBJECT_COLOR_TARGET_SIZE, egui::Sense::click());
                                    dot_resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            true,
                                            "Cambiar color del objeto",
                                        )
                                    });
                                    ui.painter().circle_filled(dot_r.center(), 6.0, dot_col);
                                    if dot_resp.hovered() {
                                        ui.painter().circle_stroke(dot_r.center(), 6.0, egui::Stroke::new(1.0, Color32::WHITE));
                                    }
                                    let dot_resp = dot_resp.on_hover_text("Cambiar color");
                                    if dot_resp.clicked() {
                                        app.open_object_color_picker(oid);
                                        row_clicked = true;
                                    }
                                    ui.add_space(SPACE_XS);

                                    let txt = if !obj_expr.is_empty() {
                                        format!("{}: {}", obj_label, obj_expr)
                                    } else {
                                        format!("{}: {}", obj_label, obj_name)
                                    };
                                    let lbl_resp = ui.add(egui::Label::new(
                                        egui::RichText::new(txt).size(TYPE_SM).color(txt_col)).sense(egui::Sense::click()).truncate()); // TYPE_SM (12) +1 = original 13
                                    if lbl_resp.clicked() { row_clicked = true; }
                                    if lbl_resp.double_clicked() && !obj_expr.is_empty() && (obj_name == "Function" || obj_name == "Point") {
                                        app.input_text = format!("{}={}", obj_label, obj_expr);
                                    }
                                });
                            });
                        });

                        // Properties Panel (Inline)
                        if is_sel && app.current_view != ViewMode::D3 {
                            // Edit a detached copy so idle controls cannot bump the
                            // document revision while an assistant request is pending.
                            if let Some(mut edited) = app.document.get_object(oid).cloned() {
                                ui.add_space(SPACE_XS);
                                ui.scope(|ui| {
                                    // Sin overrides de light mode: confiamos en
                                    // los tokens del theme LIGHT definidos en
                                    // grafito-ui/src/theme.rs.
                                    match &mut edited {
                                        GeoObject::Line(l) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("w").size(TYPE_SM).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut l.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Circle(c) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("w").size(TYPE_SM).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut c.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Function(f) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("w").size(TYPE_SM).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut f.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Point(p) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("pt").size(TYPE_XS).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Point3D(p) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("pt").size(TYPE_XS).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Polygon(poly) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("w").size(TYPE_SM).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut poly.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Pencil(pencil) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(SPACE_LG);
                                                ui.label(egui::RichText::new("pen").size(TYPE_SM).color(theme.text_tertiary));
                                                ui.add(egui::Slider::new(&mut pencil.width, 0.5..=20.0).trailing_fill(true));
                                            });
                                        }
                                        _ => {}
                                    }
                                });
                                let changed = app
                                    .document
                                    .get_object(oid)
                                    .is_some_and(|object| object != &edited);
                                match crate::panels::apply_object_panel_edit_with_previous(
                                    &mut app.document,
                                    oid,
                                    changed,
                                    |object| *object = edited,
                                ) {
                                    Ok(Some(before)) => {
                                        snapshot.capture_successful_replacement(before);
                                    }
                                    Ok(None) => {}
                                    Err(error) => {
                                        let message = format!("Propiedades: {error}");
                                        app.cas_result = message.clone();
                                        app.notify(message, grafito_ui::toast::ToastKind::Error);
                                    }
                                }
                            }
                        } else if is_sel {
                            ui.add_space(SPACE_XS);
                            ui.label(
                                egui::RichText::new("Abrí el Inspector para editar este objeto 3D.")
                                    .color(theme.text_tertiary)
                                    .size(TYPE_XS),
                            );
                        }
                    });

                if row_clicked {
                    app.selected_object = if is_sel { None } else { Some(oid) };
                }
                ui.add_space(SPACE_XS);
            }
            if let Some(id) = delete_id {
                snapshot.capture(&app.document);
                app.document.remove_object(id);
                if app.selected_object == Some(id) { app.selected_object = None; }
            }

            // Variables
            if !app.document.variables.is_empty() {
                ui.add_space(SPACE_SM);

                let vars: Vec<(String, f64)> = app
                    .document
                    .variables
                    .clone()
                    .into_iter()
                    .filter(|(name, _)| !is_internal_trig_name(name))
                    .collect();
                let mut var_to_delete = None;
                for (name, val) in &vars {
                    let mut v = *val;

                    let metadata = variable_meta_for_display(&app.document, name);
                    let (mut animating, mut min, mut max, step, mut speed) = (
                        metadata.animating,
                        metadata.min,
                        metadata.max,
                        metadata.step,
                        metadata.animation_speed,
                    );

                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                // Top row: name = value and options
                                ui.horizontal(|ui| {
                                    let val_str = if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.2}") };
                                    ui.label(egui::RichText::new(format!("{}    {}", name, val_str)).size(TYPE_SM).color(txt_col));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let settings_id = ui.make_persistent_id(("variable_settings", name));
                                        let settings_button = action_icon_button(
                                            ui,
                                            Icon::Settings,
                                            theme.text_secondary,
                                            "Configurar rango de la variable",
                                        );
                                        if settings_button.clicked() {
                                            ui.memory_mut(|mem| mem.toggle_popup(settings_id));
                                        }
                                        egui::popup::popup_below_widget(
                                            ui,
                                            settings_id,
                                            &settings_button,
                                            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                                            |ui| {
                                            ui.set_min_width(180.0);
                                            ui.horizontal(|ui| {
                                                ui.label("Min:");
                                                let max_limit = if max.is_finite() {
                                                    max
                                                } else {
                                                    f64::INFINITY
                                                };
                                                ui.add(
                                                    egui::DragValue::new(&mut min)
                                                        .speed(0.1)
                                                        .range(f64::NEG_INFINITY..=max_limit)
                                                        .clamp_existing_to_range(false),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Max:");
                                                let min_limit = if min.is_finite() {
                                                    min
                                                } else {
                                                    f64::NEG_INFINITY
                                                };
                                                ui.add(
                                                    egui::DragValue::new(&mut max)
                                                        .speed(0.1)
                                                        .range(min_limit..=f64::INFINITY)
                                                        .clamp_existing_to_range(false),
                                                );
                                            });
                                            if !min.is_finite() || !max.is_finite() || min >= max {
                                                ui.colored_label(
                                                    theme.danger,
                                                    "El mínimo debe ser finito y menor que el máximo.",
                                                );
                                            }
                                            ui.separator();
                                            if ui.button("Borrar").clicked() {
                                                var_to_delete = Some(name.clone());
                                                ui.close_menu();
                                            }
                                        },
                                        );

                                        // Selector de velocidad
                                        let speed_abs = speed.abs();
                                        let speed_label = if speed_abs == 1.0 { "1x" } 
                                                          else if speed_abs == 1.5 { "1.5x" }
                                                          else if speed_abs == 2.0 { "2x" }
                                                          else if speed_abs == 0.5 { "0.5x" }
                                                          else { "1x" };

                                        egui::ComboBox::from_id_salt(format!("speed_{}", name))
                                            .selected_text(egui::RichText::new(speed_label).size(TYPE_SM).color(theme.text_secondary))
                                            .width(50.0)
                                            .show_ui(ui, |ui| {
                                                let mut new_speed_abs = speed_abs;
                                                ui.selectable_value(&mut new_speed_abs, 0.5, "0.5x");
                                                ui.selectable_value(&mut new_speed_abs, 1.0, "1x");
                                                ui.selectable_value(&mut new_speed_abs, 1.5, "1.5x");
                                                ui.selectable_value(&mut new_speed_abs, 2.0, "2x");
                                                if new_speed_abs != speed_abs {
                                                    speed = if speed < 0.0 { -new_speed_abs } else { new_speed_abs };
                                                }
                                            });

                                        // Play/Pause button
                                        let tooltip = if animating { "Detener animación" } else { "Animar variable" };
                                        if action_icon_button(
                                            ui,
                                            if animating { Icon::Pause } else { Icon::Play },
                                            if animating { theme.accent } else { theme.text_secondary },
                                            tooltip,
                                        )
                                        .clicked()
                                        {
                                            animating = !animating;
                                            if speed == 0.0 { speed = 1.0; } // Ensure it moves when played
                                        }
                                    });
                                });

                                ui.add_space(SPACE_XS);

                                // Bottom row: min, slider, max
                                if min.is_finite() && max.is_finite() && min < max {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(format!("{}", min)).size(TYPE_SM));

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(egui::RichText::new(format!("{}", max)).size(TYPE_SM));

                                            let mut sl_resp = None;
                                            ui.scope(|ui| {
                                                let visuals = ui.visuals_mut();
                                                visuals.selection.bg_fill = theme.accent;

                                            let mut slider = egui::Slider::new(&mut v, min..=max)
                                                .show_value(false)
                                                .clamping(egui::SliderClamping::Edits)
                                                .trailing_fill(true);

                                            if !animating {
                                                slider = slider.step_by(step);
                                            }

                                            let slider_width = ui.available_width().max(50.0);
                                            sl_resp = Some(ui.add_sized([slider_width, ui.spacing().interact_size.y], slider));
                                        });

                                            if let Some(sl_resp) = sl_resp {
                                                if sl_resp.dragged() && animating {
                                                    animating = false;
                                                }
                                                if sl_resp.changed() && !animating {
                                                    snapshot.capture(&app.document);
                                                    if let Err(error) =
                                                        app.document.try_set_variable(name.clone(), v)
                                                    {
                                                        let message = format!("Variable: {error}");
                                                        app.cas_result = message.clone();
                                                        app.notify(message, grafito_ui::toast::ToastKind::Error);
                                                    }
                                                }
                                            }
                                        });
                                    });
                                } else {
                                    ui.colored_label(
                                        theme.danger,
                                        "Corrige el rango antes de editar el valor.",
                                    );
                                }
                            });
                        });

                    let meta_changed = metadata.animating != animating
                        || metadata.min != min
                        || metadata.max != max
                        || metadata.animation_speed != speed;
                    if meta_changed {
                        let mut candidate = metadata;
                        candidate.animating = animating;
                        candidate.min = min;
                        candidate.max = max;
                        candidate.animation_speed = speed;
                        if let Err(error) = apply_variable_meta_panel_edit(
                            &mut app.document,
                            name,
                            candidate,
                            &mut snapshot,
                        ) {
                            let message = format!("Variable: {error}");
                            app.cas_result = message.clone();
                            app.notify(message, grafito_ui::toast::ToastKind::Error);
                        }
                    }

                    ui.add_space(SPACE_XS);
                    ui.separator();
                }
                if let Some(to_del) = var_to_delete {
                    snapshot.capture(&app.document);
                    app.document.remove_variable(&to_del);
                }
            }
                });
        });
    });
    let _ = snapshot.save_if_semantically_changed(
        &mut app.document,
        &mut app.undo_stack,
        &mut app.redo_stack,
    );
}
