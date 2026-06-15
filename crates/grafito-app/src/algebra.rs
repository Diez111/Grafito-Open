//! Algebra side panel: object list, inline property editors, variables,
//! and command input preview.

use crate::{commands, GrafitoApp, ViewMode};
use egui::{Color32, Key};
use grafito_core::{GeoObject, ObjectId};

pub(crate) fn draw_algebra_panel(app: &mut GrafitoApp, ctx: &egui::Context) {
    let is_dark = app.dark_mode;
    let accent = Color32::from_rgb(53, 132, 228);
    let alg_fill = if is_dark {
        Color32::from_rgb(24, 26, 34)
    } else {
        Color32::from_rgb(248, 249, 252)
    };
    let sep_col = if is_dark {
        Color32::from_rgb(55, 55, 60)
    } else {
        Color32::from_rgb(175, 175, 180)
    };
    let txt_col = if is_dark {
        Color32::WHITE
    } else {
        Color32::from_rgb(26, 26, 26)
    };
    let _txt_dim = if is_dark {
        Color32::from_gray(140)
    } else {
        Color32::from_gray(110)
    };

    egui::SidePanel::left("algebra_panel")
    .default_width(220.0)
    .min_width(160.0)
    .resizable(true)
    .frame(egui::Frame::none().fill(alg_fill).stroke(egui::Stroke::new(1.0, sep_col)))
    .show(ctx, |ui| {
        // Input row
        egui::Frame::none()
            .fill(if is_dark { Color32::from_gray(33) } else { Color32::from_gray(248) })
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
                        let outcome = commands::process_input(&mut app.document, &mut app.input_text);
                        let time = ui.ctx().input(|i| i.time);
                        app.handle_command_outcome(outcome, time, &input_was);
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

                    let o_col = obj.color();
                    let col = Color32::from_rgba_unmultiplied(
                        (o_col.r * 255.0) as u8,
                        (o_col.g * 255.0) as u8,
                        (o_col.b * 255.0) as u8,
                        255,
                    );
                    let expr = match obj {
                        grafito_core::GeoObject::Function(f) => f.expr.clone(),
                        grafito_core::GeoObject::Point(p) => format!("({:.2}, {:.2})", p.position.x, p.position.y),
                        grafito_core::GeoObject::Line(l) => format!("({:.2}, {:.2}) ↔ ({:.2}, {:.2})", l.start.x, l.start.y, l.end.x, l.end.y),
                        grafito_core::GeoObject::Circle(c) => format!("(x - {:.2})² + (y - {:.2})² = {:.2}²", c.center.x, c.center.y, c.radius),
                        grafito_core::GeoObject::Ellipse(e) => format!("(x - {:.2})²/{:.2}² + (y - {:.2})²/{:.2}² = 1", e.center.x, e.center.y, e.rx, e.ry),
                        grafito_core::GeoObject::Polygon(p) => format!("{} vertices", p.vertices.len()),
                        grafito_core::GeoObject::Point3D(p) => format!("({:.2}, {:.2}, {:.2})", p.position.x, p.position.y, p.position.z),
                        grafito_core::GeoObject::Sphere3D(s) => format!("r={:.2} c=({:.2}, {:.2}, {:.2})", s.radius, s.center.x, s.center.y, s.center.z),
                        grafito_core::GeoObject::Cube3D(c) => format!("size={:.2}", c.size),
                        grafito_core::GeoObject::Segment3D(s) => format!("({:.2}, {:.2}, {:.2}) ↔ ({:.2}, {:.2}, {:.2})", s.a.x, s.a.y, s.a.z, s.b.x, s.b.y, s.b.z),
                        _ => String::new(),
                    };
                    (obj.label().to_string(), obj.name().to_string(), obj.is_visible(), col, expr)
                };

                let is_sel = app.selected_object == Some(oid);
                let frame_fill = if is_sel {
                    if is_dark { Color32::from_rgba_unmultiplied(94, 139, 255, 40) } else { Color32::from_rgba_unmultiplied(38, 99, 255, 30) }
                } else {
                    if is_dark { Color32::from_gray(30) } else { Color32::from_rgb(255, 255, 255) }
                };
                let border = if is_sel {
                    egui::Stroke::new(1.0, if is_dark { Color32::from_rgb(94, 139, 255) } else { Color32::from_rgb(38, 99, 255) })
                } else {
                    egui::Stroke::new(1.0, if is_dark { Color32::from_gray(40) } else { Color32::from_rgb(230, 230, 235) })
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
                                if ui.add_sized([28.0, 24.0], egui::Button::new("🗑").frame(false)).on_hover_text("Eliminar").clicked() {
                                    delete_id = Some(oid);
                                }
                                let eye = if obj_vis { "👁" } else { "Ø" };
                                if ui.add_sized([28.0, 24.0], egui::Button::new(eye).frame(false)).on_hover_text("Visibilidad").clicked() {
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
                                    _ => {}
                                }
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
                ui.add_space(6.0);
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Variables").size(11.0).color(Color32::from_gray(130)));
                
                let vars: Vec<(String, f64)> = app.document.variables.clone().into_iter().collect();
                for (name, val) in &vars {
                    let mut v = *val;
                    
                    // Asegurar que exista la meta-data de la variable (valores por defecto)
                    if !app.document.variable_meta.contains_key(name) {
                        app.document.variable_meta.insert(
                            name.clone(),
                            grafito_core::VariableMeta {
                                position: grafito_geometry::Point2::new(0.0, 0.0),
                                min: -10.0,
                                max: 10.0,
                                step: 0.1,
                                visible: true,
                                animating: false,
                                animation_speed: 1.0,
                            },
                        );
                    }
                    
                    if let Some(meta) = app.document.variable_meta.get_mut(name) {
                        ui.horizontal(|ui| {
                            ui.add_space(5.0);
                            
                            // Botón de Play/Pause
                            let play_icon = if meta.animating { "⏸" } else { "▶" };
                            if ui.add_sized([20.0, 20.0], egui::Button::new(play_icon).frame(false)).clicked() {
                                meta.animating = !meta.animating;
                            }
                            
                            ui.label(egui::RichText::new(name).size(12.0));
                            
                            let sl = egui::Slider::new(&mut v, meta.min..=meta.max).step_by(meta.step);
                            if ui.add_sized([100.0, 16.0], sl).changed() {
                                app.document.set_variable(name.clone(), v);
                                app.document.recompute_bound_parameters();
                            }
                        });
                    }
                }
            }
        });
    });
}
