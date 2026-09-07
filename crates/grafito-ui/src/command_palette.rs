//! Paleta de comandos de Grafito — búsqueda rápida bilingüe con Ctrl+K.
//!
//! Las acciones de UI muestran su etiqueta en español rioplatense (`name`)
//! pero despachan con una clave estable en inglés (`selection_key`) para no
//! romper `GrafitoApp::apply_palette_command` (grafito-app/src/app.rs),
//! que matchea `"Point Tool"`, `"Save"`, etc. La búsqueda es bilingüe:
//! `keywords` guarda alias en inglés + español y el filtro también revisa
//! `syntax_hint` con [`fuzzy_match`] (subsecuencia en orden, sin tildes).

use crate::i18n::{palette_action, palette_footer, t, Locale};
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
    /// Slug del catálogo i18n (`palette_action`) para las 14 acciones UI;
    /// `None` en comandos del registry (conservan su etiqueta).
    locale_slug: Option<&'static str>,
    insertion: Option<&'static str>,
    command_id: Option<&'static str>,
}

impl PaletteCommand {
    fn registered(spec: &'static CommandSpec) -> Self {
        Self {
            name: spec.palette_label,
            category: spec.category,
            // F10-FIX (OOB latente): `signatures` vacío (imposible vía macro
            // `command!`, posible a mano) → fallback honesto al canónico en
            // vez de `signatures[0]` (index OOB → panic).
            syntax_hint: spec
                .signatures
                .first()
                .map_or(spec.canonical, |sig| sig.syntax),
            help: spec.help,
            selection_key: spec.palette_label,
            keywords: spec.id,
            locale_slug: None,
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

    /// Nombre visible en el idioma pedido. Las 14 acciones UI resuelven vía
    /// catálogo i18n (ES idéntico a `name`); el registry conserva su etiqueta.
    fn localized_name(&self, locale: Locale) -> &'static str {
        match self.locale_slug {
            Some(slug) => {
                let label = palette_action(slug, locale);
                if label.is_empty() {
                    self.name
                } else {
                    label
                }
            }
            None => self.name,
        }
    }

    /// Copia con el nombre visible localizado (clave de despacho intacta).
    fn with_locale(&self, locale: Locale) -> Self {
        Self {
            name: self.localized_name(locale),
            ..*self
        }
    }
}

const UI_ACTIONS: &[PaletteCommand] = &[
    PaletteCommand {
        name: "Herramienta Punto",
        category: "Herramientas",
        syntax_hint: "Clic en el lienzo | (x, y)",
        help: "Activa la herramienta para crear puntos.",
        selection_key: "Point Tool",
        locale_slug: Some("point"),
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
        locale_slug: Some("line"),
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
        locale_slug: Some("circle"),
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
        locale_slug: Some("polygon"),
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
        locale_slug: Some("function"),
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
        locale_slug: Some("pencil"),
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
        locale_slug: Some("eraser"),
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
        locale_slug: Some("save"),
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
        locale_slug: Some("export_svg"),
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
        locale_slug: Some("export_png"),
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
        locale_slug: Some("export_tikz"),
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
        locale_slug: Some("zoom_fit"),
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
        locale_slug: Some("toggle_grid"),
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
        locale_slug: Some("toggle_dark"),
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

/// Todos los comandos con el nombre visible en el idioma pedido (mismo orden
/// que [`all_commands`]; claves de despacho intactas). ES idéntico al actual.
pub fn all_commands_localized(locale: Locale) -> Vec<PaletteCommand> {
    let mut commands: Vec<PaletteCommand> = UI_ACTIONS
        .iter()
        .map(|action| action.with_locale(locale))
        .collect();
    commands.extend(command_registry::palette_commands().map(PaletteCommand::registered));
    commands
}

#[derive(Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub search: String,
    pub selected_index: usize,
}

/// Recientes de la paleta (MRU en memoria, sin I/O ni persistencia).
/// La app lo alimenta con cada despacho; el filtro lo usa para ordenar.
#[derive(Debug, Clone, Default)]
pub struct MruPalette {
    entries: std::collections::VecDeque<String>,
}

impl MruPalette {
    /// Capacidad del MRU: 8 recientes bastan sin tapar la búsqueda.
    pub const CAP: usize = 8;

    /// Registra una clave de despacho (`selection_key`). Sin duplicados.
    pub fn record(&mut self, selection_key: &str) {
        let key = selection_key.trim();
        if key.is_empty() {
            return;
        }
        self.entries.retain(|item| item != key);
        self.entries.push_front(key.to_owned());
        while self.entries.len() > Self::CAP {
            self.entries.pop_back();
        }
    }

    /// Claves recientes, de más a menos reciente.
    pub fn recent(&self) -> Vec<String> {
        self.entries.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Ordena comandos: recientes primero (en orden MRU), resto intacto.
    pub fn apply_order(&self, commands: &[PaletteCommand]) -> Vec<PaletteCommand> {
        if self.entries.is_empty() {
            return commands.to_vec();
        }
        let mut ordered = Vec::with_capacity(commands.len());
        for key in &self.entries {
            if let Some(cmd) = commands.iter().find(|cmd| cmd.selection_key == key) {
                ordered.push(*cmd);
            }
        }
        for cmd in commands {
            if !ordered
                .iter()
                .any(|item| item.selection_key == cmd.selection_key)
            {
                ordered.push(*cmd);
            }
        }
        ordered
    }
}

/// Tooltip rico de un comando: qué es + sintaxis + ayuda, en 3 líneas.
/// Lo usa el hover de la paleta para que ningún comando sea críptico.
pub fn rich_tooltip_for(cmd: &PaletteCommand) -> String {
    format!("{}\n{}\n{}", cmd.name, cmd.syntax_hint, cmd.help)
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
        Self::filter_in(&all_commands(), &self.search)
    }

    /// Filtro bilingüe con nombres visibles en el idioma pedido (ver
    /// [`CommandPaletteState::filtered_commands`]). ES idéntico al actual.
    pub fn filtered_commands_localized(&self, locale: Locale) -> Vec<PaletteCommand> {
        Self::filter_in(&all_commands_localized(locale), &self.search)
    }

    /// Filtrado con recientes MRU primero (sin búsqueda: todo ordenado MRU).
    /// La app pasa su [`MruPalette`] alimentado en cada despacho.
    pub fn filtered_commands_mru(&self, mru: &MruPalette) -> Vec<PaletteCommand> {
        mru.apply_order(&self.filtered_commands())
    }

    /// Núcleo del filtro bilingüe (español + inglés) sobre nombre, categoría,
    /// `syntax_hint`, ayuda y alias. Cada palabra del query debe coincidir
    /// (subcadena o difusa en orden) en al menos un campo.
    fn filter_in(all: &[PaletteCommand], search: &str) -> Vec<PaletteCommand> {
        let query = search.trim().to_lowercase();
        if query.is_empty() {
            return all.to_vec();
        }
        all.iter()
            .copied()
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
        self.clamp_to(len);
    }

    fn clamp_selected_index_localized(&mut self, locale: Locale) {
        let len = self.filtered_commands_localized(locale).len();
        self.clamp_to(len);
    }

    fn clamp_to(&mut self, len: usize) {
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        self.show_localized(ctx, Locale::Es)
    }

    /// Paleta con textos en el idioma pedido (título, vacío y pie vía catálogo
    /// i18n; nombres de acciones vía [`all_commands_localized`]). ES idéntico
    /// a [`CommandPaletteState::show`]; las claves de despacho no cambian.
    pub fn show_localized(&mut self, ctx: &egui::Context, locale: Locale) -> Option<String> {
        if !self.open {
            return None;
        }

        let mut selected_command = None;
        let screen_rect = ctx.screen_rect();
        let mut open = self.open;
        let mut dismissed = false;
        egui::Window::new(t("palette.title", locale))
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
                        let filtered = self.filtered_commands_localized(locale);
                        if let Some(cmd) = filtered.get(self.selected_index) {
                            selected_command = Some(cmd.selection_key.to_string());
                        }
                    }
                    response.request_focus();
                });

