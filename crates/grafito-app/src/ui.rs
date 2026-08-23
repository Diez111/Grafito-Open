//! Top-level egui chrome: menu bar, toolbar, icon sidebar, input/status bars,
//! and the floating color-picker dialog.

use crate::app::{
    ActiveColorPicker, AutocompleteItem, ColorPickerTarget, DeferredPanelSnapshot, DocumentAction,
    FileCommand, UnsavedDecision,
};
use crate::{GrafitoApp, Perspective, ViewMode, WorkspaceDockTab};
use egui::{Align2, Color32};
use grafito_ui::animation::interpolate_color;
use grafito_ui::icons::{action_icon_button, draw_icon, Icon};
use grafito_ui::theme::{current_theme, DARK, LIGHT};
use grafito_ui::tokens::{
    RADIUS_MD, SPACE_MD, SPACE_SM, SPACING_BUTTON_X, SPACING_BUTTON_Y, SPACING_MINIMAL_X,
    SPACING_MINIMAL_Y, TOP_BAR_HEIGHT, TYPE_BASE,
};
use grafito_ui::toolbar::ToolGroupId;
use grafito_ui::Tool;

pub(crate) struct CommandInputResponse {
    pub submitted: bool,
    pub changed: bool,
}

/// The native minimum is 960 logical points; this also covers HiDPI windows
/// whose physical captures look much narrower than their egui width.
pub(crate) const COMPACT_TOP_CHROME_MAX_WIDTH: f32 = 1_120.0;

pub(crate) fn top_chrome_uses_overflow(viewport_width: f32) -> bool {
    viewport_width <= COMPACT_TOP_CHROME_MAX_WIDTH
}

pub(crate) const fn assistant_reopen_control_visible(assistant_visible: bool) -> bool {
    !assistant_visible
}

pub(crate) fn restore_assistant_visibility(
    assistant_visible: &mut bool,
    reopen_requested: bool,
) -> bool {
    if !reopen_requested || !assistant_reopen_control_visible(*assistant_visible) {
        return false;
    }
    *assistant_visible = true;
    true
}

pub(crate) fn draw_assistant_reopen_control(
    ui: &mut egui::Ui,
    assistant_visible: &mut bool,
    accent: Color32,
) -> Option<egui::Response> {
    if !assistant_reopen_control_visible(*assistant_visible) {
        return None;
    }
    let response = ui
        .button(egui::RichText::new("Asistente").color(accent))
        .on_hover_text("Mostrar asistente");
    let _ = restore_assistant_visibility(assistant_visible, response.clicked());
    Some(response)
}

fn draw_file_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    ui.menu_button("Archivo", |ui| {
        if ui.button("Nuevo (Ctrl+N)").clicked() {
            app.handle_file_command(FileCommand::New);
            ui.close_menu();
        }
        if ui.button("Abrir... (Ctrl+O)").clicked() {
            app.handle_file_command(FileCommand::Open);
            ui.close_menu();
        }
        if ui.button("Guardar (Ctrl+S)").clicked() {
            app.handle_file_command(FileCommand::Save);
            ui.close_menu();
        }
        if ui.button("Guardar como... (Ctrl+Shift+S)").clicked() {
            app.handle_file_command(FileCommand::SaveAs);
            ui.close_menu();
        }
        ui.menu_button("Exportar", |ui| {
            for (label, format) in [
                ("SVG...", crate::export::ExportFormat::Svg),
                ("PNG...", crate::export::ExportFormat::Png),
                ("TikZ...", crate::export::ExportFormat::Tikz),
            ] {
                if ui.button(label).clicked() {
                    app.export_with_dialog(format);
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        if ui.button("Salir").clicked() {
            app.handle_file_command(FileCommand::Exit);
            ui.close_menu();
        }
    });
}

fn draw_edit_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
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
}

fn draw_view_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
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
}

fn draw_perspectives_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    ui.menu_button("Perspectivas", |ui| {
        let mut selected = app.perspective;
        for perspective in Perspective::ALL {
            ui.radio_value(
                &mut selected,
                perspective,
                format!(
                    "{}  (Ctrl+Shift+{})",
                    perspective.title(),
                    perspective.shortcut_number()
                ),
            );
        }
        if selected != app.perspective {
            app.set_perspective(selected);
        }
        ui.separator();
        if ui.button("Cargar ejemplo de esta perspectiva").clicked() {
            if app.document.object_count() == 0 {
                match app.load_perspective_examples(app.perspective) {
                    Ok(()) => app.notify(
                        "Ejemplo cargado para la perspectiva actual",
                        grafito_ui::toast::ToastKind::Success,
                    ),
                    Err(error) => app.handle_command_outcome(
                        grafito_command::commands::CommandOutcome::Error(error),
                        ui.ctx().input(|input| input.time),
                        "Cargar ejemplo de perspectiva",
                    ),
                }
            } else {
                app.notify(
                    "No se cargó el ejemplo: el documento ya tiene objetos",
                    grafito_ui::toast::ToastKind::Info,
                );
            }
            ui.close_menu();
        }
    });
}

fn draw_tools_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    ui.menu_button("Herramientas", |ui| {
        if ui
            .checkbox(&mut app.keyboard_visible, "Teclado visible")
            .changed()
            && !app.keyboard_visible
        {
            app.keyboard_expanded = false;
        }
        if ui
            .checkbox(&mut app.assistant_visible, "Asistente visible")
            .changed()
            && app.assistant_visible
        {
            app.open_assistant_workspace();
        }
        ui.separator();
        let mut trig_visible = app.show_trig_animation;
        if ui
            .add_enabled(
                crate::app::trig_animation_supported(app.current_view),
                egui::Checkbox::new(&mut trig_visible, "Animación Trigonométrica"),
            )
            .changed()
        {
            app.set_trig_animation_visible(trig_visible);
            ui.close_menu();
        }
        if !crate::app::trig_animation_supported(app.current_view) {
            ui.label(
                egui::RichText::new("Disponible en vistas 2D.")
                    .color(current_theme(ui.ctx()).text_tertiary)
                    .size(11.0),
            );
        }
    });
}

