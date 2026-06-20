//! Top-level egui chrome: menu bar, toolbar, icon sidebar, input/status bars,
//! and the floating color-picker dialog.

use crate::{commands, GrafitoApp, Perspective, ViewMode};
use egui::{Align2, Color32};
use grafito_ui::icons::{draw_icon, Icon};
use grafito_ui::theme::{current_theme, DARK, LIGHT};
use grafito_ui::toolbar::ToolGroupId;
use grafito_ui::Tool;

pub(crate) fn draw_top_bar(app: &mut GrafitoApp, ctx: &egui::Context) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_top_bar");

    let theme = current_theme(ctx);
    let accent = theme.accent;
    let bar_fill = theme.toolbar_bg;
    let side_fill = theme.sidebar_bg;
    let sep_col = theme.separator;
    let _txt_col = theme.text_primary;

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
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
                                DARK.apply(ui.ctx());
                            } else {
                                LIGHT.apply(ui.ctx());
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
                ui.menu_button("Perspectivas", |ui| {
                    let mut selected = app.perspective;
                    for p in Perspective::ALL {
                        ui.radio_value(
                            &mut selected,
                            p,
                            format!("{}  (Ctrl+Shift+{})", p.title(), p.shortcut_number()),
                        );
                    }
                    if selected != app.perspective {
                        app.set_perspective(selected);
                    }
                });
                ui.menu_button("Herramientas", |ui| {
                    ui.checkbox(&mut app.keyboard_visible, "Teclado visible");
                });
                ui.menu_button("Ayuda", |ui| {
                    if ui.button("Acerca de Grafito v1.0.0-beta").clicked() {}
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
                            DARK.apply(ui.ctx());
                        } else {
                            LIGHT.apply(ui.ctx());
                        }
                    }

                    ui.add_space(8.0);
                    let is_3d = app.current_view == ViewMode::D3;
                    let toggle_text = if is_3d { "2D Vista" } else { "3D Vista" };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(toggle_text)
                                    .color(if is_3d {
                                        Color32::from_rgb(16, 185, 129)
                                    } else {
                                        accent
                                    })
                                    .strong(),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Cambiar entre vista 2D y 3D")
                        .clicked()
                    {
                        app.current_view = if is_3d { ViewMode::D2 } else { ViewMode::D3 };
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
            // Filtra los grupos de herramientas según la perspectiva activa.
            let mut groups: Vec<ToolGroupId> =
                app.perspective.layout().visible_tool_groups.to_vec();
            let is_3d = app.current_view == ViewMode::D3;
            if is_3d && !groups.contains(&ToolGroupId::ThreeD) {
                groups.push(ToolGroupId::ThreeD);
            }
            grafito_ui::toolbar::toolbar_filtered(ui, &mut app.current_tool, &groups);
        });

    // ── LEFT SIDEBAR (56px, labeled tabs) ──
    let tabs: &[(&str, Icon, &str)] = &[
        ("Álgebra", Icon::Menu, "Objetos, variables, comandos"),
        (
            "Herram.",
            Icon::Settings,
            "Herramientas de construcción y análisis",
        ),
        ("CAS", Icon::Analyze, "Cálculo simbólico paso a paso"),
        ("Tabla", Icon::Function, "Valores numéricos x|f(x)"),
        ("Hoja", Icon::Grid, "Hoja de cálculo"),
        ("Vista", Icon::Eye, "Cuadrícula, ejes, etiquetas"),
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
                // ── Selector de Perspectivas (fila superior del sidebar) ──
                for p in Perspective::ALL {
                    let active = app.perspective == p;
                    let bg = if active {
                        theme.sidebar_tab_active_bg
                    } else {
                        Color32::TRANSPARENT
                    };
                    let ic_color = if active {
                        theme.sidebar_tab_active
                    } else {
                        theme.sidebar_tab_inactive
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(46.0, 22.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        ui.painter().rect_filled(rect, 5.0, bg);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            p.short_label(),
                            egui::FontId::proportional(11.0),
                            ic_color,
                        );
                    }
                    if resp.clicked() {
                        app.set_perspective(p);
                    }
                    resp.on_hover_text(p.title());
                    ui.add_space(1.0);
                }
                // Separador entre perspectivas y tabs.
                ui.painter().line_segment(
                    [
                        egui::pos2(ui.min_rect().min.x + 8.0, ui.min_rect().min.y),
                        egui::pos2(ui.min_rect().max.x - 8.0, ui.min_rect().min.y),
                    ],
                    egui::Stroke::new(1.0, sep_col),
                );
                ui.add_space(4.0);
                // ── Tabs existentes del sidebar ──
                for (i, (label, icon, tip)) in tabs.iter().enumerate() {
                    let active = app.sidebar_tab == i;
                    let bg = if active {
                        theme.sidebar_tab_active_bg
                    } else {
                        Color32::TRANSPARENT
                    };
                    let ic_color = if active {
                        theme.sidebar_tab_active
                    } else {
                        theme.sidebar_tab_inactive
                    };

                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(46.0, 48.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        ui.painter().rect_filled(rect, 6.0, bg);
                        // Icono vectorial (sin emojis, sin letras sueltas)
                        let icon_rect = egui::Rect::from_center_size(
                            rect.center() - egui::vec2(0.0, 7.0),
                            egui::vec2(20.0, 20.0),
                        );
                        draw_icon(ui.painter(), icon_rect, *icon, ic_color);
                        // Texto debajo
                        ui.painter().text(
                            rect.center() + egui::vec2(0.0, 14.0),
                            Align2::CENTER_CENTER,
                            *label,
                            egui::FontId::proportional(9.0),
                            ic_color,
                        );
                    }

                    if resp.clicked() {
                        app.sidebar_tab = i;
                        if i == 4 {
                            // index 4 is now Hoja
                            app.show_spreadsheet = true;
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

    let theme = current_theme(ctx);
    let accent = theme.accent;
    let sep_col = theme.separator;
    let txt_dim = theme.text_tertiary;
    let txt_col = theme.text_primary;

    // ── INPUT BAR (always visible, like GeoGebra) ──
    {
        let mut should_exec = false;
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(32.0)
            .frame(
                egui::Frame::none()
                    .fill(theme.input_bar_bg)
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
            let input_was = app.input_text.clone();
            let outcome = commands::process_input(&mut app.document, &mut cmd);
            let time = ctx.input(|i| i.time);
            app.handle_command_outcome(outcome, time, &input_was);
            app.input_text.clear();
        }
    }

    // ── STATUS BAR ──
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(22.0)
        .frame(
            egui::Frame::none()
                .fill(theme.status_bar_bg)
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
                        ViewMode::D2 => status_hint_for_tool(app.current_tool),
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

fn status_hint_for_tool(tool: Tool) -> String {
    match tool {
        Tool::Select => "↖ Seleccionar: clic objeto, arrastrar vacío para mover vista".to_string(),
        Tool::Point => "· Punto: clic para crear".to_string(),
        Tool::Point3D => "· Punto 3D: clic para crear".to_string(),
        Tool::Line => "╱ Recta: clic en dos puntos".to_string(),
        Tool::Segment => "─ Segmento: clic en dos puntos".to_string(),
        Tool::Ray => "→ Semirrecta: clic origen, clic dirección".to_string(),
        Tool::Vector => "⇒ Vector: clic origen, clic extremo".to_string(),
        Tool::Circle => "○ Círculo: clic centro, clic borde".to_string(),
        Tool::Polygon => "△ Polígono: clic vértices, clic der para cerrar".to_string(),
        Tool::RegularPolygon => "⬡ Polígono regular: clic centro, clic vértice".to_string(),
        Tool::Function => "f(x) Función: clic para crear y editar".to_string(),
        Tool::Distance => "↔ Distancia: clic en dos puntos".to_string(),
        Tool::DistanceConstraint => "↔ Restricción Distancia: clic en dos puntos".to_string(),
        Tool::Angle => "∠ Ángulo: clic en 3 puntos (vértice, brazo 1, brazo 2)".to_string(),
        Tool::AngleConstraint => "∠ Restricción Ángulo: clic en dos rectas".to_string(),
        Tool::Area => "⬜ Área: clic en polígono o círculo".to_string(),
        Tool::Slope => "m Pendiente: clic en recta".to_string(),
        Tool::Slider => "═ Deslizador: clic para crear variable".to_string(),
        Tool::Locus => "⌒ Locus: selecciona punto móvil, luego dependiente".to_string(),
        Tool::Midpoint => "M Punto medio: clic en dos puntos".to_string(),
        Tool::Perpendicular => "⟂ Perpendicular: clic en dos puntos".to_string(),
        Tool::Tangent => "⌒ Tangente: selecciona círculo y recta".to_string(),
        Tool::Root => "x₀ Raíces: clic en una función".to_string(),
        Tool::Extremum => "max Extremos: clic en una función".to_string(),
        Tool::Intersect => "× Intersección: clic en dos objetos".to_string(),
        Tool::Coincident => "● Coincidente: selecciona dos puntos".to_string(),
        Tool::Horizontal => "─ Horizontal: selecciona una recta".to_string(),
        Tool::Vertical => "│ Vertical: selecciona una recta".to_string(),
        Tool::EqualLength => "= Longitud igual: selecciona dos segmentos".to_string(),
        Tool::Symmetry => "⇄ Simetría: punto, imagen, eje".to_string(),
        Tool::EllipseByFoci => "⬭ Elipse: dos focos y un punto".to_string(),
        Tool::ParabolaByFocusDirectrix => "⩗ Parábola: foco y directriz".to_string(),
        Tool::HyperbolaByFoci => "⩘ Hipérbola: dos focos y un punto".to_string(),
        Tool::ConicByFivePoints => "C5 Cónica: cinco puntos".to_string(),
        Tool::PolygonUnion => "∪ Unión: dos polígonos".to_string(),
        Tool::PolygonIntersection => "∩ Intersección: dos polígonos".to_string(),
        Tool::PolygonDifference => "\\ Diferencia: dos polígonos".to_string(),
        Tool::PolygonXor => "⊕ XOR: dos polígonos".to_string(),
        Tool::Sphere3D => "◯ Esfera 3D: clic centro y borde".to_string(),
        Tool::Cube3D => "□ Cubo 3D: clic centro y borde".to_string(),
        _ => "Espacio / clic medio: mover vista".to_string(),
    }
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
            .default_width(330.0)
            .fixed_size([330.0, 280.0])
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