                ui.separator();

                let filtered = self.filtered_commands_localized(locale);
                self.clamp_selected_index_localized(locale);
                if filtered.is_empty() {
                    ui.label(t("palette.empty", locale));
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
                                        egui::RichText::new(rich_tooltip_for(cmd)).small().weak(),
                                    );
                                }
                            }
                        });
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    let filtered = self.filtered_commands_localized(locale);
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
                    egui::RichText::new(palette_footer(
                        filtered.len(),
                        all_commands_localized(locale).len(),
                        locale,
                    ))
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
    use super::{
        all_commands, command_registry, fuzzy_match, rich_tooltip_for, CommandPaletteState,
        MruPalette, UI_ACTIONS,
    };

    #[test]
    fn paleta_expone_registro_mas_acciones_ui() {
        let total = all_commands().len();
        assert_eq!(
            total,
            command_registry::palette_commands().count() + UI_ACTIONS.len()
        );
        let state = CommandPaletteState::default();
        assert_eq!(state.filtered_commands().len(), total);
    }

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
    fn comandos_localizados_en_espanol_coinciden_con_la_ui_actual() {
        use super::UI_ACTIONS;
        use crate::i18n::{palette_action, Locale};
        assert_eq!(UI_ACTIONS.len(), 14);
        let current = all_commands();
        let localized = super::all_commands_localized(Locale::Es);
        assert_eq!(current.len(), localized.len());
        for (a, b) in current.iter().zip(localized.iter()) {
            // Migración neutra: mismo orden, mismo nombre, misma clave.
            assert_eq!(a.name, b.name, "ES cambió para {:?}", a.selection_key);
            assert_eq!(a.selection_key, b.selection_key);
            assert_eq!(a.category, b.category);
        }
        // Cada acción UI resuelve su slug en ambas lenguas.
        for action in UI_ACTIONS {
            let slug = action.locale_slug.unwrap_or("");
            assert!(!slug.is_empty(), "sin slug para {:?}", action.selection_key);
            assert_eq!(palette_action(slug, Locale::Es), action.name);
            assert!(!palette_action(slug, Locale::En).is_empty());
        }
    }

    #[test]
    fn paleta_en_ingles_conmuta_nombres_y_conserva_despacho() {
        use crate::i18n::Locale;
        let localized = super::all_commands_localized(Locale::En);
        let ui_actions: Vec<_> = localized
            .iter()
            .filter(|cmd| !cmd.is_registered())
            .collect();
        assert_eq!(ui_actions.len(), 14);
        for expected in [
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
                ui_actions.iter().any(|cmd| cmd.name == expected),
                "falta el nombre EN {expected:?}"
            );
            // La clave de despacho sigue siendo la histórica en inglés.
            assert!(
                ui_actions.iter().any(|cmd| cmd.selection_key == expected),
                "falta la clave estable {expected:?}"
            );
        }
        // El filtro en EN encuentra por nombre inglés y por alias español.
        for (query, expected_key) in [("pencil", "Pencil"), ("cuadricula", "Toggle Grid")] {
            let state = CommandPaletteState {
                search: query.to_string(),
                ..Default::default()
            };
            let keys: Vec<&str> = state
                .filtered_commands_localized(Locale::En)
                .iter()
                .map(|cmd| cmd.selection_key)
                .collect();
            assert!(
                keys.contains(&expected_key),
                "query {query:?} en EN debería encontrar {expected_key:?}, encontró {keys:?}"
            );
        }
        // Pie localizado en ambas lenguas (formato idéntico al actual en ES).
        assert_eq!(
            crate::i18n::palette_footer(3, 207, Locale::Es),
            "3 de 207 · ↑↓ navegar · Enter abrir · Esc cerrar"
        );
        assert_eq!(
            crate::i18n::palette_footer(3, 207, Locale::En),
            "3 of 207 · ↑↓ navigate · Enter open · Esc close"
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

    #[test]
    fn mru_ordena_recientes_primero_sin_perder_comandos() {
        let all = all_commands();
        let mut mru = MruPalette::default();
        assert_eq!(mru.apply_order(&all).len(), all.len());
        mru.record("Save");
        mru.record("Pencil");
        mru.record("");
        mru.record("Save");
        assert_eq!(mru.recent(), vec!["Save".to_string(), "Pencil".to_string()]);
        let ordered = mru.apply_order(&all);
        assert_eq!(ordered.len(), all.len());
        assert_eq!(ordered[0].selection_key, "Save");
        assert_eq!(ordered[1].selection_key, "Pencil");
        // Capacidad acotada.
        for i in 0..20 {
            mru.record(Box::leak(format!("Cmd{i}").into_boxed_str()) as &str);
        }
        assert_eq!(mru.recent().len(), MruPalette::CAP);
        mru.clear();
        assert!(mru.recent().is_empty());
    }

    #[test]
    fn tooltip_rico_combina_nombre_sintaxis_y_ayuda() {
        let cmd = UI_ACTIONS.iter().find(|cmd| cmd.selection_key == "Save");
        match cmd {
            Some(save) => {
                let tip = rich_tooltip_for(save);
                assert!(tip.contains(save.name));
                assert!(tip.contains(save.syntax_hint));
                assert!(tip.contains(save.help));
            }
            None => panic!("falta la acción Guardar"),
        }
    }

    #[test]
    fn filtrado_mru_respeta_busqueda_y_orden() {
        let state = CommandPaletteState {
            search: "punto".to_string(),
            ..Default::default()
        };
        let mut mru = MruPalette::default();
        mru.record("Save");
        let ordered = state.filtered_commands_mru(&mru);
        let plain = state.filtered_commands();
        assert_eq!(ordered.len(), plain.len());
        assert!(ordered.iter().all(|cmd| {
            super::fuzzy_match("punto", cmd.name)
                || super::fuzzy_match("punto", cmd.category)
                || super::fuzzy_match("punto", cmd.syntax_hint)
                || super::fuzzy_match("punto", cmd.help)
                || super::fuzzy_match("punto", cmd.keywords)
        }));
    }
}