fn draw_panels_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    ui.menu_button("Paneles", |ui| {
        for (tab, label) in [
            (0, "Álgebra"),
            (1, "Herramientas"),
            (2, "CAS"),
            (3, "Vista"),
        ] {
            let selected = app.compact_drawer_open && app.sidebar_tab == tab;
            if ui.selectable_label(selected, label).clicked() {
                app.sidebar_tab = tab;
                app.compact_drawer_open = true;
                ui.close_menu();
            }
        }
        if app.perspective == Perspective::Geometry3D {
            ui.separator();
            let inspector_selected = app.compact_geometry_utility_open
                && app.workspace_dock_tab == WorkspaceDockTab::Inspector;
            if ui
                .selectable_label(inspector_selected, "Inspector 3D")
                .clicked()
            {
                app.workspace_dock_tab = WorkspaceDockTab::Inspector;
                app.compact_geometry_utility_open = true;
                ui.close_menu();
            }
            let assistant_selected = app.compact_geometry_utility_open
                && app.workspace_dock_tab == WorkspaceDockTab::Assistant;
            if ui
                .selectable_label(assistant_selected, "Asistente 3D")
                .clicked()
            {
                app.open_assistant_workspace();
                ui.close_menu();
            }
            if app.compact_geometry_utility_open {
                ui.separator();
                if ui.button("Ocultar utilidad 3D").clicked() {
                    app.compact_geometry_utility_open = false;
                    ui.close_menu();
                }
            }
        }
        if app.compact_drawer_open {
            ui.separator();
            if ui.button("Ocultar panel").clicked() {
                app.compact_drawer_open = false;
                ui.close_menu();
            }
        }
    });
}

fn draw_help_menu(ui: &mut egui::Ui, app: &mut GrafitoApp) {
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
}

