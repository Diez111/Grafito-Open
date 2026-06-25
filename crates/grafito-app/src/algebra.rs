//! Algebra side panel: object list, inline property editors, variables,
//! and command input preview.

use crate::{commands, GrafitoApp, ViewMode};
use egui::{Color32, Key};
use grafito_core::{GeoObject, ObjectId};
use grafito_ui::icons::{draw_icon, Icon};
use grafito_ui::theme::current_theme;

fn color32_from_object_color(color: grafito_geometry::Color, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (color.r * 255.0).clamp(0.0, 255.0) as u8,
        (color.g * 255.0).clamp(0.0, 255.0) as u8,
        (color.b * 255.0).clamp(0.0, 255.0) as u8,
        alpha,
    )
}

pub(crate) fn object_expression_summary(obj: &GeoObject) -> String {
    match obj {
        GeoObject::Function(f) => f.expr.clone(),
        GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
        GeoObject::Line(l) => {
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            let len = (dx * dx + dy * dy).sqrt();
            format!(
                "({:.2}, {:.2}) ↔ ({:.2}, {:.2})  L={:.3}",
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
        GeoObject::PhasePortrait(p) => format!("({}, {})", p.expr_dx, p.expr_dy),
        _ => String::new(),
    }
}

pub(crate) fn draw_object_card(ui: &mut egui::Ui, app: &mut GrafitoApp, oid: ObjectId) {
    let theme = current_theme(ui.ctx());
    let Some(obj) = app.document.get_object(oid) else {
        return;
    };
    // Si el label está vacío (típico de PencilObj y algunos LineObj sin
    // asignación), usamos el nombre del tipo para que el usuario pueda
    // identificar el objeto en el panel y borrarlo si quiere.
    let display_label = {
        let raw = obj.label().to_string();
        if raw.is_empty() {
            format!("({})", obj.name())
        } else {
            raw
        }
    };
    let obj_name = obj.name().to_string();
    let obj_vis = obj.is_visible();
    let obj_col = color32_from_object_color(obj.color(), 255);
    let obj_expr = object_expression_summary(obj);

    let is_sel = app.selected_object == Some(oid);
    let frame_fill = if is_sel {
        theme.accent_muted
    } else {
        theme.panel_bg
    };
    let border = if is_sel {
        egui::Stroke::new(1.0, theme.accent)
    } else {
        egui::Stroke::new(1.0, theme.separator)
    };

    let mut row_clicked = false;
    let mut delete = false;
    ui.add_space(4.0);
    egui::Frame::none()
        .fill(frame_fill)
        .rounding(8.0)
        .stroke(border)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            // Las cards de objetos se acotan al ancho disponible del sidebar
            // con un tope superior: si el sidebar es muy ancho, la card no
            // se estira más allá de 360px para no romper la jerarquía
            // visual ni entrar en ciclos con SidePanel resizable.
            let card_max_w = ui.available_width().min(360.0);
            ui.set_min_width(card_max_w);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (del_rect, del_resp) =
                        ui.allocate_exact_size(egui::vec2(28.0, 24.0), egui::Sense::click());
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

                    let (eye_rect, eye_resp) =
                        ui.allocate_exact_size(egui::vec2(28.0, 24.0), egui::Sense::click());
                    if ui.is_rect_visible(eye_rect) {
                        draw_icon(
                            ui.painter(),
                            eye_rect.shrink(4.0),
                            if obj_vis { Icon::Eye } else { Icon::EyeOff },
                            theme.text_secondary,
                        );
                    }
                    if eye_resp.on_hover_text("Visibilidad").clicked() {
                        if let Some(o) = app.document.get_object_mut(oid) {
                            o.set_visible(!obj_vis);
                            app.document.bump_version();
                        }
                    }

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let dot_alpha = if obj_vis { 255u8 } else { 80u8 };
                        let dot_col = Color32::from_rgba_unmultiplied(
                            obj_col.r(),
                            obj_col.g(),
                            obj_col.b(),
                            dot_alpha,
                        );
                        let (dot_r, dot_resp) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click());
                        ui.painter().circle_filled(dot_r.center(), 6.0, dot_col);
                        if dot_resp.hovered() {
                            ui.painter().circle_stroke(
                                dot_r.center(),
                                6.0,
                                egui::Stroke::new(1.0, Color32::WHITE),
                            );
                        }
                        if dot_resp.on_hover_text("Cambiar color").clicked() {
                            let obj_color = app
                                .document
                                .get_object(oid)
                                .map(|o| o.color())
                                .unwrap_or_else(|| {
                                    grafito_geometry::Color::new(1.0, 1.0, 1.0, 1.0)
                                });
                            app.active_color_picker = Some((
                                oid,
                                grafito_ui::color_picker::HsvColorPicker::new(obj_color),
                            ));
                            row_clicked = true;
                        }
                        ui.add_space(5.0);

                        let txt = if !obj_expr.is_empty() {
                            format!("{}: {}", display_label, obj_expr)
                        } else {
                            format!("{}: {}", display_label, obj_name)
                        };
                        let lbl_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(txt)
                                    .size(13.0)
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
                            app.input_text = format!("{}={}", display_label, obj_expr);
                        }
                    });
                });
            });
        });

    if row_clicked {
        app.selected_object = if is_sel { None } else { Some(oid) };
    }
    if delete {
        app.document.remove_object(oid);
        if app.selected_object == Some(oid) {
            app.selected_object = None;
        }
    }
    ui.add_space(2.0);
}