// ── F10 hostile fuzz P0 (solo tests, sin tocar prod) ──────────────────────
// F10-FIX: spec con `signatures: &[]` ya no paniquea en
// `PaletteCommand::registered` (antes `spec.signatures[0].syntax`, OOB);
// ahora cae al fallback honesto (canónico).
#[cfg(test)]
mod hostile_crash_f10 {
    use super::*;
    use grafito_command::command_registry::{
        ArgumentKind, CommandSignature, MutationClass, RiskLevel,
    };

    fn empty_sig_spec() -> CommandSpec {
        CommandSpec {
            id: "hostil.vacio",
            canonical: "Hostil",
            aliases: &[],
            signatures: &[],
            help: "comando hostil sin firmas",
            category: "Crear",
            insertion: "Hostil[",
            dispatch_key: "Hostil",
            mutation: MutationClass::CreatesObject,
            risk: RiskLevel::Low,
            palette_visible: true,
            palette_label: "Hostil",
        }
    }

    #[test]
    fn hostile_registered_con_signatures_vacio() {
        // F10-FIX: assert directo de fallback (antes `catch_unwind` que
        // documentaba el panic en [0]). Ya no paniquea: `syntax_hint` cae
        // al canónico honesto.
        let spec: &'static CommandSpec = Box::leak(Box::new(empty_sig_spec()));
        let cmd = PaletteCommand::registered(spec);
        assert_eq!(cmd.syntax_hint, "Hostil");
        assert_eq!(cmd.name, "Hostil");
    }