pub(crate) fn draw_top_bar(
    app: &mut GrafitoApp,
    ctx: &egui::Context,
    show_sidebar: bool,
    show_compact_panel_menu: bool,
    show_right_drawer_toggle: bool,
) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_top_bar");

    let theme = current_theme(ctx);
    let accent = theme.accent;
    let bar_fill = theme.panel_bg;
    let side_fill = theme.sidebar_bg;
    let sep_col = theme.separator;

    // ── Scandinavian single bar — 48 px, hairline 10% solo abajo
    egui::TopBottomPanel::top("top_bar")
        .exact_height(TOP_BAR_HEIGHT)
        .frame(
            egui::Frame::none()
                .fill(bar_fill)
                .stroke(egui::Stroke::new(1.0, sep_col.gamma_multiply(0.6)))
                .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(SPACING_MINIMAL_X, SPACING_MINIMAL_Y);
            ui.spacing_mut().button_padding = egui::vec2(SPACING_BUTTON_X, SPACING_BUTTON_Y);
            let bar_rect = ui.max_rect();
            egui::menu::bar(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(SPACING_MINIMAL_X, SPACING_MINIMAL_Y);
                ui.spacing_mut().button_padding = egui::vec2(SPACING_BUTTON_X, SPACING_BUTTON_Y);
                let compact_top_chrome = top_chrome_uses_overflow(ctx.screen_rect().width());
                if compact_top_chrome {
                    draw_file_menu(ui, app);
                    draw_edit_menu(ui, app);
                    if show_compact_panel_menu {
                        draw_panels_menu(ui, app);
                    }
                    ui.menu_button("Más", |ui| {
                        draw_view_menu(ui, app);
                        draw_perspectives_menu(ui, app);
                        draw_tools_menu(ui, app);
                        if !show_compact_panel_menu {
                            draw_panels_menu(ui, app);
                        }
                        draw_help_menu(ui, app);
                    });
                } else {
                    draw_file_menu(ui, app);
                    draw_edit_menu(ui, app);
                    draw_view_menu(ui, app);
                    draw_perspectives_menu(ui, app);
                    draw_tools_menu(ui, app);
                    if show_compact_panel_menu {
                        draw_panels_menu(ui, app);
                    }
                    draw_help_menu(ui, app);
                }

                // ── Toolbar — aire, scroll horizontal si no entra
                ui.add_space(SPACE_MD);
                {
                    let mut groups: Vec<ToolGroupId> =
                        app.perspective.layout().visible_tool_groups.to_vec();
                    let is_3d = app.current_view == ViewMode::D3;
                    if is_3d && !groups.contains(&ToolGroupId::ThreeD) {
                        groups.push(ToolGroupId::ThreeD);
                    }
                    // Reserva para controles derecha (~260px) y evita empujar fuera de pantalla
                    let avail_for_toolbar = (ui.available_width() - 280.0).clamp(140.0, 520.0);
                    egui::ScrollArea::horizontal()
                        .id_salt("top_toolbar_scroll")
                        .auto_shrink([true, false])
                        .show(ui, |ui| {
                            // Fuerza ancho mínimo para que ScrollArea sepa cuando scrollear
                            ui.set_min_width(avail_for_toolbar);
                            grafito_ui::toolbar::toolbar_inline(ui, &mut app.current_tool, &groups);
                        });
                }

                // ── Right controls — Pou habitáculo al lado del toggle tema ──
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing =
                        egui::vec2(SPACING_MINIMAL_X, SPACING_MINIMAL_Y);
                    ui.spacing_mut().button_padding =
                        egui::vec2(SPACING_BUTTON_X, SPACING_BUTTON_Y);

                    if show_right_drawer_toggle
                        && action_icon_button(
                            ui,
                            Icon::Menu,
                            accent,
                            if app.right_drawer_open {
                                "Ocultar panel contextual"
                            } else {
                                "Mostrar panel contextual"
                            },
                        )
                        .clicked()
                    {
                        app.right_drawer_open = !app.right_drawer_open;
                    }
                    // Whiteboard solo en wide, para no desbordar en 1120
                    if !compact_top_chrome
                        && action_icon_button(
                            ui,
                            Icon::Whiteboard,
                            if app.whiteboard_open {
                                accent
                            } else {
                                grafito_ui::theme::current_theme(ui.ctx()).text_secondary
                            },
                            "Pizarra libre (dibujo tipo Excalidraw)",
                        )
                        .clicked()
                    {
                        app.whiteboard_open = !app.whiteboard_open;
                    }
                    // Toggle tema Sun/Moon — Scandinavian calm, visible con fondo (panel/button_bg + borde separator)
                    {
                        let theme_toggle_bg = theme.button_bg;
                        let theme_toggle_hover = theme.button_hover;
                        let is_dark = app.dark_mode;
                        let toggle_icon = if is_dark { Icon::Sun } else { Icon::Moon };
                        let toggle_tip = if is_dark {
                            "Activar tema claro"
                        } else {
                            "Activar tema oscuro"
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
                        let response = response.on_hover_text(toggle_tip);
                        let bg = if response.hovered() {
                            theme_toggle_hover
                        } else {
                            theme_toggle_bg
                        };
                        if ui.is_rect_visible(rect) {
                            ui.painter().rect_filled(rect, RADIUS_MD, bg);
                            ui.painter().rect_stroke(
                                rect,
                                RADIUS_MD,
                                egui::Stroke::new(1.0, theme.separator),
                            );
                            draw_icon(ui.painter(), rect.shrink(3.0), toggle_icon, accent);
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, toggle_tip)
                        });
                        if response.clicked() {
                            app.dark_mode = !app.dark_mode;
                            if app.dark_mode {
                                DARK.apply(ui.ctx());
                            } else {
                                LIGHT.apply(ui.ctx());
                            }
                        }
                    }
                    // Nivel — solo en wide para no desbordar
                    if !compact_top_chrome {
                        let level = app.profile.level;
                        let coins = app.profile.mascot_mut_or_default().coins;
                        egui::Frame::none()
                            .fill(theme.accent.gamma_multiply(0.09))
                            .rounding(RADIUS_MD)
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("Nivel {level}"))
                                            .size(11.0)
                                            .strong()
                                            .color(theme.accent),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("· {coins}"))
                                            .size(11.0)
                                            .color(theme.text_tertiary),
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                    // Pou — ventana profesional Casa/Vestir/Jugar/Progreso (Scandinavian shell, playful)
                    {
                        let is_open = app.show_pou_window;
                        let bg = if is_open {
                            theme.accent.gamma_multiply(0.14)
                        } else {
                            theme.button_bg
                        };
                        let hover_bg = if is_open {
                            theme.accent.gamma_multiply(0.22)
                        } else {
                            theme.button_hover
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
                        let resp = resp.on_hover_text("Pou — Casa / Vestir / Jugar / Progreso");
                        let hovered = resp.hovered();
                        let cur_bg = if hovered { hover_bg } else { bg };
                        if ui.is_rect_visible(rect) {
                            ui.painter().rect_filled(rect, RADIUS_MD, cur_bg);
                            ui.painter().rect_stroke(
                                rect,
                                RADIUS_MD,
                                egui::Stroke::new(
                                    1.0,
                                    if is_open {
                                        theme.accent
                                    } else {
                                        theme.separator
                                    },
                                ),
                            );
                            draw_icon(
                                ui.painter(),
                                rect.shrink(3.0),
                                Icon::Pou,
                                if is_open {
                                    theme.accent
                                } else {
                                    theme.text_secondary
                                },
                            );
                        }
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                "Pou — habitáculo",
                            )
                        });
                        if resp.clicked() {
                            app.show_pou_window = !app.show_pou_window;
                            if app.show_pou_window {
                                app.pou_tab = crate::app::PouTab::Casa;
                            }
                        }
                    }
                    // Configuración unificada — abre "Configuración" (cierra asistente si estaba abierto)
                    {
                        let is_open = app.show_mascot_config || app.assistant.settings_open;
                        let bg = if is_open {
                            theme.accent.gamma_multiply(0.14)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
                        let resp = resp.on_hover_text("Configuración");
                        let hovered = resp.hovered();
                        let cur_bg = if hovered { theme.button_hover } else { bg };
                        if ui.is_rect_visible(rect) {
                            if hovered || is_open {
                                ui.painter().rect_filled(rect, RADIUS_MD, cur_bg);
                                ui.painter().rect_stroke(
                                    rect,
                                    RADIUS_MD,
                                    egui::Stroke::new(
                                        1.0,
                                        if is_open {
                                            theme.accent
                                        } else {
                                            theme.separator
                                        },
                                    ),
                                );
                            }
                            draw_icon(
                                ui.painter(),
                                rect.shrink(3.0),
                                Icon::Settings,
                                if is_open {
                                    theme.accent
                                } else {
                                    theme.text_secondary
                                },
                            );
                        }
                        if resp.clicked() {
                            let will_open = !is_open;
                            app.show_mascot_config = will_open;
                            app.assistant.settings_open = false;
                            app.assistant.config_tab = 0;
                        }
                    }
                    if let Some(response) =
                        draw_assistant_reopen_control(ui, &mut app.assistant_visible, accent)
                    {
                        if response.clicked() {
                            app.open_assistant_workspace();
                        }
                    }
                });

                // Marca Grafito ya está en la barra del sistema; no duplicar en el centro
                let _ = bar_rect;
                let _ = TYPE_BASE;
            });
        });

    // ── LEFT SIDEBAR (56px, labeled tabs) ──
    // 6 tabs armonizados: un icono representativo por panel + etiqueta corta
    // legible. Las perspectivas se cambian únicamente desde la barra superior.
    let tabs: &[(&str, Icon, &str)] = &[
        ("Álgebra", Icon::Function, "Objetos, variables y comandos"),
        (
            "Herram.",
            Icon::Settings,
            "Herramientas de construcción y análisis",
        ),
        ("CAS", Icon::Analyze, "Cálculo simbólico paso a paso"),
        ("Vista", Icon::Eye, "Cuadrícula, ejes y estilo"),
    ];
    if show_sidebar {
        egui::SidePanel::left("icon_bar")
            .exact_width(60.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(side_fill)
                    .stroke(egui::Stroke::new(1.0, sep_col.gamma_multiply(0.6))),
            )
            .show(ctx, |ui| {
                #[cfg(feature = "profile")]
                puffin::profile_scope!("ui_sidebar");
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);

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

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(50.0, 52.0),
                                    egui::Sense::click(),
                                );
                                let resp = resp.on_hover_text(*tip);
                                let progress = ui.ctx().animate_bool(
                                    ui.id().with(("sidebar_tab_state", i)),
                                    active || resp.hovered(),
                                );
                                let target_fill = if active { bg } else { theme.hover_overlay };
                                let target_border = if active {
                                    theme.accent
                                } else {
                                    theme.separator
                                };
                                if ui.is_rect_visible(rect) {
                                    ui.painter().rect(
                                        rect,
                                        RADIUS_MD,
                                        interpolate_color(
                                            Color32::TRANSPARENT,
                                            target_fill,
                                            progress,
                                        ),
                                        egui::Stroke::new(
                                            1.0,
                                            interpolate_color(
                                                Color32::TRANSPARENT,
                                                target_border,
                                                progress,
                                            ),
                                        ),
                                    );
                                    if active {
                                        let marker = egui::Rect::from_center_size(
                                            egui::pos2(rect.min.x + 4.0, rect.center().y),
                                            egui::vec2(2.0, 22.0),
                                        );
                                        ui.painter().rect_filled(marker, 1.0, theme.accent);
                                    }
                                    let icon_rect = egui::Rect::from_center_size(
                                        rect.center() - egui::vec2(0.0, 7.0),
                                        egui::vec2(21.0, 21.0),
                                    );
                                    draw_icon(ui.painter(), icon_rect, *icon, ic_color);
                                    ui.painter().text(
                                        rect.center() + egui::vec2(0.0, 14.0),
                                        Align2::CENTER_CENTER,
                                        *label,
                                        egui::FontId::proportional(10.0),
                                        ic_color,
                                    );
                                }

                                resp.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *tip)
                                });
                                if resp.clicked() {
                                    if active {
                                        app.left_drawer_open = !app.left_drawer_open;
                                    } else {
                                        app.sidebar_tab = i;
                                        app.left_drawer_open = true;
                                    }
                                }
                                ui.add_space(3.0);
                            }

                            ui.add_space(8.0);
                        });
                    });
            });
    }
}

