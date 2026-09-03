//! Paleta de comandos de Grafito — búsqueda rápida bilingüe con Ctrl+K.
//!
//! Las acciones de UI muestran su etiqueta en español rioplatense (`name`)
//! pero despachan con una clave estable en inglés (`selection_key`) para no
//! romper `GrafitoApp::apply_palette_command` (grafito-app/src/app.rs),
//! que matchea `"Point Tool"`, `"Save"`, etc. La búsqueda es bilingüe:
//! `keywords` guarda alias en inglés + español y el filtro también revisa
//! `syntax_hint` con [`fuzzy_match`] (subsecuencia en orden, sin tildes).

use grafito_command::command_registry;

pub use command_registry::CommandSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCommand {
    /// Etiqueta visible (español rioplatense para las acciones de UI).
    pub name: &'static str,
    pub category: &'static str,
    pub syntax_hint: &'static str,
    pub help: &'static str,
    /// Clave estable de despacho: lo que [`CommandPaletteState::show`]
    /// devuelve al elegir el comando. Para acciones UI es el nombre
    /// histórico en inglés; para comandos del registry es `palette_label`.
    pub selection_key: &'static str,
    /// Alias bilingües (inglés + español) sólo para la búsqueda.
    pub keywords: &'static str,
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
            selection_key: spec.palette_label,
            keywords: spec.id,
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
        name: "Herramienta Punto",
        category: "Herramientas",
        syntax_hint: "Clic en el lienzo | (x, y)",
        help: "Activa la herramienta para crear puntos.",
        selection_key: "Point Tool",
        keywords: "point tool punto",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Herramienta Recta",
        category: "Herramientas",
        syntax_hint: "Clic en dos puntos | A = (x1, y1), B = (x2, y2)",
        help: "Activa la herramienta para crear rectas.",
        selection_key: "Line Tool",
        keywords: "line tool recta linea",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Herramienta Circunferencia",
        category: "Herramientas",
        syntax_hint: "Clic en el centro y en el borde",
        help: "Activa la herramienta para crear circunferencias.",
        selection_key: "Circle Tool",
        keywords: "circle tool circunferencia circulo",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Herramienta Polígono",
        category: "Herramientas",
        syntax_hint: "Clic en los vértices",
        help: "Activa la herramienta para crear polígonos.",
        selection_key: "Polygon Tool",
        keywords: "polygon tool poligono",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Herramienta Función",
        category: "Herramientas",
        syntax_hint: "f(x) = expresión",
        help: "Activa la herramienta para crear funciones.",
        selection_key: "Function Tool",
        keywords: "function tool funcion",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Lápiz",
        category: "Herramientas",
        syntax_hint: "Mantené el clic y arrastrá para dibujar a mano alzada",
        help: "Activa el lápiz de dibujo libre.",
        selection_key: "Pencil",
        keywords: "pencil lapiz dibujo mano alzada",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Borrador",
        category: "Herramientas",
        syntax_hint: "Clic o arrastrá para borrar objetos",
        help: "Activa el borrador.",
        selection_key: "Eraser",
        keywords: "eraser borrador goma borrar",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Guardar",
        category: "Archivo",
        syntax_hint: "Guarda el documento actual",
        help: "Abre el diálogo para guardar el documento.",
        selection_key: "Save",
        keywords: "save guardar documento",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Exportar SVG",
        category: "Archivo",
        syntax_hint: "Exporta gráficos vectoriales",
        help: "Exporta el documento como SVG.",
        selection_key: "Export SVG",
        keywords: "export svg exportar vectorial",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Exportar PNG",
        category: "Archivo",
        syntax_hint: "Exporta una imagen raster",
        help: "Exporta el documento como PNG.",
        selection_key: "Export PNG",
        keywords: "export png exportar imagen raster",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Exportar TikZ",
        category: "Archivo",
        syntax_hint: "Exporta código LaTeX TikZ",
        help: "Exporta el documento como TikZ.",
        selection_key: "Export TikZ",
        keywords: "export tikz exportar latex",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Encuadrar todo",
        category: "Vista",
        syntax_hint: "Ajusta todos los objetos a la vista",
        help: "Ajusta el encuadre al contenido del documento.",
        selection_key: "Zoom to Fit",
        keywords: "zoom fit encuadrar ajustar vista",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Alternar cuadrícula",
        category: "Vista",
        syntax_hint: "Muestra u oculta la cuadrícula",
        help: "Alterna la cuadrícula del lienzo.",
        selection_key: "Toggle Grid",
        keywords: "toggle grid cuadricula grilla",
        insertion: None,
        command_id: None,
    },
    PaletteCommand {
        name: "Alternar modo oscuro",
        category: "Vista",
        syntax_hint: "Cambia entre tema claro y oscuro",
        help: "Alterna el tema de la aplicación.",
        selection_key: "Toggle Dark Mode",
        keywords: "toggle dark mode tema oscuro claro noche",
        insertion: None,
        command_id: None,
    },
];

