//! Paleta de comandos de Grafito - busqueda rapida de comandos con Ctrl+K.

use grafito_command::command_registry;

pub use command_registry::CommandSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCommand {
    pub name: &'static str,
    pub category: &'static str,
    pub syntax_hint: &'static str,
    pub help: &'static str,
    insertion: Option<&'static str>,
    command_id: Option<&'static str>,
}

impl PaletteCommand {
    fn registered(spec: &'static CommandSpec) -> Self {
        Self {
            name: spec.palette_label,
            category: spec.category,
            syntax_hint: spec.signatures[0].syntax,
            help: spec.help,
            insertion: Some(spec.insertion),
            command_id: Some(spec.id),
        }
    }

    pub fn input_template(&self) -> Option<String> {
        self.insertion.map(str::to_owned)
    }

    pub fn is_registered(&self) -> bool {
        self.command_id.is_some()
    }

    pub fn registered_spec(&self) -> Option<&'static CommandSpec> {
        self.command_id.and_then(command_registry::by_id)
    }
}

const UI_ACTIONS: &[PaletteCommand] = &[
    PaletteCommand {
        name: "Point Tool",
        category: "Herramientas",
        syntax_hint: "Clic en el lienzo | (x, y)",
        help: "Activa la herramienta para crear puntos.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Line Tool",
        category: "Herramientas",
        syntax_hint: "Clic dos puntos | A = (x1, y1), B = (x2, y2)",
        help: "Activa la herramienta para crear rectas.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Circle Tool",
        category: "Herramientas",
        syntax_hint: "Clic centro + borde",
        help: "Activa la herramienta para crear circunferencias.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Polygon Tool",
        category: "Herramientas",
        syntax_hint: "Clic vertices",
        help: "Activa la herramienta para crear poligonos.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Function Tool",
        category: "Herramientas",
        syntax_hint: "f(x) = expr",
        help: "Activa la herramienta para crear funciones.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Pencil",
        category: "Herramientas",
        syntax_hint: "Clic sostenido y arrastrar para dibujar a mano alzada",
        help: "Activa el lapiz de dibujo libre.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Eraser",
        category: "Herramientas",
        syntax_hint: "Clic o arrastrar para borrar objetos",
        help: "Activa el borrador.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Save",
        category: "Archivo",
        syntax_hint: "Guardar documento actual",
        help: "Abre el dialogo para guardar el documento.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Export SVG",
        category: "Archivo",
        syntax_hint: "Exportar graficos vectoriales",
        help: "Exporta el documento como SVG.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Export PNG",
        category: "Archivo",
        syntax_hint: "Exportar imagen raster",
        help: "Exporta el documento como PNG.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Export TikZ",
        category: "Archivo",
        syntax_hint: "Exportar codigo LaTeX TikZ",
        help: "Exporta el documento como TikZ.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Zoom to Fit",
        category: "Vista",
        syntax_hint: "Ajustar todos los objetos a la vista",
        help: "Ajusta el encuadre al contenido del documento.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Toggle Grid",
        category: "Vista",
        syntax_hint: "Mostrar u ocultar cuadricula",
        help: "Alterna la cuadricula del lienzo.",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Toggle Dark Mode",
        category: "Vista",
        syntax_hint: "Cambiar tema claro u oscuro",
        help: "Alterna el tema de la aplicacion.",
        insertion: None,
        command_id: None,
    },
];

pub fn all_commands() -> Vec<PaletteCommand> {
    let mut commands = UI_ACTIONS.to_vec();
    commands.extend(command_registry::palette_commands().map(PaletteCommand::registered));
    commands
}

#[derive(Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub search: String,
    pub selected_index: usize,
}

/// Ancho de la paleta dejando un margen seguro para ventanas estrechas.
pub fn palette_window_width(viewport_width: f32) -> f32 {
    (viewport_width - 16.0).clamp(1.0, 640.0)
}

impl CommandPaletteState {
    pub fn filtered_commands(&self) -> Vec<PaletteCommand> {
        let all = all_commands();
        let search_lower = self.search.to_lowercase();

        all.into_iter()
            .filter(|cmd| {
                search_lower.is_empty()
                    || cmd.name.to_lowercase().contains(&search_lower)
                    || cmd.category.to_lowercase().contains(&search_lower)
                    || cmd.help.to_lowercase().contains(&search_lower)
            })
            .collect()
    }

    pub fn clamp_selected_index(&mut self) {
        let len = self.filtered_commands().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.open {
            return None;
        }

        let mut selected_command = None;
        let screen_rect = ctx.screen_rect();
        let mut open = self.open;
        let mut dismissed = false;
        egui::Window::new("Paleta de Comandos")
            .collapsible(false)
            .resizable(false)
            .default_pos([8.0, 48.0])
            .default_width(palette_window_width(screen_rect.width()))
            .max_width(palette_window_width(screen_rect.width()))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (search_rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    if ui.is_rect_visible(search_rect) {
                        let theme = crate::theme::current_theme(ui.ctx());
                        crate::icons::draw_icon(
                            ui.painter(),
                            search_rect,
                            crate::icons::Icon::Search,
                            theme.text_secondary,
                        );
                    }
                    let response = ui.text_edit_singleline(&mut self.search);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let filtered = self.filtered_commands();
                        if let Some(cmd) = filtered.get(self.selected_index) {
                            selected_command = Some(cmd.name.to_string());
                        }
                    }
                    response.request_focus();
                });

                ui.separator();

                let filtered = self.filtered_commands();
                self.clamp_selected_index();
                if filtered.is_empty() {
                    ui.label("No se encontraron comandos");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height((screen_rect.height() - 170.0).max(120.0))
                        .show(ui, |ui| {
                            for (i, cmd) in filtered.iter().enumerate() {
                                let is_selected = i == self.selected_index;
                                let response = ui.selectable_label(
                                    is_selected,
                                    format!("{} - {}", cmd.name, cmd.category),
                                );

                                if response.clicked() {
                                    selected_command = Some(cmd.name.to_string());
                                }

                                if response.hovered() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}\n{}",
                                            cmd.syntax_hint, cmd.help
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                }
                            }
                        });
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    let filtered = self.filtered_commands();
                    if self.selected_index < filtered.len().saturating_sub(1) {
                        self.selected_index += 1;
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    dismissed = true;
                }
            });

        if selected_command.is_some() || dismissed {
            open = false;
        }
        if selected_command.is_some() {
            self.search.clear();
            self.selected_index = 0;
        }

        self.open = open;
        selected_command
    }
}