/// Renders the shared Geometry 3D utility contents and returns a close request.
fn draw_geometry_utility_contents(
    app: &mut GrafitoApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
) -> bool {
    let theme = current_theme(ctx);
    let mut close_requested = false;

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Espacio de trabajo")
                .color(theme.text_secondary)
                .size(12.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close_requested = action_icon_button(
                ui,
                Icon::Close,
                theme.text_secondary,
                "Ocultar dock contextual",
            )
            .clicked();
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let inspector_selected = app.workspace_dock_tab == WorkspaceDockTab::Inspector;
        if ui
            .selectable_label(inspector_selected, "Inspector")
            .on_hover_text("Propiedades del objeto seleccionado")
            .clicked()
        {
            app.workspace_dock_tab = WorkspaceDockTab::Inspector;
        }
        let assistant_label = if app.assistant.is_pending {
            "Asistente •"
        } else {
            "Asistente"
        };
        let assistant_selected = app.workspace_dock_tab == WorkspaceDockTab::Assistant;
        if ui
            .selectable_label(assistant_selected, assistant_label)
            .on_hover_text("Asistente matemático")
            .clicked()
        {
            app.open_assistant_workspace();
        }
    });
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    match app.workspace_dock_tab {
        WorkspaceDockTab::Inspector => crate::panels::draw_right_properties_contents(app, ui),
        WorkspaceDockTab::Assistant => app.draw_assistant_contents_in_workspace_dock(ctx, ui),
    }

    close_requested
}

/// Un único dock contextual evita que Inspector y Asistente compitan por el
/// canvas de Geometry 3D. Ambos conservan su estado mientras la otra pestaña
/// está activa.
pub(crate) fn draw_geometry_utility_dock(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    egui::SidePanel::right("geometry_utility_dock")
        .default_width(344.0)
        .min_width(292.0)
        .max_width(440.0)
        .resizable(true)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator)),
        )
        .show(ctx, |ui| {
            if draw_geometry_utility_contents(app, ctx, ui) {
                app.right_drawer_open = false;
            }
        });
}

/// Compact Geometry 3D keeps the same Inspector/Assistant utility on demand.
pub(crate) fn draw_compact_geometry_utility_dock(app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    let maximum_height = (ctx.available_rect().height() * 0.65).max(180.0);
    egui::TopBottomPanel::bottom("geometry_utility_compact_dock")
        .resizable(true)
        .default_height(360.0)
        .min_height(180.0)
        .max_height(maximum_height)
        .frame(
            egui::Frame::none()
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator)),
        )
        .show(ctx, |ui| {
            if draw_geometry_utility_contents(app, ctx, ui) {
                app.compact_geometry_utility_open = false;
            }
        });
}

