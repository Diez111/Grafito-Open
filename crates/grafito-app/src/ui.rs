//! Top-level egui chrome: menu bar, toolbar, icon sidebar, input/status bars,
//! and the floating color-picker dialog.

use crate::app::AutocompleteItem;
use crate::{GrafitoApp, Perspective, ViewMode};
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
                    ui.separator();
                    if ui
                        .checkbox(&mut app.show_trig_animation, "Animación Trigonométrica")
                        .changed()
                    {
                        ui.close_menu();
                    }
                });
                ui.menu_button("Ayuda", |ui| {
                    let version = env!("CARGO_PKG_VERSION");
                    if ui
                        .button(format!("Acerca de Grafito v{}", version))
                        .clicked()
                    {
                        app.show_about = true;
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // El switch 2D/3D ahora vive en el selector de perspectivas del
                    // sidebar. El menú top bar sólo muestra marca + toggle de tema.
                    if ui.available_width() > 700.0 {
                        ui.label(
                            egui::RichText::new("Grafito")
                                .color(accent)
                                .strong()
                                .size(14.0),
                        );
                        ui.add_space(4.0);
                    }
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
    // 6 tabs armonizados: un icono representativo por panel + etiqueta corta
    // legible. Las perspectivas se cambian con el dropdown arriba, no más
    // array vertical de 10 elementos que saturaba el sidebar.
    let tabs: &[(&str, Icon, &str)] = &[
        ("Álgebra", Icon::Function, "Objetos, variables y comandos"),
        (
            "Herram.",
            Icon::Settings,
            "Herramientas de construcción y análisis",
        ),
        ("CAS", Icon::Analyze, "Cálculo simbólico paso a paso"),
        ("Tabla", Icon::Grid, "Tabla de valores x|f(x), estadística"),
        ("Hoja", Icon::Histogram, "Hoja de cálculo y datos"),
        ("Vista", Icon::Eye, "Cuadrícula, ejes y estilo"),
    ];
    egui::SidePanel::left("icon_bar")
        .exact_width(56.0)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(side_fill)
                .stroke(egui::Stroke::new(1.0, sep_col)),
        )
        .show(ctx, |ui| {
            #[cfg(feature = "profile")]
            puffin::profile_scope!("ui_sidebar");
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);

                        // ── Botón Perspectiva (dropdown compacto) ──
                        // Reemplaza las 10 lineas verticales de short_labels.
                        // Una sola celda arriba anuncia la perspectiva activa y
                        // al click abre el popup con las 10 opciones.
                        let cur_p = app.perspective;
                        let active_pbg = theme.sidebar_tab_active_bg;
                        let active_txt = theme.sidebar_tab_active;
                        let dropdown_rect_w = 46.0;
                        let dropdown_rect_h = 38.0;
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(dropdown_rect_w, dropdown_rect_h),
                            egui::Sense::click(),
                        );
                        if ui.is_rect_visible(rect) {
                            ui.painter().rect_filled(rect, 6.0, active_pbg);
                            // Icono del menú Hamburguesa arriba
                            let icon_rect = egui::Rect::from_center_size(
                                rect.center() - egui::vec2(0.0, 8.0),
                                egui::vec2(18.0, 14.0),
                            );
                            draw_icon(ui.painter(), icon_rect, Icon::Menu, active_txt);
                            // short_label abajo (G2, AL, etc.) en proporcional 9
                            ui.painter().text(
                                rect.center() + egui::vec2(0.0, 11.0),
                                Align2::CENTER_CENTER,
                                cur_p.short_label(),
                                egui::FontId::proportional(9.0),
                                active_txt,
                            );
                        }
                        // Popup con las 10 perspectivas: open en click izquierdo.
                        let popup_id = ui.make_persistent_id("sidebar_perspective_popup");
                        if resp.clicked() {
                            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                        }
                        egui::popup::popup_below_widget(
                            ui,
                            popup_id,
                            &resp,
                            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(220.0);
                                let mut selected = app.perspective;
                                for p in Perspective::ALL {
                                    ui.radio_value(
                                        &mut selected,
                                        p,
                                        format!(
                                            "{}  (Ctrl+Shift+{})",
                                            p.title(),
                                            p.shortcut_number()
                                        ),
                                    );
                                }
                                if selected != app.perspective {
                                    app.set_perspective(selected);
                                    ui.memory_mut(|mem| mem.close_popup());
                                }
                            },
                        );
                        resp.on_hover_text(format!(
                            "Perspectiva actual: {}  (Ctrl+Shift+{}) — click para cambiar",
                            cur_p.title(),
                            cur_p.shortcut_number()
                        ));

                        // Separador entre el botón perspectiva y los tabs.
                        ui.add_space(6.0);
                        ui.painter().line_segment(
                            [
                                egui::pos2(ui.min_rect().min.x + 10.0, ui.min_rect().min.y),
                                egui::pos2(ui.min_rect().max.x - 10.0, ui.min_rect().min.y),
                            ],
                            egui::Stroke::new(1.0, sep_col),
                        );
                        ui.add_space(6.0);

                        // ── Tabs del sidebar (6, uno por panel izquierdo) ──
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

                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(46.0, 48.0), egui::Sense::click());
                            if ui.is_rect_visible(rect) {
                                ui.painter().rect_filled(rect, 6.0, bg);
                                let icon_rect = egui::Rect::from_center_size(
                                    rect.center() - egui::vec2(0.0, 7.0),
                                    egui::vec2(20.0, 20.0),
                                );
                                draw_icon(ui.painter(), icon_rect, *icon, ic_color);
                                ui.painter().text(
                                    rect.center() + egui::vec2(0.0, 14.0),
                                    Align2::CENTER_CENTER,
                                    *label,
                                    egui::FontId::proportional(9.5),
                                    ic_color,
                                );
                            }

                            if resp.clicked() {
                                app.sidebar_tab = i;
                            }
                            resp.on_hover_text(*tip);
                            ui.add_space(3.0);
                        }

                        ui.add_space(8.0);
                    });
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
                    let has_focus = r.has_focus();

                    // Sugerencias de autocompletado (comandos, objetos, variables).
                    let suggestions = if !app.input_text.is_empty() {
                        compute_autocomplete_suggestions(&app.input_text, &app.document)
                    } else {
                        Vec::new()
                    };
                    let show_popup = !suggestions.is_empty() && has_focus;
                    app.autocomplete.open = show_popup;

                    if show_popup {
                        if app.autocomplete.selected >= suggestions.len() {
                            app.autocomplete.selected = 0;
                        }
                        let len = suggestions.len();
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            app.autocomplete.selected = (app.autocomplete.selected + 1) % len;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            app.autocomplete.selected = (app.autocomplete.selected + len - 1) % len;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            app.autocomplete.open = false;
                            app.autocomplete.selected = 0;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if let Some(item) = suggestions.get(app.autocomplete.selected) {
                                apply_autocomplete_item(&mut app.input_text, item);
                                app.autocomplete.open = false;
                                app.autocomplete.selected = 0;
                                r.request_focus();
                            }
                        }
                    } else if r.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !app.input_text.is_empty()
                    {
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

                    // Popup de autocompletado (Area flotante bajo el input bar).
                    if show_popup {
                        let popup_pos = egui::pos2(r.rect.min.x, r.rect.max.y);
                        let selected = app.autocomplete.selected;
                        let display: Vec<(String, String)> = suggestions
                            .iter()
                            .take(8)
                            .map(|it| (it.text.clone(), it.detail.clone()))
                            .collect();
                        let popup_id = ui.id().with("autocomplete_popup");
                        let mut clicked: Option<usize> = None;
                        egui::Area::new(popup_id)
                            .fixed_pos(popup_pos)
                            .order(egui::Order::Foreground)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    for (i, (text, detail)) in display.iter().enumerate() {
                                        let is_sel = i == selected;
                                        let resp = ui.add(egui::SelectableLabel::new(
                                            is_sel,
                                            format!("{}  · {}", text, detail),
                                        ));
                                        if resp.clicked() {
                                            clicked = Some(i);
                                        }
                                    }
                                });
                            });
                        if let Some(i) = clicked {
                            if let Some(item) = suggestions.get(i) {
                                apply_autocomplete_item(&mut app.input_text, item);
                                app.autocomplete.open = false;
                                app.autocomplete.selected = 0;
                                r.request_focus();
                            }
                        }
                    }
                });
            });
        if should_exec && !app.input_text.is_empty() {
            let time = ctx.input(|i| i.time);
            app.submit_input_text(time);
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
        egui::Window::new("Selector de Color")
            .collapsible(false)
            .resizable(true)
            .default_size([330.0, 280.0])
            .min_size([260.0, 240.0])
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

// ─────────────────────────────────────────────────────────────────────────
// Autocompletado de la barra de entrada
// ─────────────────────────────────────────────────────────────────────────

/// Coincidencia difusa por subsecuencia (case-insensitive).
/// Devuelve `true` si todos los caracteres de `needle` aparecen en `haystack`
/// en el mismo orden (no necesariamente contiguos).
fn fuzzy_match(needle: &str, haystack: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack_lower = haystack.to_lowercase();
    let mut it = haystack_lower.chars();
    needle
        .to_lowercase()
        .chars()
        .all(|nc| it.any(|hc| hc == nc))
}

/// Calcula hasta 8 sugerencias de autocompletado para el texto de entrada,
/// combinando comandos de la paleta, etiquetas de objetos y variables del
/// documento. Ordena primero las que comienzan con el prefijo escrito.
fn compute_autocomplete_suggestions(
    input: &str,
    document: &grafito_core::Document,
) -> Vec<AutocompleteItem> {
    let mut items: Vec<AutocompleteItem> = Vec::new();

    for cmd in grafito_ui::command_palette::all_commands() {
        if fuzzy_match(input, cmd.name) {
            items.push(AutocompleteItem {
                text: cmd.name.to_string(),
                detail: cmd.category.to_string(),
                bracket: cmd.syntax_hint.contains('['),
            });
        }
    }

    for (_, obj) in document.objects_iter() {
        let label = obj.label();
        if !label.is_empty() && fuzzy_match(input, label) {
            items.push(AutocompleteItem {
                text: label.to_string(),
                detail: obj.name().to_string(),
                bracket: false,
            });
        }
    }

    for k in document.variables().keys() {
        if fuzzy_match(input, k) {
            items.push(AutocompleteItem {
                text: k.clone(),
                detail: "variable".to_string(),
                bracket: false,
            });
        }
    }

    let input_lower = input.to_lowercase();
    items.sort_by(|a, b| {
        let ap = a.text.to_lowercase().starts_with(&input_lower);
        let bp = b.text.to_lowercase().starts_with(&input_lower);
        bp.cmp(&ap)
            .then_with(|| a.text.to_lowercase().cmp(&b.text.to_lowercase()))
    });
    items.truncate(8);
    items
}

/// Reemplaza el token actual (el fragmento que se está escribiendo tras el
/// último separador) por el item seleccionado. Para comandos bracket, añade
/// `[` al final para que el usuario complete los argumentos.
fn apply_autocomplete_item(input: &mut String, item: &AutocompleteItem) {
    let separators = ['[', '(', ',', ' ', '\t', '='];
    let token_start = input
        .rfind(|c: char| separators.contains(&c))
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &input[..token_start];
    if item.bracket {
        *input = format!("{}{}[", prefix, item.text);
    } else {
        *input = format!("{}{}", prefix, item.text);
    }
}