/// Dobla tildes y diéresis del español a su vocal base para que la búsqueda
/// sea insensible a acentos ("lapiz" encuentra "Lápiz"). Asume minúsculas.
fn fold_spanish(lower: &str) -> String {
    lower
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Búsqueda difusa "contiene en orden": el query coincide si aparece como
/// subcadena (vía rápida) o como subsecuencia en orden dentro del objetivo.
///
/// Insensible a mayúsculas y a tildes; ignora espacios en el pase difuso
/// para que "darkmode" encuentre "dark mode" y viceversa.
pub fn fuzzy_match(query: &str, target: &str) -> bool {
    let query = fold_spanish(&query.to_lowercase());
    let target = fold_spanish(&target.to_lowercase());
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if target.contains(query) {
        return true;
    }
    let compact_target: Vec<char> = target.chars().filter(|c| !c.is_whitespace()).collect();
    let mut pos = 0;
    for qc in query.chars().filter(|c| !c.is_whitespace()) {
        let mut found = false;
        while pos < compact_target.len() {
            let tc = compact_target[pos];
            pos += 1;
            if tc == qc {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

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
    /// Filtra bilingüe (español + inglés) sobre nombre, categoría,
    /// `syntax_hint`, ayuda y alias. Cada palabra del query debe coincidir
    /// (subcadena o difusa en orden) en al menos un campo.
    pub fn filtered_commands(&self) -> Vec<PaletteCommand> {
        let all = all_commands();
        let query = self.search.trim().to_lowercase();
        if query.is_empty() {
            return all;
        }
        all.into_iter()
            .filter(|cmd| {
                query.split_whitespace().all(|token| {
                    [
                        cmd.name,
                        cmd.category,
                        cmd.syntax_hint,
                        cmd.help,
                        cmd.keywords,
                    ]
                    .iter()
                    .any(|haystack| fuzzy_match(token, haystack))
                })
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
                            selected_command = Some(cmd.selection_key.to_string());
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
                                    selected_command = Some(cmd.selection_key.to_string());
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

                ui.separator();
                ui.label(
                    egui::RichText::new("↑↓ navegar · Enter abrir · Esc cerrar")
                        .small()
                        .weak(),
                );
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

#[cfg(test)]
mod tests {
    use super::{all_commands, fuzzy_match, CommandPaletteState};

    #[test]
    fn fuzzy_match_contiene_en_orden() {
        assert!(fuzzy_match("", "cualquier cosa"));
        assert!(fuzzy_match("punto", "Herramienta Punto"));
        assert!(fuzzy_match("PUNTO", "herramienta punto"));
        // Subsecuencia en orden, no sólo subcadena.
        assert!(fuzzy_match("pncl", "Pencil"));
        assert!(fuzzy_match("hrpt", "Herramienta Punto"));
        // El orden importa.
        assert!(!fuzzy_match("ecnp", "Pencil"));
        // Insensible a tildes y a espacios.
        assert!(fuzzy_match("lapiz", "Lápiz"));
        assert!(fuzzy_match("cuadricula", "Alternar cuadrícula"));
        assert!(fuzzy_match("darkmode", "dark mode"));
        // Basura no coincide.
        assert!(!fuzzy_match("zzzqqqx", "Guardar"));
        assert!(!fuzzy_match("zzzqqqx", "Exporta el documento como SVG."));
    }

    #[test]
    fn busqueda_bilingue_encuentra_acciones_en_ambos_idiomas() {
        for (query, expected_key) in [
            ("punto", "Point Tool"),
            ("point", "Point Tool"),
            ("guardar", "Save"),
            ("save", "Save"),
            ("cuadricula", "Toggle Grid"),
            ("grid", "Toggle Grid"),
            ("oscuro", "Toggle Dark Mode"),
            ("dark", "Toggle Dark Mode"),
            ("encuadrar", "Zoom to Fit"),
            ("zoom", "Zoom to Fit"),
            ("lapiz", "Pencil"),
            ("pencil", "Pencil"),
        ] {
            let state = CommandPaletteState {
                search: query.to_string(),
                ..Default::default()
            };
            let keys: Vec<&str> = state
                .filtered_commands()
                .iter()
                .map(|cmd| cmd.selection_key)
                .collect();
            assert!(
                keys.contains(&expected_key),
                "query {query:?} debería encontrar {expected_key:?}, encontró {keys:?}"
            );
        }
    }

    #[test]
    fn busqueda_tambien_revisa_syntax_hint() {
        let state = CommandPaletteState {
            search: "f(x)".to_string(),
            ..Default::default()
        };
        let keys: Vec<&str> = state
            .filtered_commands()
            .iter()
            .map(|cmd| cmd.selection_key)
            .collect();
        assert!(
            keys.contains(&"Function Tool"),
            "buscar 'f(x)' debería encontrar la herramienta función, encontró {keys:?}"
        );

        let state = CommandPaletteState {
            search: "lienzo".to_string(),
            ..Default::default()
        };
        let keys: Vec<&str> = state
            .filtered_commands()
            .iter()
            .map(|cmd| cmd.selection_key)
            .collect();
        assert!(
            keys.contains(&"Point Tool"),
            "buscar 'lienzo' (sólo en syntax_hint) debería encontrar punto, encontró {keys:?}"
        );
    }

    #[test]
    fn acciones_ui_muestran_espanol_y_despachan_clave_inglesa() {
        let commands = all_commands();
        let ui_actions: Vec<_> = commands.iter().filter(|cmd| !cmd.is_registered()).collect();
        assert_eq!(ui_actions.len(), 14);
        // Etiquetas visibles en español rioplatense.
        for expected in [
            "Herramienta Punto",
            "Herramienta Recta",
            "Herramienta Circunferencia",
            "Herramienta Polígono",
            "Herramienta Función",
            "Lápiz",
            "Borrador",
            "Guardar",
            "Exportar SVG",
            "Exportar PNG",
            "Exportar TikZ",
            "Encuadrar todo",
            "Alternar cuadrícula",
            "Alternar modo oscuro",
        ] {
            assert!(
                ui_actions.iter().any(|cmd| cmd.name == expected),
                "falta la etiqueta {expected:?}"
            );
        }
        // Claves estables en inglés para `apply_palette_command` (app.rs).
        for expected_key in [
            "Point Tool",
            "Line Tool",
            "Circle Tool",
            "Polygon Tool",
            "Function Tool",
            "Pencil",
            "Eraser",
            "Save",
            "Export SVG",
            "Export PNG",
            "Export TikZ",
            "Zoom to Fit",
            "Toggle Grid",
            "Toggle Dark Mode",
        ] {
            assert!(
                ui_actions
                    .iter()
                    .any(|cmd| cmd.selection_key == expected_key),
                "falta la clave estable {expected_key:?}"
            );
        }
        let guardar = ui_actions.iter().find(|cmd| cmd.name == "Guardar");
        match guardar {
            Some(g) => {
                assert_eq!(g.selection_key, "Save");
                assert!(g.input_template().is_none());
            }
            None => panic!("falta la acción Guardar"),
        }
    }
}