pub(crate) fn draw_bottom_bar(app: &mut GrafitoApp, ctx: &egui::Context, show_input: bool) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_bottom_bar");

    let theme = current_theme(ctx);
    let accent = theme.accent;
    let sep_col = theme.separator;
    let txt_dim = theme.text_tertiary;
    let _txt_col = theme.text_primary;

    // ── INPUT BAR — hairline 10% (no negro)
    if show_input {
        let mut should_exec = false;
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(40.0)
            .frame(
                egui::Frame::none()
                    .fill(theme.input_bar_bg)
                    .stroke(egui::Stroke::new(1.0, sep_col.gamma_multiply(0.6)))
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("+").color(accent).size(17.0).strong());
                    let response = draw_command_input(
                        ui,
                        app,
                        "bottom_bar",
                        [command_input_width(ui.available_width(), 40.0), 26.0],
                        "Entrada... (ej: sin(x), A=(1,2), Derivative[x^2,x])",
                        true,
                    );
                    if response.submitted && !app.input_text.is_empty() {
                        should_exec = true;
                    }

                    if action_icon_button(ui, Icon::Play, accent, "Ejecutar entrada").clicked() {
                        should_exec = true;
                    }
                });
            });
        if should_exec && !app.input_text.is_empty() {
            let time = ctx.input(|i| i.time);
            app.submit_input_text(time);
        }
    }

    // ── STATUS BAR — hairline
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(24.0)
        .frame(
            egui::Frame::none()
                .fill(theme.status_bar_bg)
                .stroke(egui::Stroke::new(1.0, sep_col.gamma_multiply(0.6)))
                .inner_margin(egui::Margin::symmetric(10.0, 1.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let coord_text = if let Some(pos) = app.last_mouse_pos {
                    let view = app.document.view();
                    let local = app
                        .canvas_origin
                        .map(|origin| canvas_local_pointer(pos, origin))
                        .unwrap_or(pos);
                    let world = view.screen_to_world(glam::Vec2::new(local.x, local.y));
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
                        ViewMode::D3 => status_hint_for_3d_tool(app.current_tool),
                    }
                };
                if !hint.is_empty() {
                    ui.add(
                        egui::Label::new(egui::RichText::new(hint).size(11.0).color(txt_dim))
                            .truncate(),
                    );
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

pub(crate) fn draw_command_input(
    ui: &mut egui::Ui,
    app: &mut GrafitoApp,
    id_salt: &'static str,
    size: [f32; 2],
    hint: &str,
    frame: bool,
) -> CommandInputResponse {
    let theme = current_theme(ui.ctx());
    let response = ui.add_sized(
        [size[0].max(0.0), size[1].max(0.0)],
        egui::TextEdit::singleline(&mut app.input_text)
            .id_salt(id_salt)
            .hint_text(hint)
            .frame(frame)
            .text_color(theme.text_primary),
    );
    if app.command_input_focus_requested {
        response.request_focus();
        app.command_input_focus_requested = false;
    }

    let changed = response.changed();
    let suggestions = if !app.input_text.is_empty() {
        compute_autocomplete_suggestions(&app.input_text, &app.document)
    } else {
        Vec::new()
    };

    let mut completed = false;
    // Tab is reserved for completion; Enter keeps the usual command-submission behavior.
    let completion_key = (!suggestions.is_empty()
        && (response.has_focus() || response.lost_focus()))
    .then(|| {
        ui.input_mut(|input| {
            [egui::Key::Tab].into_iter().find(|key| {
                is_autocomplete_completion_key(*key)
                    && input.consume_key(egui::Modifiers::NONE, *key)
            })
        })
    })
    .flatten();
    let mut show_popup =
        !suggestions.is_empty() && (response.has_focus() || completion_key.is_some());
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
            show_popup = false;
            app.autocomplete.open = false;
            app.autocomplete.selected = 0;
        }
        if show_popup && completion_key.is_some() {
            completed = complete_autocomplete_selection(
                &mut app.input_text,
                &suggestions,
                &mut app.autocomplete,
            );
            if completed {
                show_popup = false;
                response.request_focus();
            }
        }
    }
    app.autocomplete.open = show_popup;

    if show_popup && draw_autocomplete_popup(ui, app, id_salt, response.rect, &suggestions) {
        response.request_focus();
    }

    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    CommandInputResponse {
        submitted: !completed && enter && (response.has_focus() || response.lost_focus()),
        changed,
    }
}

pub(crate) fn command_input_width(available_width: f32, reserved_width: f32) -> f32 {
    (available_width - reserved_width).max(0.0)
}

pub(crate) fn canvas_local_pointer(pointer: egui::Pos2, canvas_origin: egui::Pos2) -> egui::Pos2 {
    pointer - canvas_origin.to_vec2()
}

pub(crate) fn draw_unsaved_changes_dialog(app: &mut GrafitoApp, ctx: &egui::Context) {
    let Some(action) = app.pending_document_action() else {
        return;
    };

    let save_error = app.document_save_error().map(str::to_owned);
    let mut open = true;
    let mut decision = None;
    egui::Window::new("Cambios sin guardar")
        .id(egui::Id::new("unsaved_changes_dialog"))
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(ui.ctx().screen_rect().height() * 0.6)
                .show(ui, |ui| {
                    ui.set_min_width(380.0);
                    egui::Frame::none().show(ui, |ui| {
                        dialog_contents(ui, action, save_error.as_ref(), &mut decision);
                    });
                });
        });
    if !open && decision.is_none() {
        decision = Some(UnsavedDecision::Cancel);
    }
    if let Some(decision) = decision {
        app.queue_unsaved_decision(decision);
    }
}

fn dialog_contents(
    ui: &mut egui::Ui,
    action: DocumentAction,
    save_error: Option<&String>,
    decision: &mut Option<UnsavedDecision>,
) {
    ui.add(egui::Label::new(
        egui::RichText::new("Guardar cambios antes de continuar?")
            .size(16.0)
            .strong(),
    ));
    ui.add_space(6.0);
    ui.add(egui::Label::new(egui::RichText::new(action.prompt_message()).size(13.0)).wrap());
    if let Some(error) = save_error {
        ui.add_space(6.0);
        ui.colored_label(
            current_theme(ui.ctx()).danger,
            format!("No se pudo guardar: {error}"),
        );
    }
    ui.add_space(14.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 30.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if ui.button("Guardar").clicked() {
                *decision = Some(UnsavedDecision::Save);
            }
            if ui.button("Descartar").clicked() {
                *decision = Some(UnsavedDecision::Discard);
            }
            if ui.button("Cancelar").clicked() {
                *decision = Some(UnsavedDecision::Cancel);
            }
        },
    );
}

fn draw_autocomplete_popup(
    ui: &mut egui::Ui,
    app: &mut GrafitoApp,
    id_salt: &'static str,
    input_rect: egui::Rect,
    suggestions: &[AutocompleteItem],
) -> bool {
    let popup_pos = egui::pos2(input_rect.min.x, input_rect.max.y);
    let selected = app.autocomplete.selected;
    let display: Vec<(String, String)> = suggestions
        .iter()
        .take(8)
        .map(|it| (it.text.clone(), it.detail.clone()))
        .collect();
    let popup_id = ui.id().with(id_salt).with("autocomplete_popup");
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
                        format!("{}  - {}", text, detail),
                    ));
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    if let Some(i) = clicked {
        app.autocomplete.selected = i;
        return complete_autocomplete_selection(
            &mut app.input_text,
            suggestions,
            &mut app.autocomplete,
        );
    }
    false
}

pub(crate) fn status_hint_for_3d_tool(tool: Tool) -> String {
    match tool {
        Tool::Tesseract4D => {
            "Teseracto 4D: clic para crear un objeto centrado y proyectado".to_string()
        }
        Tool::Hypercube5D => {
            "Hipercubo 5D: clic para crear un objeto centrado y proyectado".to_string()
        }
        _ => "3D: clic izq pan (Select), der orbitar, rueda zoom".to_string(),
    }
}

