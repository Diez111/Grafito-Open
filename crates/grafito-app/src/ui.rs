//! Top-level egui chrome: menu bar, toolbar, icon sidebar, input/status bars,
//! and the floating color-picker dialog.

use crate::{commands, GrafitoApp, ViewMode};
use egui::Color32;
use grafito_ui::theme::{DARK as THEME_DARK, LIGHT as THEME_LIGHT};
use grafito_ui::Tool;

pub(crate) fn draw_top_bar(app: &mut GrafitoApp, ctx: &egui::Context) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_top_bar");

    let is_dark = app.dark_mode;
    let accent = Color32::from_rgb(53, 132, 228); // GNOME blue
    let bar_fill = if is_dark {
        Color32::from_rgb(36, 36, 36)
    } else {
        Color32::WHITE
    };
    let side_fill = if is_dark {
        Color32::from_rgb(30, 30, 38)
    } else {
        Color32::from_rgb(250, 250, 252)
    };
    let sep_col = if is_dark {
        Color32::from_rgb(55, 55, 60)
    } else {
        Color32::from_rgb(175, 175, 180)
    };
    let _txt_col = if is_dark {
        Color32::WHITE
    } else {
        Color32::from_rgb(26, 26, 26)
    };

    // ── MENU BAR + QUICK CONTROLS ──
    egui::TopBottomPanel::top("menu_bar")
        .exact_height(32.0)
        .frame(
            egui::Frame::none()
                .fill(bar_fill)
                .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
        )
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Archivo", |ui| {
                    if ui.button("Nuevo").clicked() {
                        app.document.clear();
                    }
                    if ui.button("Abrir (Ctrl+O)").clicked() {
                        app.load_from_file();
                    }
                    if ui.button("Guardar (Ctrl+S)").clicked() {
                        app.save_to_file();
                    }
                    ui.separator();
                    if ui.button("Salir").clicked() {
                        std::process::exit(0);
                    }
                });
                ui.menu_button("Editar", |ui| {
                    if ui.button("Deshacer (Ctrl+Z)").clicked() {
                        app.undo();
                    }
                    if ui.button("Rehacer (Ctrl+Y)").clicked() {
                        app.redo();
                    }
                    if ui.button("Eliminar (Supr)").clicked() {
                        app.delete_selected();
                    }
                });
                ui.menu_button("Vista", |ui| {
                    ui.checkbox(&mut app.show_grid, "Mostrar cuadrícula");
                    ui.checkbox(&mut app.dark_mode, "Modo oscuro")
                        .clicked()
                        .then(|| {
                            if app.dark_mode {
                                THEME_DARK.apply(ui.ctx());
                            } else {
                                THEME_LIGHT.apply(ui.ctx());
                            }
                        });
                    ui.checkbox(&mut app.snap_to_grid, "Ajustar a cuadrícula")
                        .changed();
                    ui.separator();
                    let mut is_3d = app.current_view == ViewMode::D3;
                    if ui.checkbox(&mut is_3d, "Vista 3D").changed() {
                        app.current_view = if is_3d { ViewMode::D3 } else { ViewMode::D2 };
                    }
                    ui.checkbox(&mut app.exam_mode, "Modo examen");
                    ui.checkbox(&mut app.document.view_mut().x_log, "Eje X log");
                    ui.checkbox(&mut app.document.view_mut().y_log, "Eje Y log");
                    ui.separator();
                    ui.checkbox(&mut app.use_gpu, "Renderizado GPU");
                });
                ui.menu_button("Herramientas", |ui| {
                    ui.checkbox(&mut app.keyboard_visible, "Teclado visible");
                });
                ui.menu_button("Ayuda", |ui| {
                    if ui.button("Acerca de Grafito v0.9.0-alpha").clicked() {}
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Grafito")
                            .color(accent)
                            .strong()
                            .size(14.0),
                    );
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new(if app.dark_mode {
                                "Tema Claro"
                            } else {
                                "Tema Oscuro"
                            })
                            .frame(false),
                        )
                        .clicked()
                    {
                        app.dark_mode = !app.dark_mode;
                        if app.dark_mode {
                            THEME_DARK.apply(ui.ctx());
                        } else {
                            THEME_LIGHT.apply(ui.ctx());
                        }
                    }
                });
            });
        });

    // ── TOOLBAR (horizontal, with dropdown groups) ──
    egui::TopBottomPanel::top("toolbar_panel")
        .default_height(38.0)
        .min_height(38.0)
        .max_height(120.0)
        .frame(egui::Frame::none().fill(side_fill))
        .show(ctx, |ui| {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("ui_toolbar");
            grafito_ui::toolbar::toolbar(
                ui,
                &mut app.current_tool,
                app.current_view == ViewMode::D3,
            );
        });

    // ── LEFT SIDEBAR (56px, labeled tabs) ──
    let tabs: &[(&str, &str, &str)] = &[
        ("Álgebra", "∑", "Objetos, variables, comandos"),
        ("CAS", "⌨", "Cálculo simbólico paso a paso"),
        ("Tabla", "☰", "Valores numéricos x|f(x)"),
        ("Hoja", "⊞", "Hoja de cálculo"),
        ("Vista", "◎", "Cuadrícula, ejes, etiquetas"),
    ];
    egui::SidePanel::left("icon_bar")
        .exact_width(52.0)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(side_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("ui_sidebar");
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                for (i, (label, icon, tip)) in tabs.iter().enumerate() {
                    let active = app.sidebar_tab == i;
                    let bg = if active {
                        Color32::from_rgba_unmultiplied(53, 132, 228, 50)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let ic = if active {
                        accent
                    } else {
                        Color32::from_gray(130)
                    };

                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(46.0, 48.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        ui.painter().rect_filled(rect, 6.0, bg);
                        // Draw the icon
                        ui.painter().text(
                            rect.center() - egui::vec2(0.0, 6.0),
                            egui::Align2::CENTER_CENTER,
                            *icon,
                            egui::FontId::proportional(16.0),
                            ic,
                        );
                        // Draw the text
                        ui.painter().text(
                            rect.center() + egui::vec2(0.0, 12.0),
                            egui::Align2::CENTER_CENTER,
                            *label,
                            egui::FontId::proportional(9.0),
                            ic,
                        );
                    }

                    if resp.clicked() {
                        app.sidebar_tab = i;
                        if i == 3 {
                            app.show_spreadsheet = false; // Never auto-open right panel when switching to Hoja
                        }
                    }
                    resp.on_hover_text(*tip);
                    ui.add_space(2.0);
                }
            });
        });
}