    #[test]
    fn hostile_all_commands_no_paniquea_con_reales() {
        // Con datos reales (todas tienen ≥1 firma por el macro) no debe paniquear.
        let all = all_commands();
        assert!(!all.is_empty());
        let _ = all_commands_localized(crate::i18n::Locale::Es);
    }

    #[test]
    fn hostile_filtro_con_search_gigante_y_unicode() {
        for search in [
            "",
            "   ",
            "***",
            "(((",
            "\u{1F600}".repeat(500).as_str(),
            "**".repeat(5000).as_str(),
            &"a".repeat(200_000),
            "| a | b |",
            "```".repeat(100).as_str(),
        ] {
            let state = CommandPaletteState {
                search: search.to_string(),
                ..Default::default()
            };
            let _ = state.filtered_commands();
        }
        // fuzzy_match hostil directo
        let _ = fuzzy_match(&"a".repeat(10_000), &"b".repeat(10_000));
        let _ = fuzzy_match("", "");
        let _ = fuzzy_match("***", "\u{1F600}\u{1F600}");
        // signatures con sintaxis vacía/rota no deben tumbar el tooltip
        let cmd = PaletteCommand {
            name: "",
            category: "",
            syntax_hint: "",
            help: "",
            selection_key: "",
            keywords: "",
            locale_slug: None,
            insertion: None,
            command_id: None,
        };
        let _ = rich_tooltip_for(&cmd);
        let _ = CommandSignature {
            syntax: "",
            arguments: &[],
        };
        let _ = ArgumentKind::Unspecified;
    }
}