fn status_hint_for_tool(tool: Tool) -> String {
    match tool {
        Tool::Select => "Seleccionar: clic objeto, arrastrar vacio para mover vista".to_string(),
        Tool::Point => "Punto: clic para crear".to_string(),
        Tool::Point3D => "Punto 3D: clic para crear".to_string(),
        Tool::Line => "Recta: clic en dos puntos".to_string(),
        Tool::Segment => "Segmento: clic en dos puntos".to_string(),
        Tool::Ray => "Semirrecta: clic origen, clic direccion".to_string(),
        Tool::Vector => "⇒ Vector: clic origen, clic extremo".to_string(),
        Tool::Circle => "Circulo: clic centro, clic borde".to_string(),
        Tool::Polygon => "Poligono: clic vertices, clic der para cerrar".to_string(),
        Tool::RegularPolygon => "Poligono regular: clic centro, clic vertice".to_string(),
        Tool::Function => "f(x) Función: clic para crear y editar".to_string(),
        Tool::Distance => "Distancia: clic en dos puntos".to_string(),
        Tool::DistanceConstraint => "Restriccion Distancia: clic en dos puntos".to_string(),
        Tool::Angle => "Angulo: clic en 3 puntos (vertice, brazo 1, brazo 2)".to_string(),
        Tool::AngleConstraint => "Restriccion Angulo: clic en dos rectas".to_string(),
        Tool::Area => "Area: clic en poligono o circulo".to_string(),
        Tool::Slope => "m Pendiente: clic en recta".to_string(),
        Tool::Slider => "═ Deslizador: clic para crear variable".to_string(),
        Tool::Locus => "Locus: clic punto driver, clic punto objetivo".to_string(),
        Tool::Button | Tool::Image => "Herramienta no disponible en esta versión".to_string(),
        Tool::Midpoint => "M Punto medio: clic en dos puntos".to_string(),
        Tool::Perpendicular => "⟂ Perpendicular: clic en dos puntos".to_string(),
        Tool::Tangent => "Tangente: selecciona circulo y recta".to_string(),
        Tool::Root => "x₀ Raíces: clic en una función".to_string(),
        Tool::Extremum => "max Extremos: clic en una función".to_string(),
        Tool::Intersect => "Interseccion: clic en dos objetos".to_string(),
        Tool::Coincident => "Coincidente: selecciona dos puntos".to_string(),
        Tool::Horizontal => "Horizontal: selecciona una recta".to_string(),
        Tool::Vertical => "Vertical: selecciona una recta".to_string(),
        Tool::EqualLength => "= Longitud igual: selecciona dos segmentos".to_string(),
        Tool::Symmetry => "Simetria: punto, imagen, eje".to_string(),
        Tool::EllipseByFoci => "Elipse: dos focos y un punto".to_string(),
        Tool::ParabolaByFocusDirectrix => "⩗ Parábola: foco y directriz".to_string(),
        Tool::HyperbolaByFoci => "⩘ Hipérbola: dos focos y un punto".to_string(),
        Tool::ConicByFivePoints => "C5 Cónica: cinco puntos".to_string(),
        Tool::PolygonUnion => "∪ Unión: dos polígonos".to_string(),
        Tool::PolygonIntersection => "∩ Intersección: dos polígonos".to_string(),
        Tool::PolygonDifference => "\\ Diferencia: dos polígonos".to_string(),
        Tool::PolygonXor => "XOR: dos poligonos".to_string(),
        Tool::Sphere3D => "Esfera 3D: clic centro y borde".to_string(),
        Tool::Cube3D => "Cubo 3D: clic centro y borde".to_string(),
        Tool::Tesseract4D => {
            "Teseracto 4D: clic para crear un objeto centrado y proyectado".to_string()
        }
        Tool::Hypercube5D => {
            "Hipercubo 5D: clic para crear un objeto centrado y proyectado".to_string()
        }
        _ => "Espacio / clic medio: mover vista".to_string(),
    }
}

pub(crate) fn apply_color_picker_object_color_change(
    document: &mut grafito_core::Document,
    object_id: grafito_core::ObjectId,
    color: grafito_geometry::Color,
    undo_stack: &mut Vec<grafito_core::Document>,
    redo_stack: &mut Vec<grafito_core::ChangeSet>,
) -> Result<bool, String> {
    let Some(object) = document.get_object(object_id) else {
        return Ok(false);
    };
    if grafito_ui::color_picker::colors_match(object.color(), color) {
        return Ok(false);
    }
    let mut candidate = object.clone();
    candidate.set_color(color);
    let Some(before) = document.try_replace_object_with_previous(object_id, candidate)? else {
        return Ok(false);
    };

    let mut snapshot = DeferredPanelSnapshot::new(undo_stack.len());
    snapshot.capture_successful_replacement(before);
    Ok(snapshot.save_if_semantically_changed(document, undo_stack, redo_stack))
}

pub(crate) fn apply_color_picker_regular_polychoron_fill_color_change(
    document: &mut grafito_core::Document,
    object_id: grafito_core::ObjectId,
    color: grafito_geometry::Color,
    undo_stack: &mut Vec<grafito_core::Document>,
    redo_stack: &mut Vec<grafito_core::ChangeSet>,
) -> Result<bool, String> {
    let Some(grafito_core::GeoObject::RegularPolychoron4D(polychoron)) =
        document.get_object(object_id)
    else {
        return Ok(false);
    };
    let Some(fill_color) = polychoron.fill_color else {
        return Ok(false);
    };
    if grafito_ui::color_picker::colors_match(fill_color, color) {
        return Ok(false);
    }

    let mut candidate = grafito_core::GeoObject::RegularPolychoron4D(polychoron.clone());
    let grafito_core::GeoObject::RegularPolychoron4D(candidate_polychoron) = &mut candidate else {
        return Ok(false);
    };
    candidate_polychoron.fill_color = Some(color);
    let Some(before) = document.try_replace_object_with_previous(object_id, candidate)? else {
        return Ok(false);
    };

    let mut snapshot = DeferredPanelSnapshot::new(undo_stack.len());
    snapshot.capture_successful_replacement(before);
    Ok(snapshot.save_if_semantically_changed(document, undo_stack, redo_stack))
}