pub(crate) fn draw_bottom_bar(app: &mut GrafitoApp, ctx: &egui::Context) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_bottom_bar");

    let is_dark = app.dark_mode;
    let accent = Color32::from_rgb(53, 132, 228);
    let sep_col = if is_dark {
        Color32::from_rgb(55, 55, 60)
    } else {
        Color32::from_rgb(175, 175, 180)
    };
    let txt_dim = if is_dark {
        Color32::from_gray(140)
    } else {
        Color32::from_gray(110)
    };
    let txt_col = if is_dark {
        Color32::WHITE
    } else {
        Color32::from_rgb(26, 26, 26)
    };

    // ── INPUT BAR (always visible, like GeoGebra) ──
    {
        let mut should_exec = false;
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(32.0)
            .frame(
                egui::Frame::none()
                    .fill(if is_dark {
                        Color32::from_rgb(32, 32, 40)
                    } else {
                        Color32::from_rgb(245, 246, 250)
                    })
                    .stroke(egui::Stroke::new(1.0, sep_col))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                    let r = ui.add_sized(
                        [ui.available_width() - 40.0, 22.0],
                        egui::TextEdit::singleline(&mut app.input_text)
                            .hint_text("Entrada... (ej: sin(x), A=(1,2), Derivative[x^2,x])")
                            .frame(false)
                            .text_color(txt_col),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        should_exec = true;
                    }
                    if ui
                        .add_sized(
                            [28.0, 22.0],
                            egui::Button::new(egui::RichText::new("▶").color(accent)),
                        )
                        .clicked()
                    {
                        should_exec = true;
                    }
                });
            });
        if should_exec && !app.input_text.is_empty() {
            app.save_state();
            let mut cmd = app.input_text.clone();
            app.cas_result =
                commands::process_input(&mut app.document, &mut cmd).unwrap_or_default();
            if !app.cas_result.is_empty() {
                if app.cas_history.len() > 20 {
                    app.cas_history.remove(0);
                }
                app.cas_history
                    .push(format!("> {}\n  {}", app.input_text, app.cas_result));
            }
            app.input_text.clear();
        }
    }

    // ── STATUS BAR ──
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(22.0)
        .frame(
            egui::Frame::none()
                .fill(if is_dark {
                    Color32::from_rgb(28, 28, 34)
                } else {
                    Color32::from_rgb(240, 241, 245)
                })
                .stroke(egui::Stroke::new(1.0, sep_col))
                .inner_margin(egui::Margin::symmetric(10.0, 1.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let coord_text = if let Some(pos) = app.last_mouse_pos {
                    let view = app.document.view();
                    let world = view.screen_to_world(glam::Vec2::new(pos.x, pos.y));
                    if view.x_log || view.y_log {
                        format!("x: {:.4}, y: {:.4}", world.x, world.y)
                    } else {
                        format!("x: {:.2}, y: {:.2}", world.x, world.y)
                    }
                } else {
                    "x: ---, y: ---".to_string()
                };
                ui.label(egui::RichText::new(coord_text).size(11.0).color(txt_dim));
                ui.add_space(16.0);
                let hint = if let Some(h) = app.pending_action_hint() {
                    h.to_string()
                } else {
                    match app.current_view {
                        ViewMode::D2 => match app.current_tool {
                            Tool::Select => {
                                "↖ Seleccionar: clic objeto, arrastrar vacío para mover vista"
                            }
                            Tool::Point => "· Punto: clic para crear",
                            Tool::Line => "╱ Recta: clic en dos puntos",
                            Tool::Circle => "○ Círculo: clic centro, clic borde",
                            Tool::Polygon => "△ Polígono: clic vértices, clic der para cerrar",
                            Tool::Function => "f(x) Función: escribe en la entrada",
                            Tool::Distance => "↔ Distancia: clic en dos puntos",
                            Tool::Angle => "∠ Ángulo: clic vértice, luego dos puntos",
                            Tool::Slider => "═ Deslizador: clic para crear variable",
                            Tool::Locus => "⌒ Locus: selecciona punto móvil, luego dependiente",
                            _ => "Espacio / clic medio: mover vista",
                        }
                        .to_string(),
                        ViewMode::D3 => {
                            "3D: clic izq pan (Select), der orbitar, rueda zoom".to_string()
                        }
                    }
                };
                if !hint.is_empty() {
                    ui.label(egui::RichText::new(hint).size(11.0).color(txt_dim));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} objetos", app.document.object_count()))
                            .size(11.0)
                            .color(txt_dim),
                    );
                });
            });
        });
}

pub(crate) fn draw_color_picker(app: &mut GrafitoApp, ctx: &egui::Context) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_color_picker");

    if let Some((oid, mut picker)) = app.active_color_picker.clone() {
        let mut keep_open = true;

        // Adjust the window design to be centered and not ugly
        egui::Window::new("🎨 Selector de Color")
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .fixed_size([320.0, 300.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                if picker.show(ui, &mut app.color_favorites) {
                    if let Some(o) = app.document.get_object_mut(oid) {
                        o.set_color(picker.to_color());
                    }
                    ctx.request_repaint();
                }
            });

        if keep_open {
            app.active_color_picker = Some((oid, picker));
        } else {
            app.active_color_picker = None;
        }
    }
}
