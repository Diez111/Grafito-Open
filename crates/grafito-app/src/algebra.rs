//! Algebra side panel: object list, inline property editors, variables,
//! and command input preview.

use crate::{commands, GrafitoApp, ViewMode};
use egui::{Color32, Key};
use grafito_core::{GeoObject, ObjectId};
use grafito_ui::icons::{draw_icon, Icon};
use grafito_ui::theme::current_theme;

pub(crate) fn draw_algebra_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let accent = theme.accent;
    let alg_fill = theme.panel_bg;
    let sep_col = theme.separator;
    let txt_col = theme.text_primary;
    let _txt_dim = theme.text_tertiary;

    egui::SidePanel::left("algebra_panel")
    .default_width(220.0)
    .min_width(160.0)
    .resizable(true)
    .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
    .show(ctx, |ui| {
        // Input row
        egui::Frame::none()
            .fill(theme.input_bg)
            .inner_margin(egui::Margin { left:8.0, right:8.0, top:6.0, bottom:6.0 })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                    ui.add_space(3.0);
                    let r = ui.add_sized(
                        [ui.available_width(), 22.0],
                        egui::TextEdit::singleline(&mut app.input_text)
                            .hint_text("Entrada...")
                            .frame(false)
                            .text_color(txt_col));
                    app.preview_object = None;
                    if !app.input_text.is_empty() {
                        app.preview_object = commands::parse_preview(&app.input_text);
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        app.save_state();
                        let input_was = app.input_text.clone();
                        let time = ui.ctx().input(|i| i.time);
                        app.execute_command_and_record(&input_was, time);
                    }
                });
            });
        ui.add(egui::Separator::default().spacing(0.0));

        // ── Object list — compact, 1 line each ──────────────────────
        egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
            let mut delete_id: Option<ObjectId> = None;
            let ids: Vec<ObjectId> = app.document.objects_iter().map(|(id,_)| *id).collect();
            for oid in ids {
                let (obj_label, obj_name, obj_vis, obj_col, obj_expr) = {
                    let Some(obj) = app.document.get_object(oid) else { continue; };

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

                    // El Pencil es una herramienta de dibujo libre, no un
                    // objeto analizable: no debe aparecer en el panel de álgebra.
                    if matches!(obj, grafito_core::GeoObject::Pencil(_)) {
                        continue;
                    }

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
                    (obj.label().to_string(), obj.name().to_string(), obj.is_visible(), col, expr)
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
                        ui.set_min_width(ui.available_width());
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
                                        format!("{}: {}", obj_label, obj_expr)
                                    } else {
                                        format!("{}: {}", obj_label, obj_name)
                                    };
                                    let lbl_resp = ui.add(egui::Label::new(
                                        egui::RichText::new(txt).size(13.0).color(txt_col)).sense(egui::Sense::click()).truncate());
                                    if lbl_resp.clicked() { row_clicked = true; }
                                    if lbl_resp.double_clicked() && !obj_expr.is_empty() && (obj_name == "Function" || obj_name == "Point") {
                                        app.input_text = format!("{}={}", obj_label, obj_expr);
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
                                    if !ui.visuals().dark_mode {
                                        ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(100);
                                        ui.style_mut().visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(80);
                                        ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                        ui.style_mut().visuals.selection.bg_fill = egui::Color32::BLACK;
                                    }
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
                                        if !ui.visuals().dark_mode {
                                            ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(100);
                                            ui.style_mut().visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(80);
                                            ui.style_mut().visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
                                            ui.style_mut().visuals.selection.bg_fill = egui::Color32::BLACK;
                                        }

                                        let slider = egui::Slider::new(&mut v, min..=max)
                                            .step_by(step)
                                            .show_value(false)
                                            .trailing_fill(true);

                                        sl_resp = Some(ui.add_sized([ui.available_width() - 50.0, 16.0], slider));
                                    });

                                    if let Some(sl_resp) = sl_resp {
                                        if sl_resp.changed() {
                                            app.document.set_variable(name.clone(), v);
                                            app.document.recompute_bound_parameters();
                                        }
                                    }

                                    ui.label(egui::RichText::new(format!("{}", max)).size(12.0));

                                    let play_icon = if animating { "⏸" } else { "▶" };
                                    if ui.add_sized([20.0, 20.0], egui::Button::new(play_icon).frame(false)).clicked() {
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