const COLOR_PICKER_DIALOG_SIZE: egui::Vec2 = egui::vec2(390.0, 350.0);
const COLOR_PICKER_SAFE_MARGIN: f32 = 16.0;

pub(crate) fn color_picker_safe_viewport(viewport: egui::Rect) -> egui::Rect {
    let horizontal_inset = COLOR_PICKER_SAFE_MARGIN.min((viewport.width() - 1.0).max(0.0) * 0.5);
    let vertical_inset = COLOR_PICKER_SAFE_MARGIN.min((viewport.height() - 1.0).max(0.0) * 0.5);
    egui::Rect::from_min_max(
        viewport.min + egui::vec2(horizontal_inset, vertical_inset),
        viewport.max - egui::vec2(horizontal_inset, vertical_inset),
    )
}

/// Acción terminal del diálogo; los cambios del picker siguen siendo transitorios
/// hasta que se recibe [`Self::Apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorPickerDialogAction {
    Apply,
    Cancel,
    Dismiss,
}

pub(crate) fn apply_color_picker_dialog_action(
    action: ColorPickerDialogAction,
    document: &mut grafito_core::Document,
    target: ColorPickerTarget,
    object_id: grafito_core::ObjectId,
    color: grafito_geometry::Color,
    undo_stack: &mut Vec<grafito_core::Document>,
    redo_stack: &mut Vec<grafito_core::ChangeSet>,
) -> Result<bool, String> {
    match action {
        ColorPickerDialogAction::Apply => match target {
            ColorPickerTarget::ObjectColor => apply_color_picker_object_color_change(
                document, object_id, color, undo_stack, redo_stack,
            ),
            ColorPickerTarget::RegularPolychoronFill => {
                apply_color_picker_regular_polychoron_fill_color_change(
                    document, object_id, color, undo_stack, redo_stack,
                )
            }
        },
        ColorPickerDialogAction::Cancel | ColorPickerDialogAction::Dismiss => Ok(false),
    }
}

pub(crate) fn draw_color_picker(app: &mut GrafitoApp, ctx: &egui::Context) {
    #[cfg(feature = "profile")]
    puffin::profile_scope!("ui_color_picker");
    let Some(ActiveColorPicker {
        object_id,
        target,
        mut picker,
    }) = app.active_color_picker.clone()
    else {
        return;
    };

    let mut keep_open = true;
    let mut outcome = grafito_ui::color_picker::ColorPickerOutcome::default();
    let mut dialog_action = None;
    let theme = current_theme(ctx);

    egui::Window::new("Selector de Color")
        .collapsible(false)
        .resizable(false)
        .fixed_size(COLOR_PICKER_DIALOG_SIZE)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .constrain_to(color_picker_safe_viewport(ctx.screen_rect()))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator))
                .rounding(RADIUS_MD),
        )
        .open(&mut keep_open)
        .show(ctx, |ui| {
            outcome = picker.show(ui, &mut app.color_favorites);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    dialog_action = Some(ColorPickerDialogAction::Cancel);
                }
                if ui.button("Aplicar").clicked() {
                    dialog_action = Some(ColorPickerDialogAction::Apply);
                }
            });
        });

    if outcome.any_changed() {
        ctx.request_repaint();
    }

    let dialog_action =
        dialog_action.or_else(|| (!keep_open).then_some(ColorPickerDialogAction::Dismiss));
    if let Some(action) = dialog_action {
        let _ = apply_color_picker_dialog_action(
            action,
            &mut app.document,
            target,
            object_id,
            picker.to_color(),
            &mut app.undo_stack,
            &mut app.redo_stack,
        );
        app.active_color_picker = None;
    } else {
        app.active_color_picker = Some(ActiveColorPicker {
            object_id,
            target,
            picker,
        });
    }
}

const MAX_AUTOCOMPLETE_TOKEN_CHARS: usize = 256;
const MAX_AUTOCOMPLETE_SUGGESTIONS: usize = 8;
const MAX_DOCUMENT_AUTOCOMPLETE_CANDIDATES: usize = 64;
const MIN_AUTOCOMPLETE_SCORE: f64 = 0.35;
const AUTOCOMPLETE_SEPARATORS: &[char] = &['[', '(', ',', ' ', '\t', '=', '+', '-', '*', '/'];

fn autocomplete_token(input: &str) -> Option<&str> {
    let mut token_chars = 0;
    for (index, character) in input.char_indices().rev() {
        if AUTOCOMPLETE_SEPARATORS.contains(&character) {
            return Some(input[index + character.len_utf8()..].trim());
        }
        token_chars += 1;
        if token_chars > MAX_AUTOCOMPLETE_TOKEN_CHARS {
            return None;
        }
    }
    Some(input.trim())
}

fn autocomplete_text_is_bounded(text: &str) -> bool {
    text.char_indices()
        .nth(MAX_AUTOCOMPLETE_TOKEN_CHARS)
        .is_none()
}