pub(crate) fn draw_algebra_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let accent = theme.accent;
    let alg_fill = theme.panel_bg;
    let _sep_col = theme.separator;
    let txt_col = theme.text_primary;
    let _txt_dim = theme.text_tertiary;

    egui::SidePanel::left("algebra_panel")
    .exact_width(260.0)
    .resizable(false)
    .frame(egui::Frame::none().fill(alg_fill).inner_margin(egui::Margin::same(0.0)))
    .show(ctx, |ui| {
        // Input row in-panel: es la affordance principal para "agregar cosas"
        // en el panel de Álgebra. Editar aquí equivale a la barra inferior
        // (ambas usan `app.input_text`); lo dejamos visible acá porque es
        // donde el usuario espera tipear.
        egui::Frame::none()
            .fill(theme.input_bg)
            .inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 6.0, bottom: 6.0 })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                    ui.add_space(3.0);
                    // Usamos `desired_width` en vez de `add_sized` con
                    // `available_width` para evitar la realimentación que
                    // hacía crecer el panel lateral sin límite.
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut app.input_text)
                            .hint_text("Entrada...")
                            .desired_width(ui.available_width())
                            .frame(false)
                            .text_color(txt_col),
                    );
                    app.preview_object = None;
                    if !app.input_text.is_empty() {
                        app.preview_object = commands::parse_preview(&app.input_text);
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let time = ui.ctx().input(|i| i.time);
                        app.submit_input_text(time);
                    }
                });
            });
        ui.add(egui::Separator::default().spacing(0.0));

        // ── Object list — compact, 1 line each ──────────────────────
        egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
            let mut delete_id: Option<ObjectId> = None;
            let ids: Vec<ObjectId> = app.document.objects_iter().map(|(id,_)| *id).collect();
            for oid in ids {
                let (display_label, obj_name, obj_vis, obj_col, obj_expr) = {
                    let Some(obj) = app.document.get_object(oid) else { continue; };
                    // Si el label está vacío (típico de PencilObj y
                    // algunos LineObj sin asignación), usamos el nombre
                    // del tipo para que el usuario pueda identificar el
                    // objeto en el panel y borrarlo si quiere.
                    let raw_label = obj.label();
                    let computed_display = if raw_label.is_empty() {
                        format!("({})", obj.name())
                    } else {
                        raw_label.to_string()
                    };

                    // Filter objects by current view mode
                    let is_3d_object = matches!(obj.name(),
                        "Point3D" | "Segment3D" | "Sphere3D" | "Cube3D" | "Pyramid3D" |
                        "Cone3D" | "Cylinder3D" | "Torus3D" | "MoebiusStrip" |
                        "Surface3D" | "ParametricCurve3D" |
                        "Attractor3D" | "HyperSurface4D" | "VectorField3D"
                    );
                    let is_3d_view = app.current_view == ViewMode::D3;
                    if is_3d_object != is_3d_view {
                        continue;
                    }

                    // Pencil es una herramienta de dibujo libre. Permitimos
                    // que aparezca en el panel de Álgebra para que el usuario
                    // pueda borrarlo/ocultarlo; antes se filtraba, pero eso
                    // dejaba PencilObj persistidos sin forma de gestionarlos.

                    let o_col = obj.color();
                    let col = Color32::from_rgba_unmultiplied(
                        (o_col.r * 255.0).clamp(0.0, 255.0) as u8,
                        (o_col.g * 255.0).clamp(0.0, 255.0) as u8,
                        (o_col.b * 255.0).clamp(0.0, 255.0) as u8,
                        255,
                    );
                    let expr = match obj {
                        grafito_core::GeoObject::Function(f) => f.expr.clone(),
                        grafito_core::GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
                        grafito_core::GeoObject::Line(l) => {
                            let dx = l.end.x - l.start.x;
                            let dy = l.end.y - l.start.y;
                            let len = (dx * dx + dy * dy).sqrt();
                            format!(
                                "({:.2}, {:.2}) ↔ ({:.2}, {:.2})  L={:.3}",
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
                    (computed_display, obj.name().to_string(), obj.is_visible(), col, expr)
                };

                let is_sel = app.selected_object == Some(oid);
                let frame_fill = if is_sel {
                    theme.accent_muted
                } else {
                    theme.panel_bg
                };
                let border = if is_sel {
                    egui::Stroke::new(1.0, theme.accent)
                } else {
                    egui::Stroke::new(1.0, theme.separator)
                };

                let mut row_clicked = false;
                ui.add_space(4.0);
                egui::Frame::none()
                    .fill(frame_fill)
                    .rounding(8.0)
                    .stroke(border)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        let card_max_w = ui.available_width().min(360.0);
                        ui.set_min_width(card_max_w);
                        ui.horizontal(|ui| {
                            // Right-side controls drawn first
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (del_rect, del_resp) = ui.allocate_exact_size(
                                    egui::vec2(28.0, 24.0),
                                    egui::Sense::click(),
                                );
                                if ui.is_rect_visible(del_rect) {
                                    draw_icon(ui.painter(), del_rect.shrink(4.0), Icon::Delete, theme.text_secondary);
                                }
                                if del_resp.on_hover_text("Eliminar").clicked() {
                                    delete_id = Some(oid);
                                }
                                let (eye_rect, eye_resp) = ui.allocate_exact_size(
                                    egui::vec2(28.0, 24.0),
                                    egui::Sense::click(),
                                );
                                if ui.is_rect_visible(eye_rect) {
                                    draw_icon(
                                        ui.painter(),
                                        eye_rect.shrink(4.0),
                                        if obj_vis { Icon::Eye } else { Icon::EyeOff },
                                        theme.text_secondary,
                                    );
                                }
                                if eye_resp.on_hover_text("Visibilidad").clicked() {
                                    if let Some(o) = app.document.get_object_mut(oid) {
                                        let v = o.is_visible(); o.set_visible(!v);
                                    }
                                }

                                // Left-side controls in remaining space
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let dot_alpha = if obj_vis { 255u8 } else { 80u8 };
                                    let dot_col = Color32::from_rgba_unmultiplied(
                                        obj_col.r(), obj_col.g(), obj_col.b(), dot_alpha);
                                    let (dot_r, dot_resp) = ui.allocate_exact_size(egui::vec2(12.0,12.0), egui::Sense::click());
                                    ui.painter().circle_filled(dot_r.center(), 6.0, dot_col);
                                    if dot_resp.hovered() {
                                        ui.painter().circle_stroke(dot_r.center(), 6.0, egui::Stroke::new(1.0, Color32::WHITE));
                                    }
                                    let dot_resp = dot_resp.on_hover_text("Cambiar color");
                                    if dot_resp.clicked() {
                                        let obj_color = app.document.get_object(oid).map(|o| o.color()).unwrap_or_else(|| grafito_geometry::Color::new(1.0, 1.0, 1.0, 1.0));
                                        app.active_color_picker = Some((oid, grafito_ui::color_picker::HsvColorPicker::new(obj_color)));
                                        row_clicked = true;
                                    }
                                    ui.add_space(5.0);

                                    let txt = if !obj_expr.is_empty() {
                                        format!("{}: {}", display_label, obj_expr)
                                    } else {
                                        format!("{}: {}", display_label, obj_name)
                                    };
                                    let lbl_resp = ui.add(egui::Label::new(
                                        egui::RichText::new(txt).size(13.0).color(txt_col)).sense(egui::Sense::click()).truncate());
                                    if lbl_resp.clicked() { row_clicked = true; }
                                    if lbl_resp.double_clicked() && !obj_expr.is_empty() && (obj_name == "Function" || obj_name == "Point") {
                                        app.input_text = format!("{}={}", display_label, obj_expr);
                                    }
                                });
                            });
                        });

                        // Properties Panel (Inline)
                        if is_sel {
                            // Property sliders
                            if let Some(obj) = app.document.get_object_mut(oid) {
                                ui.add_space(2.0);
                                ui.scope(|ui| {
                                    // Sin overrides de light mode: confiamos en
                                    // los tokens del theme LIGHT definidos en
                                    // grafito-ui/src/theme.rs.
                                    match obj {
                                        GeoObject::Line(l) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut l.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Circle(c) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut c.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Function(f) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut f.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Point(p) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("●").size(10.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Point3D(p) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("●").size(10.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut p.size, 1.0..=20.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Polygon(poly) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("〰").size(14.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut poly.width, 0.5..=10.0).trailing_fill(true));
                                            });
                                        }
                                        GeoObject::Pencil(pencil) => {
                                            ui.horizontal(|ui| {
                                                ui.add_space(20.0);
                                                ui.label(egui::RichText::new("✏").size(12.0).color(Color32::from_gray(130)));
                                                ui.add(egui::Slider::new(&mut pencil.width, 0.5..=20.0).trailing_fill(true));
                                            });
                                        }
                                        _ => {}
                                    }
                                });
                            }
                        }
                    });

                if row_clicked {
                    app.selected_object = if is_sel { None } else { Some(oid) };
                }
                ui.add_space(2.0);
            }
            if let Some(id) = delete_id {
                app.document.remove_object(id);
                if app.selected_object == Some(id) { app.selected_object = None; }
            }

            // Variables
            if !app.document.variables.is_empty() {
                ui.add_space(10.0);

                let vars: Vec<(String, f64)> = app.document.variables.clone().into_iter().collect();
                let mut var_to_delete = None;
                for (name, val) in &vars {
                    let mut v = *val;

                    // Asegurar que exista la meta-data de la variable (valores por defecto)
                    if !app.document.variable_meta.contains_key(name) {
                        app.document.variable_meta.insert(
                            name.clone(),
                            grafito_core::VariableMeta {
                                position: grafito_geometry::Point2::new(0.0, 0.0),
                                min: -5.0,
                                max: 5.0,
                                step: 0.1,
                                visible: true,
                                animating: false,
                                animation_speed: 1.0,
                            },
                        );
                    }

                    let (mut animating, mut min, mut max, step) = {
                        let Some(meta) = app.document.variable_meta.get(name) else {
                            continue;
                        };
                        (meta.animating, meta.min, meta.max, meta.step)
                    };

                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                // Top row: name = value and options
                                ui.horizontal(|ui| {
                                    // Make sure it looks like `a = 1.0`
                                    // Use format to limit decimals if it's an integer
                                    let val_str = if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.2}") };
                                    ui.label(egui::RichText::new(format!("{} = {}", name, val_str)).size(14.0).color(txt_col));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.menu_button("⚙", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label("Min:");
                                                ui.add(egui::DragValue::new(&mut min).speed(0.1));
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Max:");
                                                ui.add(egui::DragValue::new(&mut max).speed(0.1));
                                            });
                                            ui.separator();
                                            if ui.button("Borrar").clicked() {
                                                var_to_delete = Some(name.clone());
                                                ui.close_menu();
                                            }
                                        });
                                    });
                                });

                                ui.add_space(4.0);

                                // Bottom row: min, slider, max, play button
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("{}", min)).size(12.0));

                                    let mut sl_resp = None;
                                    ui.scope(|ui| {
                                        // Sin overrides de light mode: tokens del theme.

                                        let slider = egui::Slider::new(&mut v, min..=max)
                                            .step_by(step)
                                            .show_value(false)
                                            .trailing_fill(true);

                                        // Clamp para que el slider no se degenerate
                                        // en pantallas angostas con labels largos.
                                        let slider_w = (ui.available_width() - 50.0).max(40.0);
                                        sl_resp = Some(ui.add_sized([slider_w, 16.0], slider));
                                    });

                                    if let Some(sl_resp) = sl_resp {
                                        if sl_resp.changed() {
                                            app.document.set_variable(name.clone(), v);
                                            app.document.recompute_bound_parameters();
                                        }
                                    }

                                    ui.label(egui::RichText::new(format!("{}", max)).size(12.0));

                                    let (play_rect, play_resp) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::click(),
                                    );
                                    if ui.is_rect_visible(play_rect) {
                                        draw_icon(
                                            ui.painter(),
                                            play_rect.shrink(2.0),
                                            if animating { Icon::Pause } else { Icon::Play },
                                            if animating { theme.accent } else { theme.text_secondary },
                                        );
                                    }
                                    if play_resp
                                        .on_hover_text(if animating { "Detener animación" } else { "Animar variable" })
                                        .clicked()
                                    {
                                        animating = !animating;
                                    }
                                });
                            });
                        });

                    if let Some(meta) = app.document.variable_meta.get_mut(name) {
                        meta.animating = animating;
                        meta.min = min;
                        meta.max = max;
                    }

                    ui.add_space(2.0);
                    ui.separator();
                }
                if let Some(to_del) = var_to_delete {
                    app.document.remove_variable(&to_del);
                }
            }
        });
    });
}