/// Calcula el puntaje de similitud entre el query escrito y un candidato
/// utilizando coincidencia exacta, prefijo, subsecuencia y distancia de Levenshtein (tolerancia a erratas).
pub(crate) fn similarity_score(query: &str, candidate: &str) -> f64 {
    if !autocomplete_text_is_bounded(query) || !autocomplete_text_is_bounded(candidate) {
        return 0.0;
    }

    let q = query.to_lowercase();
    let c = candidate.to_lowercase();

    if q.is_empty() {
        return 0.0;
    }

    if c == q {
        return 2.0; // Coincidencia perfecta
    }

    if c.starts_with(&q) {
        return 1.5 - (c.len() - q.len()) as f64 * 0.02; // Comienza con el query (más corto = mayor score)
    }

    if c.contains(&q) {
        return 1.2 - (c.find(&q).unwrap() as f64 * 0.05); // Contiene el query
    }

    // Distancia de Levenshtein para tolerancia a erratas
    let q_chars: Vec<char> = q.chars().collect();
    let c_chars: Vec<char> = c.chars().collect();

    let mut previous: Vec<usize> = (0..=c_chars.len()).collect();
    let mut current = vec![0; c_chars.len() + 1];

    for i in 1..=q_chars.len() {
        current[0] = i;
        for j in 1..=c_chars.len() {
            if q_chars[i - 1] == c_chars[j - 1] {
                current[j] = previous[j - 1];
            } else {
                current[j] = 1 + previous[j - 1].min(previous[j].min(current[j - 1]));
            }
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[c_chars.len()];
    let max_len = q_chars.len().max(c_chars.len()) as f64;
    1.0 - (distance as f64 / max_len)
}

const MATH_FUNCTIONS: &[(&str, &str)] = &[
    ("deriv_z", "derivada complejos df/dz"),
    ("deriv_z_conj", "derivada complejos df/d(conj z)"),
    ("sin", "seno complejo/real"),
    ("cos", "coseno complejo/real"),
    ("tan", "tangente complejo/real"),
    ("sinh", "seno hiperbólico"),
    ("cosh", "coseno hiperbólico"),
    ("tanh", "tangente hiperbólica"),
    ("exp", "exponencial e^z"),
    ("ln", "logaritmo natural"),
    ("sqrt", "raíz cuadrada"),
    ("abs", "módulo / valor absoluto"),
    ("conj", "conjugado complejo"),
    ("re", "parte real de z"),
    ("im", "parte imaginaria de z"),
    ("arg", "argumento principal de z"),
    ("gamma", "función Gamma"),
    ("zeta", "función Zeta de Riemann"),
    ("bessel_j", "bessel J_0(z)"),
    ("bessel_y", "bessel Y_0(z)"),
    ("lambert_w", "función W de Lambert"),
    ("erf", "función de error"),
];

/// Returns a bounded insertion slot for a matching candidate, if it belongs
/// among the suggestions currently retained.
pub(crate) fn autocomplete_candidate_slot(
    candidates: &[(AutocompleteItem, f64)],
    text: &str,
    score: f64,
) -> Option<usize> {
    if score < MIN_AUTOCOMPLETE_SCORE {
        return None;
    }
    if candidates.len() < MAX_AUTOCOMPLETE_SUGGESTIONS {
        return Some(candidates.len());
    }

    let (worst_index, worst) = candidates
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.0.text.len().cmp(&left.0.text.len()))
        })?;
    let score_order = score
        .partial_cmp(&worst.1)
        .unwrap_or(std::cmp::Ordering::Equal);
    if score_order.is_gt() || (score_order.is_eq() && text.len() < worst.0.text.len()) {
        Some(worst_index)
    } else {
        None
    }
}

fn add_autocomplete_candidate(
    candidates: &mut Vec<(AutocompleteItem, f64)>,
    current_token: &str,
    text: &str,
    detail: &str,
    bracket: bool,
) {
    let score = similarity_score(current_token, text);
    let Some(slot) = autocomplete_candidate_slot(candidates, text, score) else {
        return;
    };

    let item = AutocompleteItem {
        text: text.to_string(),
        detail: detail.to_string(),
        bracket,
    };
    if slot == candidates.len() {
        candidates.push((item, score));
    } else {
        candidates[slot] = (item, score);
    }
}

/// Calcula hasta 8 sugerencias de autocompletado para el último token del texto de entrada,
/// combinando comandos, objetos, variables y funciones matemáticas.
pub(crate) fn compute_autocomplete_suggestions(
    input: &str,
    document: &grafito_core::Document,
) -> Vec<AutocompleteItem> {
    let mut scored_items: Vec<(AutocompleteItem, f64)> = Vec::new();

    let Some(current_token) = autocomplete_token(input) else {
        return Vec::new();
    };
    if current_token.is_empty() {
        return Vec::new();
    }

    // 1. Agregar comandos de la paleta
    for cmd in grafito_ui::command_palette::all_commands() {
        if !cmd.syntax_hint.contains('[') {
            continue;
        }
        add_autocomplete_candidate(
            &mut scored_items,
            current_token,
            cmd.name,
            cmd.category,
            true,
        );
    }

    // 2. Agregar funciones matemáticas
    for (name, desc) in MATH_FUNCTIONS {
        add_autocomplete_candidate(&mut scored_items, current_token, name, desc, false);
    }

    // 3. Agregar objetos del documento
    // Bound fuzzy-scoring allocations before large documents can dominate an input frame.
    for (_, obj) in document
        .objects_iter()
        .take(MAX_DOCUMENT_AUTOCOMPLETE_CANDIDATES)
    {
        let label = obj.label();
        if !label.is_empty() {
            add_autocomplete_candidate(&mut scored_items, current_token, label, obj.name(), false);
        }
    }

    // 4. Agregar variables del documento
    for k in document
        .variables()
        .keys()
        .take(MAX_DOCUMENT_AUTOCOMPLETE_CANDIDATES)
    {
        add_autocomplete_candidate(&mut scored_items, current_token, k, "variable", false);
    }

    // Ordenar por puntaje descendente (mejor coincidencia) y tie-breaker por longitud ascendente
    scored_items.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.text.len().cmp(&b.0.text.len()))
    });

    scored_items.into_iter().map(|(item, _)| item).collect()
}

/// Reemplaza el token actual (el fragmento que se está escribiendo tras el
/// último separador) por el item seleccionado. Para comandos bracket, añade
/// `[` al final para que el usuario complete los argumentos.
pub(crate) fn apply_autocomplete_item(input: &mut String, item: &AutocompleteItem) {
    let token_start = input
        .rfind(|c: char| AUTOCOMPLETE_SEPARATORS.contains(&c))
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &input[..token_start];

    if item.bracket {
        let command = match item.text.as_str() {
            "Thomas (Butterfly)" => "Thomas",
            other => other,
        };
        *input = format!("{}{}[", prefix, command);
    } else if MATH_FUNCTIONS.iter().any(|(name, _)| name == &item.text) {
        *input = format!("{}{}(", prefix, item.text);
    } else {
        *input = format!("{}{}", prefix, item.text);
    }
}

pub(crate) fn is_autocomplete_completion_key(key: egui::Key) -> bool {
    matches!(key, egui::Key::Tab)
}

pub(crate) fn complete_autocomplete_selection(
    input: &mut String,
    suggestions: &[AutocompleteItem],
    autocomplete: &mut crate::app::InputAutocomplete,
) -> bool {
    let Some(item) = suggestions.get(autocomplete.selected) else {
        return false;
    };

    apply_autocomplete_item(input, item);
    autocomplete.open = false;
    autocomplete.selected = 0;
    true
}
