//! Grafito i18n — catálogo estático de mensajes ES/EN (tablas estáticas, cero deps).
//!
//! Oleada 2 (Fase E1): deja el catálogo 100% listo para que la Oleada 3 migre
//! los call-sites (`toolbar.rs`, `panels.rs`, `app.rs`, `assistant.rs`,
//! `command_palette.rs`) sin tocar este archivo.
//!
//! - Fuente de verdad: [`MESSAGES`] + [`MSG_COUNT`]. Todo acceso pasa por
//!   [`t`], [`group_label`], [`tool_label`], [`palette_action`],
//!   [`onboarding_msg`], [`cheat_sheet_msg`], [`toast_msg`] o
//!   [`palette_footer`]. Piel pura: sin I/O, sin spawn, sin lógica.
//! - Español idéntico al UI actual (sin normalizar tildes ausentes como
//!   `"Circulo centro-punto"` o `"Lapiz"` — Oleada 3 los migra tal cual).
//! - Números: [`format_number`] es sólo display (ES coma, EN punto, sin
//!   miles, `NaN`/`∞`); [`parse_number_tolerant`] mapea `,`→`.` y rechaza
//!   miles ambiguos (`"1.234,56"` → `None`).
//!
//! ## Ruta a `fluent` (>500 claves)
//!
//! Mientras el catálogo sea <500 claves y sin plurales/género gramatical,
//! estas tablas estáticas son suficientes (lookup lineal, `&'static str`,
//! cero deps, `clippy -D warnings` limpio). Migrar a `fluent` cuando:
//! 1. el catálogo supere ~500 claves (el lineal deja de ser trivial), o
//! 2. se necesiten plurales/género/selectores ICU (`{ $n -> [one] ... *[other] ... }`), o
//! 3. se añada un tercer idioma.
//!
//! Plan de migración (sin romper call-sites): añadir dependencia `fluent`,
//! mover cada `key` a `crates/grafito-ui/locales/{es,en}/grafito.ftl` con el
//! mismo identificador con puntos como message-id, y reimplementar `t()` como
//! lookup al `FluentBundle` manteniendo la firma
//! `t(key: &'static str, locale: Locale) -> String` (el `&'static str` pasa a
//! `String` porque fluent formatea en tiempo de ejecución). Los helpers
//! (`group_label`, `onboarding_msg`, …) no cambian de firma salvo el tipo de
//! retorno.

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// Español (rioplatense donde el UI actual lo usa). Idioma por defecto.
    #[default]
    Es,
    /// English.
    En,
}

impl Locale {
    /// Código BCP-47 del idioma.
    pub const fn code(self) -> &'static str {
        match self {
            Locale::Es => "es",
            Locale::En => "en",
        }
    }
}

/// Una entrada del catálogo: clave estable + texto en ambas lenguas.
#[derive(Debug, Clone, Copy)]
pub struct Msg {
    /// Clave estable con puntos (`"toolbar.group.move"`). Nunca se renombra.
    pub key: &'static str,
    /// Español — idéntico al UI actual.
    pub es: &'static str,
    /// English — traducción completa, sin vacíos.
    pub en: &'static str,
}

impl Msg {
    /// Texto de la entrada en el idioma pedido.
    pub const fn get(self, locale: Locale) -> &'static str {
        match locale {
            Locale::Es => self.es,
            Locale::En => self.en,
        }
    }
}

/// Número total de claves del catálogo. [`MESSAGES`] debe tener exactamente
/// esta longitud (ver test `msg_count_matches_table`).
pub const MSG_COUNT: usize = 165;

/// Catálogo completo ES/EN. Ordenado por dominio:
/// `toolbar.group` (18) + `toolbar.tool` (87) + `palette` (17) +
/// `onboarding` (11) + `cheat` (10) + `toast` (10) + `app`/misc (12) = 165.
pub static MESSAGES: &[Msg] = &[
    // ── toolbar.group (18) — ES idéntico a `ToolGroupId::label` ──
    Msg { key: "toolbar.group.move", es: "Seleccionar", en: "Select" },
    Msg { key: "toolbar.group.point", es: "Puntos", en: "Points" },
    Msg { key: "toolbar.group.line", es: "Rectas", en: "Lines" },
    Msg { key: "toolbar.group.circle", es: "Círculos", en: "Circles" },
    Msg { key: "toolbar.group.polygon", es: "Polígonos", en: "Polygons" },
    Msg { key: "toolbar.group.pencil", es: "Trazo", en: "Stroke" },
    Msg { key: "toolbar.group.eraser", es: "Borrar", en: "Erase" },
    Msg { key: "toolbar.group.conic", es: "Cónicas", en: "Conics" },
    Msg { key: "toolbar.group.curve", es: "Curvas", en: "Curves" },
    Msg { key: "toolbar.group.measure", es: "Medición", en: "Measurement" },
    Msg { key: "toolbar.group.analysis", es: "Análisis", en: "Analysis" },
    Msg { key: "toolbar.group.constraint", es: "Restricciones", en: "Constraints" },
    Msg { key: "toolbar.group.boolean", es: "Booleanas", en: "Booleans" },
    Msg { key: "toolbar.group.threed", es: "3D", en: "3D" },
    Msg { key: "toolbar.group.fourd", es: "4D proyectado", en: "Projected 4D" },
    Msg { key: "toolbar.group.advanced", es: "Avanzado", en: "Advanced" },
    Msg { key: "toolbar.group.transform", es: "Transformar", en: "Transform" },
    Msg { key: "toolbar.group.dynamics", es: "Dinámica", en: "Dynamics" },
    // ── toolbar.tool (87) — ES idéntico a `ToolEntry` en toolbar.rs ──
    Msg { key: "toolbar.tool.select", es: "Seleccionar", en: "Select" },
    Msg { key: "toolbar.tool.point", es: "Punto", en: "Point" },
    Msg { key: "toolbar.tool.midpoint", es: "M Punto medio", en: "Midpoint" },
    Msg { key: "toolbar.tool.line", es: "Recta", en: "Line" },
    Msg { key: "toolbar.tool.segment", es: "Segmento", en: "Segment" },
    Msg { key: "toolbar.tool.ray", es: "Semirrecta", en: "Ray" },
    Msg { key: "toolbar.tool.vector", es: "Vector", en: "Vector" },
    Msg { key: "toolbar.tool.perpendicular", es: "Perpendicular", en: "Perpendicular" },
    Msg { key: "toolbar.tool.circle", es: "Circulo centro-punto", en: "Center-point circle" },
    Msg { key: "toolbar.tool.tangent", es: "Tangente", en: "Tangent" },
    Msg { key: "toolbar.tool.polygon", es: "Poligono", en: "Polygon" },
    Msg { key: "toolbar.tool.regular_polygon", es: "Poligono regular", en: "Regular polygon" },
    Msg { key: "toolbar.tool.pencil", es: "Lapiz", en: "Pencil" },
    Msg { key: "toolbar.tool.eraser", es: "Borrador", en: "Eraser" },
    Msg { key: "toolbar.tool.ellipse_foci", es: "Elipse por focos", en: "Ellipse by foci" },
    Msg { key: "toolbar.tool.parabola_focus", es: "Parabola foco-directriz", en: "Focus-directrix parabola" },
    Msg { key: "toolbar.tool.hyperbola_foci", es: "Hiperbola por focos", en: "Hyperbola by foci" },
    Msg { key: "toolbar.tool.conic_five", es: "Conica por 5 puntos", en: "Conic through 5 points" },
    Msg { key: "toolbar.tool.function", es: "f(x) Función", en: "f(x) Function" },
    Msg { key: "toolbar.tool.param2d", es: "(x,y) Paramétrica 2D", en: "(x,y) 2D parametric" },
    Msg { key: "toolbar.tool.polar", es: "r(t) Polar", en: "r(t) Polar" },
    Msg { key: "toolbar.tool.implicit", es: "F(x,y)=0 Implícita", en: "F(x,y)=0 Implicit" },
    Msg { key: "toolbar.tool.field2d", es: "Campo vectorial", en: "Vector field" },
    Msg { key: "toolbar.tool.locus", es: "Lugar geométrico", en: "Locus" },
    Msg { key: "toolbar.tool.distance", es: "Distancia", en: "Distance" },
    Msg { key: "toolbar.tool.angle", es: "Angulo", en: "Angle" },
    Msg { key: "toolbar.tool.area", es: "Area", en: "Area" },
    Msg { key: "toolbar.tool.slope", es: "m Pendiente", en: "m Slope" },
    Msg { key: "toolbar.tool.root", es: "Raices", en: "Roots" },
    Msg { key: "toolbar.tool.extremum", es: "Extremos", en: "Extrema" },
    Msg { key: "toolbar.tool.inflection", es: "Inflexion", en: "Inflection" },
    Msg { key: "toolbar.tool.yintercept", es: "Interseccion Y", en: "Y intercept" },
    Msg { key: "toolbar.tool.xintercept", es: "Interseccion X", en: "X intercept" },
    Msg { key: "toolbar.tool.intersect", es: "Interseccion", en: "Intersection" },
    Msg { key: "toolbar.tool.analyze", es: "Analizar", en: "Analyze" },
    Msg { key: "toolbar.tool.coincident", es: "Coincidente", en: "Coincident" },
    Msg { key: "toolbar.tool.dist_constraint", es: "Distancia", en: "Distance" },
    Msg { key: "toolbar.tool.angle_constraint", es: "Angulo", en: "Angle" },
    Msg { key: "toolbar.tool.horizontal", es: "Horizontal", en: "Horizontal" },
    Msg { key: "toolbar.tool.vertical", es: "Vertical", en: "Vertical" },
    Msg { key: "toolbar.tool.equal_length", es: "= Igual longitud", en: "= Equal length" },
    Msg { key: "toolbar.tool.symmetry", es: "Simetria", en: "Symmetry" },
    Msg { key: "toolbar.tool.union", es: "Union", en: "Union" },
    Msg { key: "toolbar.tool.intersection", es: "Interseccion", en: "Intersection" },
    Msg { key: "toolbar.tool.difference", es: "Diferencia", en: "Difference" },
    Msg { key: "toolbar.tool.xor", es: "XOR", en: "XOR" },
    Msg { key: "toolbar.tool.point3d", es: "Punto 3D", en: "3D point" },
    Msg { key: "toolbar.tool.segment3d", es: "Segmento 3D", en: "3D segment" },
    Msg { key: "toolbar.tool.line3d", es: "Recta 3D", en: "3D line" },
    Msg { key: "toolbar.tool.plane3d", es: "Plano 3D", en: "3D plane" },
    Msg { key: "toolbar.tool.sphere3d", es: "Esfera", en: "Sphere" },
    Msg { key: "toolbar.tool.cube3d", es: "Cubo", en: "Cube" },
    Msg { key: "toolbar.tool.cylinder3d", es: "Cilindro", en: "Cylinder" },
    Msg { key: "toolbar.tool.cone3d", es: "Cono", en: "Cone" },
    Msg { key: "toolbar.tool.torus3d", es: "Toro", en: "Torus" },
    Msg { key: "toolbar.tool.moebius", es: "Mobius", en: "Möbius strip" },
    Msg { key: "toolbar.tool.surface3d", es: "z Superficie", en: "z Surface" },
    Msg { key: "toolbar.tool.curve3d", es: "(x,y,z) Curva 3D", en: "(x,y,z) 3D curve" },
    Msg { key: "toolbar.tool.field3d", es: "Campo 3D", en: "3D field" },
    Msg { key: "toolbar.tool.hypersurface4d", es: "4D Hipersuperficie", en: "4D hypersurface" },
    Msg { key: "toolbar.tool.tesseract4d", es: "Teseracto 4D: objeto centrado y proyectado", en: "4D tesseract: centered projected object" },
    Msg { key: "toolbar.tool.hypercube5d", es: "Hipercubo 5D: objeto centrado y proyectado", en: "5D hypercube: centered projected object" },
    Msg { key: "toolbar.tool.fractal", es: "Fractal", en: "Fractal" },
    Msg { key: "toolbar.tool.histogram", es: "Histograma", en: "Histogram" },
    Msg { key: "toolbar.tool.scatter", es: "Dispersion", en: "Scatter" },
    Msg { key: "toolbar.tool.domain_coloring", es: "Domain Coloring", en: "Domain Coloring" },
    Msg { key: "toolbar.tool.heatmap", es: "Heat Map", en: "Heat Map" },
    Msg { key: "toolbar.tool.complex_grid", es: "Complex Grid", en: "Complex Grid" },
    Msg { key: "toolbar.tool.slider", es: "Deslizador", en: "Slider" },
    Msg { key: "toolbar.tool.attractor3d", es: "Atractor 3D", en: "3D attractor" },
    Msg { key: "toolbar.tool.parallel", es: "Paralela", en: "Parallel" },
    Msg { key: "toolbar.tool.arc", es: "Arco 3 puntos", en: "3-point arc" },
    Msg { key: "toolbar.tool.sector", es: "Sector circular", en: "Circular sector" },
    Msg { key: "toolbar.tool.button", es: "Botón", en: "Button" },
    Msg { key: "toolbar.tool.image", es: "Imagen", en: "Image" },
    Msg { key: "toolbar.tool.trig_animation", es: "Animación trigonométrica", en: "Trigonometric animation" },
    // ── toolbar.tool F3a (11): ES idéntico a la etiqueta estática de `GROUP_*` ──
    Msg { key: "toolbar.tool.translate", es: "Traslada", en: "Translate" },
    Msg { key: "toolbar.tool.rotate", es: "Rota", en: "Rotate" },
    Msg { key: "toolbar.tool.dilate", es: "Homotecia", en: "Dilate" },
    Msg { key: "toolbar.tool.reflect", es: "Refleja", en: "Reflect" },
    Msg { key: "toolbar.tool.compass", es: "Compás", en: "Compass" },
    Msg { key: "toolbar.tool.semicircle", es: "Semicírculo", en: "Semicircle" },
    Msg { key: "toolbar.tool.spline", es: "Spline", en: "Spline" },
    Msg { key: "toolbar.tool.prism3d", es: "Prisma", en: "Prism" },
    Msg { key: "toolbar.tool.tetrahedron3d", es: "Tetraedro", en: "Tetrahedron" },
    Msg { key: "toolbar.tool.checkbox", es: "Casilla", en: "Checkbox" },
    Msg { key: "toolbar.tool.inputbox", es: "Caja de entrada", en: "Input box" },
    // ── palette (17): 14 acciones UI + título + vacío + pie ──
    // ES idéntico a `UI_ACTIONS` en command_palette.rs; EN = clave estable de despacho.
    Msg { key: "palette.action.point", es: "Herramienta Punto", en: "Point Tool" },
    Msg { key: "palette.action.line", es: "Herramienta Recta", en: "Line Tool" },
    Msg { key: "palette.action.circle", es: "Herramienta Circunferencia", en: "Circle Tool" },
    Msg { key: "palette.action.polygon", es: "Herramienta Polígono", en: "Polygon Tool" },
    Msg { key: "palette.action.function", es: "Herramienta Función", en: "Function Tool" },
    Msg { key: "palette.action.pencil", es: "Lápiz", en: "Pencil" },
    Msg { key: "palette.action.eraser", es: "Borrador", en: "Eraser" },
    Msg { key: "palette.action.save", es: "Guardar", en: "Save" },
    Msg { key: "palette.action.export_svg", es: "Exportar SVG", en: "Export SVG" },
    Msg { key: "palette.action.export_png", es: "Exportar PNG", en: "Export PNG" },
    Msg { key: "palette.action.export_tikz", es: "Exportar TikZ", en: "Export TikZ" },
    Msg { key: "palette.action.zoom_fit", es: "Encuadrar todo", en: "Zoom to Fit" },
    Msg { key: "palette.action.toggle_grid", es: "Alternar cuadrícula", en: "Toggle Grid" },
    Msg { key: "palette.action.toggle_dark", es: "Alternar modo oscuro", en: "Toggle Dark Mode" },
    Msg { key: "palette.title", es: "Paleta de Comandos", en: "Command Palette" },
    Msg { key: "palette.empty", es: "No se encontraron comandos", en: "No commands found" },
    Msg { key: "palette.footer_nav", es: "↑↓ navegar · Enter abrir · Esc cerrar", en: "↑↓ navigate · Enter open · Esc close" },
    // ── onboarding (11) — ES idéntico a `draw_onboarding_window` (app.rs) ──
    Msg { key: "onboarding.title", es: "Bienvenido a Grafito", en: "Welcome to Grafito" },
    Msg { key: "onboarding.subtitle", es: "Grafito — pizarra geométrica interactiva", en: "Grafito — interactive geometry board" },
    Msg { key: "onboarding.bullet_primary", es: "• Construye con 5 herramientas esenciales — Mover, Punto, Recta, Círculo, Polígono", en: "• Build with 5 essential tools — Move, Point, Line, Circle, Polygon" },
    Msg { key: "onboarding.bullet_secondary", es: "• Secundaria añade 3 más — Lápiz, Medida, Análisis (8 total)", en: "• Secondary adds 3 more — Pencil, Measure, Analysis (8 total)" },
    Msg { key: "onboarding.bullet_university", es: "• Universidad desbloquea 18 grupos — Cónicas, 3D, CAS, Estadística, Complejos, Dinámica…", en: "• University unlocks 18 groups — Conics, 3D, CAS, Statistics, Complex, Dynamics…" },
    Msg { key: "onboarding.btn_example", es: "Probar ejemplo", en: "Try an example" },
    Msg { key: "onboarding.btn_empty", es: "Empezar vacío", en: "Start empty" },
    Msg { key: "onboarding.btn_dismiss", es: "No mostrar", en: "Don't show again" },
    Msg { key: "onboarding.toast_example", es: "Ejemplo cargado — ¡explora Grafito!", en: "Example loaded — explore Grafito!" },
    Msg { key: "onboarding.about_title", es: "Acerca de Grafito", en: "About Grafito" },
    Msg { key: "onboarding.hint", es: "Puedes reabrir esta ventana desde Ayuda → Bienvenida", en: "You can reopen this window from Help → Welcome" },
    // ── cheat (10) — hoja de atajos verificados (app.rs handlers + ui.rs menús) ──
    Msg { key: "cheat.title", es: "Atajos de teclado", en: "Keyboard shortcuts" },
    Msg { key: "cheat.save", es: "Guardar: Ctrl+S", en: "Save: Ctrl+S" },
    Msg { key: "cheat.undo_redo", es: "Deshacer / Rehacer: Ctrl+Z / Ctrl+Y", en: "Undo / Redo: Ctrl+Z / Ctrl+Y" },
    Msg { key: "cheat.tools_2d", es: "Herramientas 2D: F1–F6", en: "2D tools: F1–F6" },
    Msg { key: "cheat.tools_3d", es: "3D: F8 Esfera · F9 Cubo", en: "3D: F8 Sphere · F9 Cube" },
    Msg { key: "cheat.pencil_eraser", es: "Lápiz / Borrador: Ctrl+P / Ctrl+E", en: "Pencil / Eraser: Ctrl+P / Ctrl+E" },
    Msg { key: "cheat.palette_theme", es: "Paleta / Tema: Ctrl+K / Ctrl+T", en: "Palette / Theme: Ctrl+K / Ctrl+T" },
    Msg { key: "cheat.analyze_snap", es: "Analizar / Ajuste: Ctrl+A / G", en: "Analyze / Snap: Ctrl+A / G" },
    Msg { key: "cheat.views", es: "Perspectivas: Ctrl+Shift+1…0", en: "Perspectives: Ctrl+Shift+1…0" },
    Msg { key: "cheat.close", es: "Cancelar / Cerrar: Esc", en: "Cancel / Close: Esc" },
    // ── toast (10) — ES idéntico a los `notify` actuales; `{path}`/`{err}` se sustituyen en el call-site ──
    Msg { key: "toast.command_done", es: "Comando completado", en: "Command completed" },
    Msg { key: "toast.command_applied", es: "Comando aplicado en Grafito.", en: "Command applied in Grafito." },
    Msg { key: "toast.saved", es: "Documento guardado en {path}", en: "Document saved to {path}" },
    Msg { key: "toast.opened", es: "Documento abierto desde {path}", en: "Document opened from {path}" },
    Msg { key: "toast.exported", es: "Exportado a {path}", en: "Exported to {path}" },
    Msg { key: "toast.save_cancelled", es: "Guardado cancelado", en: "Save cancelled" },
    Msg { key: "toast.save_error", es: "Error al guardar: {err}", en: "Failed to save: {err}" },
    Msg { key: "toast.load_error", es: "Error al cargar: {err}", en: "Failed to load: {err}" },
    Msg { key: "toast.export_error", es: "Error al exportar: {err}", en: "Failed to export: {err}" },
    Msg { key: "toast.anim_ready", es: "Animación lista.", en: "Animation ready." },
    // ── app / assistant / misc (12) ──
    Msg { key: "app.menu_file", es: "Archivo", en: "File" },
    Msg { key: "app.menu_edit", es: "Editar", en: "Edit" },
    Msg { key: "app.menu_view", es: "Vista", en: "View" },
    Msg { key: "app.menu_help", es: "Ayuda", en: "Help" },
    Msg { key: "assistant.composer_hint", es: "Escribe un mensaje…", en: "Type a message…" },
    Msg { key: "assistant.limit_hint", es: "Caracteres usados del límite de entrada · Enter envía, Shift+Enter salta", en: "Characters used of the input limit · Enter sends, Shift+Enter adds a line" },
    Msg { key: "assistant.copied", es: "Mensaje copiado.", en: "Message copied." },
    Msg { key: "assistant.generating", es: "Generando animación…", en: "Generating animation…" },
    Msg { key: "assistant.teaching_started", es: "Enseñanza iniciada: {topic}", en: "Lesson started: {topic}" },
    Msg { key: "panel.cas_empty", es: "Sin resultado — ejecuta un comando CAS", en: "No result — run a CAS command" },
    Msg { key: "common.cancel", es: "Cancelar", en: "Cancel" },
    Msg { key: "common.retry", es: "Reintentar", en: "Retry" },
];

// ── Acceso ──

/// Devuelve el texto de `key` en el idioma pedido.
///
/// La clave debe ser `&'static str` (literal en el call-site) para poder
/// devolver `&'static str` sin asignar. Si la clave no existe, devuelve la
/// propia clave (fallback visible que la Oleada 3 detecta en revisión).
pub fn t(key: &'static str, locale: Locale) -> &'static str {
    let mut i = 0;
    while i < MESSAGES.len() {
        if MESSAGES[i].key == key {
            return MESSAGES[i].get(locale);
        }
        i += 1;
    }
    key
}

/// Etiqueta de un grupo de la toolbar por slug (`"move"`, `"point"`, …,
/// `"dynamics"`). Slug desconocido → `""`.
pub fn group_label(slug: &str, locale: Locale) -> &'static str {
    match slug {
        "move" => t("toolbar.group.move", locale),
        "point" => t("toolbar.group.point", locale),
        "line" => t("toolbar.group.line", locale),
        "circle" => t("toolbar.group.circle", locale),
        "polygon" => t("toolbar.group.polygon", locale),
        "pencil" => t("toolbar.group.pencil", locale),
        "eraser" => t("toolbar.group.eraser", locale),
        "conic" => t("toolbar.group.conic", locale),
        "curve" => t("toolbar.group.curve", locale),
        "measure" => t("toolbar.group.measure", locale),
        "analysis" => t("toolbar.group.analysis", locale),
        "constraint" => t("toolbar.group.constraint", locale),
        "boolean" => t("toolbar.group.boolean", locale),
        "threed" => t("toolbar.group.threed", locale),
        "fourd" => t("toolbar.group.fourd", locale),
        "advanced" => t("toolbar.group.advanced", locale),
        "transform" => t("toolbar.group.transform", locale),
        "dynamics" => t("toolbar.group.dynamics", locale),
        _ => "",
    }
}

/// Slugs válidos para [`group_label`] (18, en orden de la toolbar).
pub const GROUP_SLUGS: &[&str; 18] = &[
    "move",
    "point",
    "line",
    "circle",
    "polygon",
    "pencil",
    "eraser",
    "conic",
    "curve",
    "transform",
    "measure",
    "analysis",
    "constraint",
    "boolean",
    "threed",
    "fourd",
    "advanced",
    "dynamics",
];

/// Etiqueta de una herramienta por slug (`"select"`, `"point"`, …).
/// Slug desconocido → `""`.
pub fn tool_label(slug: &str, locale: Locale) -> &'static str {
    match slug {
        "select" => t("toolbar.tool.select", locale),
        "point" => t("toolbar.tool.point", locale),
        "midpoint" => t("toolbar.tool.midpoint", locale),
        "line" => t("toolbar.tool.line", locale),
        "segment" => t("toolbar.tool.segment", locale),
        "ray" => t("toolbar.tool.ray", locale),
        "vector" => t("toolbar.tool.vector", locale),
        "perpendicular" => t("toolbar.tool.perpendicular", locale),
        "parallel" => t("toolbar.tool.parallel", locale),
        "circle" => t("toolbar.tool.circle", locale),
        "tangent" => t("toolbar.tool.tangent", locale),
        "arc" => t("toolbar.tool.arc", locale),
        "sector" => t("toolbar.tool.sector", locale),
        "polygon" => t("toolbar.tool.polygon", locale),
        "regular_polygon" => t("toolbar.tool.regular_polygon", locale),
        "pencil" => t("toolbar.tool.pencil", locale),
        "eraser" => t("toolbar.tool.eraser", locale),
        "ellipse_foci" => t("toolbar.tool.ellipse_foci", locale),
        "parabola_focus" => t("toolbar.tool.parabola_focus", locale),
        "hyperbola_foci" => t("toolbar.tool.hyperbola_foci", locale),
        "conic_five" => t("toolbar.tool.conic_five", locale),
        "function" => t("toolbar.tool.function", locale),
        "param2d" => t("toolbar.tool.param2d", locale),
        "polar" => t("toolbar.tool.polar", locale),
        "implicit" => t("toolbar.tool.implicit", locale),
        "field2d" => t("toolbar.tool.field2d", locale),
        "locus" => t("toolbar.tool.locus", locale),
        "distance" => t("toolbar.tool.distance", locale),
        "angle" => t("toolbar.tool.angle", locale),
        "area" => t("toolbar.tool.area", locale),
        "slope" => t("toolbar.tool.slope", locale),
        "root" => t("toolbar.tool.root", locale),
        "extremum" => t("toolbar.tool.extremum", locale),
        "inflection" => t("toolbar.tool.inflection", locale),
        "yintercept" => t("toolbar.tool.yintercept", locale),
        "xintercept" => t("toolbar.tool.xintercept", locale),
        "intersect" => t("toolbar.tool.intersect", locale),
        "analyze" => t("toolbar.tool.analyze", locale),
        "coincident" => t("toolbar.tool.coincident", locale),
        "dist_constraint" => t("toolbar.tool.dist_constraint", locale),
        "angle_constraint" => t("toolbar.tool.angle_constraint", locale),
        "horizontal" => t("toolbar.tool.horizontal", locale),
        "vertical" => t("toolbar.tool.vertical", locale),
        "equal_length" => t("toolbar.tool.equal_length", locale),
        "symmetry" => t("toolbar.tool.symmetry", locale),
        "union" => t("toolbar.tool.union", locale),
        "intersection" => t("toolbar.tool.intersection", locale),
        "difference" => t("toolbar.tool.difference", locale),
        "xor" => t("toolbar.tool.xor", locale),
        "point3d" => t("toolbar.tool.point3d", locale),
        "segment3d" => t("toolbar.tool.segment3d", locale),
        "line3d" => t("toolbar.tool.line3d", locale),
        "plane3d" => t("toolbar.tool.plane3d", locale),
        "sphere3d" => t("toolbar.tool.sphere3d", locale),
        "cube3d" => t("toolbar.tool.cube3d", locale),
        "cylinder3d" => t("toolbar.tool.cylinder3d", locale),
        "cone3d" => t("toolbar.tool.cone3d", locale),
        "torus3d" => t("toolbar.tool.torus3d", locale),
        "moebius" => t("toolbar.tool.moebius", locale),
        "surface3d" => t("toolbar.tool.surface3d", locale),
        "curve3d" => t("toolbar.tool.curve3d", locale),
        "field3d" => t("toolbar.tool.field3d", locale),
        "hypersurface4d" => t("toolbar.tool.hypersurface4d", locale),
        "tesseract4d" => t("toolbar.tool.tesseract4d", locale),
        "hypercube5d" => t("toolbar.tool.hypercube5d", locale),
        "fractal" => t("toolbar.tool.fractal", locale),
        "histogram" => t("toolbar.tool.histogram", locale),
        "scatter" => t("toolbar.tool.scatter", locale),
        "domain_coloring" => t("toolbar.tool.domain_coloring", locale),
        "heatmap" => t("toolbar.tool.heatmap", locale),
        "complex_grid" => t("toolbar.tool.complex_grid", locale),
        "slider" => t("toolbar.tool.slider", locale),
        "button" => t("toolbar.tool.button", locale),
        "image" => t("toolbar.tool.image", locale),
        "attractor3d" => t("toolbar.tool.attractor3d", locale),
        "trig_animation" => t("toolbar.tool.trig_animation", locale),
        "translate" => t("toolbar.tool.translate", locale),
        "rotate" => t("toolbar.tool.rotate", locale),
        "dilate" => t("toolbar.tool.dilate", locale),
        "reflect" => t("toolbar.tool.reflect", locale),
        "compass" => t("toolbar.tool.compass", locale),
        "semicircle" => t("toolbar.tool.semicircle", locale),
        "spline" => t("toolbar.tool.spline", locale),
        "prism3d" => t("toolbar.tool.prism3d", locale),
        "tetrahedron3d" => t("toolbar.tool.tetrahedron3d", locale),
        "checkbox" => t("toolbar.tool.checkbox", locale),
        "inputbox" => t("toolbar.tool.inputbox", locale),
        _ => "",
    }
}

/// Etiqueta de una acción de la paleta por slug (`"point"`, `"save"`, …).
/// Slug desconocido → `""`.
pub fn palette_action(slug: &str, locale: Locale) -> &'static str {
    match slug {
        "point" => t("palette.action.point", locale),
        "line" => t("palette.action.line", locale),
        "circle" => t("palette.action.circle", locale),
        "polygon" => t("palette.action.polygon", locale),
        "function" => t("palette.action.function", locale),
        "pencil" => t("palette.action.pencil", locale),
        "eraser" => t("palette.action.eraser", locale),
        "save" => t("palette.action.save", locale),
        "export_svg" => t("palette.action.export_svg", locale),
        "export_png" => t("palette.action.export_png", locale),
        "export_tikz" => t("palette.action.export_tikz", locale),
        "zoom_fit" => t("palette.action.zoom_fit", locale),
        "toggle_grid" => t("palette.action.toggle_grid", locale),
        "toggle_dark" => t("palette.action.toggle_dark", locale),
        _ => "",
    }
}

/// Pie de la paleta: `"{filtrados} de {total} · {navegación}"`.
/// ES idéntico al formato actual de `command_palette.rs`.
pub fn palette_footer(filtered: usize, total: usize, locale: Locale) -> String {
    match locale {
        Locale::Es => format!(
            "{filtered} de {total} · {}",
            t("palette.footer_nav", locale)
        ),
        Locale::En => format!(
            "{filtered} of {total} · {}",
            t("palette.footer_nav", locale)
        ),
    }
}

// ── Helpers por dominio (Oleada 3 los usa para migrar call-sites) ──

/// Sufijos válidos de `onboarding.*` (11).
pub const ONBOARDING_KEYS: &[&str; 11] = &[
    "title",
    "subtitle",
    "bullet_primary",
    "bullet_secondary",
    "bullet_university",
    "btn_example",
    "btn_empty",
    "btn_dismiss",
    "toast_example",
    "about_title",
    "hint",
];

/// Mensaje de onboarding por sufijo (`"title"`, `"btn_example"`, …).
/// Sufijo desconocido → se devuelve el propio sufijo.
pub fn onboarding_msg(suffix: &'static str, locale: Locale) -> &'static str {
    match suffix {
        "title" => t("onboarding.title", locale),
        "subtitle" => t("onboarding.subtitle", locale),
        "bullet_primary" => t("onboarding.bullet_primary", locale),
        "bullet_secondary" => t("onboarding.bullet_secondary", locale),
        "bullet_university" => t("onboarding.bullet_university", locale),
        "btn_example" => t("onboarding.btn_example", locale),
        "btn_empty" => t("onboarding.btn_empty", locale),
        "btn_dismiss" => t("onboarding.btn_dismiss", locale),
        "toast_example" => t("onboarding.toast_example", locale),
        "about_title" => t("onboarding.about_title", locale),
        "hint" => t("onboarding.hint", locale),
        _ => suffix,
    }
}

/// Sufijos válidos de `cheat.*` (10).
pub const CHEAT_KEYS: &[&str; 10] = &[
    "title",
    "save",
    "undo_redo",
    "tools_2d",
    "tools_3d",
    "pencil_eraser",
    "palette_theme",
    "analyze_snap",
    "views",
    "close",
];

/// Entrada de la hoja de atajos por sufijo. Sufijo desconocido → el sufijo.
pub fn cheat_sheet_msg(suffix: &'static str, locale: Locale) -> &'static str {
    match suffix {
        "title" => t("cheat.title", locale),
        "save" => t("cheat.save", locale),
        "undo_redo" => t("cheat.undo_redo", locale),
        "tools_2d" => t("cheat.tools_2d", locale),
        "tools_3d" => t("cheat.tools_3d", locale),
        "pencil_eraser" => t("cheat.pencil_eraser", locale),
        "palette_theme" => t("cheat.palette_theme", locale),
        "analyze_snap" => t("cheat.analyze_snap", locale),
        "views" => t("cheat.views", locale),
        "close" => t("cheat.close", locale),
        _ => suffix,
    }
}

/// Sufijos válidos de `toast.*` (10).
pub const TOAST_KEYS: &[&str; 10] = &[
    "command_done",
    "command_applied",
    "saved",
    "opened",
    "exported",
    "save_cancelled",
    "save_error",
    "load_error",
    "export_error",
    "anim_ready",
];

/// Plantilla de toast por sufijo (`{path}`/`{err}`/`{topic}` se sustituyen en
/// el call-site con `str::replace`). Sufijo desconocido → el sufijo.
pub fn toast_msg(suffix: &'static str, locale: Locale) -> &'static str {
    match suffix {
        "command_done" => t("toast.command_done", locale),
        "command_applied" => t("toast.command_applied", locale),
        "saved" => t("toast.saved", locale),
        "opened" => t("toast.opened", locale),
        "exported" => t("toast.exported", locale),
        "save_cancelled" => t("toast.save_cancelled", locale),
        "save_error" => t("toast.save_error", locale),
        "load_error" => t("toast.load_error", locale),
        "export_error" => t("toast.export_error", locale),
        "anim_ready" => t("toast.anim_ready", locale),
        _ => suffix,
    }
}

// ── Portugués: overlay parcial F3d (sin variante `Locale`) ──
//
// Por qué overlay y no `Locale::Pt`: los `match` sobre `Locale` en
// `toolbar.rs` (`toolbar_live_text`) y `grafito-app/src/utils.rs` (`AppLocale`)
// son exhaustivos y esos archivos están fuera del alcance de F3d
// (PROHIBIDO: pertenecen a F3a/b/c). Añadir una variante rompería su
// compilación. El tercer idioma completo llega con la migración a `fluent`
// (ver docs del módulo); mientras tanto el PT vive como tabla parcial
// `clave → texto` con fallback al EN en el call-site. El lint `unwrap_used`
// sigue prohibido en prod: el fallback se escribe con `match` o `if let`.
//
// Cobertura F3d: 78 claves principales (18 grupos + 17 paleta + 11 onboarding
// + 10 cheat + 10 toast + 12 app/misc). Las 87 `toolbar.tool` quedan en
// fallback ES/EN a propósito: se miden en el test `pt_reports_tool_fallback`.

/// Una entrada del overlay portugués: clave del catálogo + texto PT.
#[derive(Debug, Clone, Copy)]
pub struct PtMsg {
    /// Clave estable de [`MESSAGES`] (nunca se renombra).
    pub key: &'static str,
    /// Português (BR neutro). Sin vacíos; placeholders `{path}`/`{err}`/`{topic}`
    /// idénticos al ES/EN cuando la clave los lleva.
    pub pt: &'static str,
}

/// Claves principales de UI con traducción PT (78). Ordenado por dominio como
/// [`MESSAGES`]: grupos (18) + paleta (17) + onboarding (11) + cheat (10) +
/// toast (10) + app/misc (12).
pub static PT_MESSAGES: &[PtMsg] = &[
    // ── grupos (18) ──
    PtMsg { key: "toolbar.group.move", pt: "Selecionar" },
    PtMsg { key: "toolbar.group.point", pt: "Pontos" },
    PtMsg { key: "toolbar.group.line", pt: "Retas" },
    PtMsg { key: "toolbar.group.circle", pt: "Círculos" },
    PtMsg { key: "toolbar.group.polygon", pt: "Polígonos" },
    PtMsg { key: "toolbar.group.pencil", pt: "Traço" },
    PtMsg { key: "toolbar.group.eraser", pt: "Apagar" },
    PtMsg { key: "toolbar.group.conic", pt: "Cônicas" },
    PtMsg { key: "toolbar.group.curve", pt: "Curvas" },
    PtMsg { key: "toolbar.group.transform", pt: "Transformar" },
    PtMsg { key: "toolbar.group.measure", pt: "Medição" },
    PtMsg { key: "toolbar.group.analysis", pt: "Análise" },
    PtMsg { key: "toolbar.group.constraint", pt: "Restrições" },
    PtMsg { key: "toolbar.group.boolean", pt: "Booleanas" },
    PtMsg { key: "toolbar.group.threed", pt: "3D" },
    PtMsg { key: "toolbar.group.fourd", pt: "4D projetado" },
    PtMsg { key: "toolbar.group.advanced", pt: "Avançado" },
    PtMsg { key: "toolbar.group.dynamics", pt: "Dinâmica" },
    // ── paleta (17) ──
    PtMsg { key: "palette.action.point", pt: "Ferramenta Ponto" },
    PtMsg { key: "palette.action.line", pt: "Ferramenta Reta" },
    PtMsg { key: "palette.action.circle", pt: "Ferramenta Circunferência" },
    PtMsg { key: "palette.action.polygon", pt: "Ferramenta Polígono" },
    PtMsg { key: "palette.action.function", pt: "Ferramenta Função" },
    PtMsg { key: "palette.action.pencil", pt: "Lápis" },
    PtMsg { key: "palette.action.eraser", pt: "Borracha" },
    PtMsg { key: "palette.action.save", pt: "Salvar" },
    PtMsg { key: "palette.action.export_svg", pt: "Exportar SVG" },
    PtMsg { key: "palette.action.export_png", pt: "Exportar PNG" },
    PtMsg { key: "palette.action.export_tikz", pt: "Exportar TikZ" },
    PtMsg { key: "palette.action.zoom_fit", pt: "Enquadrar tudo" },
    PtMsg { key: "palette.action.toggle_grid", pt: "Alternar grade" },
    PtMsg { key: "palette.action.toggle_dark", pt: "Alternar modo escuro" },
    PtMsg { key: "palette.title", pt: "Paleta de Comandos" },
    PtMsg { key: "palette.empty", pt: "Nenhum comando encontrado" },
    PtMsg { key: "palette.footer_nav", pt: "↑↓ navegar · Enter abrir · Esc fechar" },
    // ── onboarding (11) ──
    PtMsg { key: "onboarding.title", pt: "Bem-vindo ao Grafito" },
    PtMsg { key: "onboarding.subtitle", pt: "Grafito — lousa geométrica interativa" },
    PtMsg { key: "onboarding.bullet_primary", pt: "• Construa com 5 ferramentas essenciais — Mover, Ponto, Reta, Círculo, Polígono" },
    PtMsg { key: "onboarding.bullet_secondary", pt: "• Secundário adiciona mais 3 — Lápis, Medida, Análise (8 no total)" },
    PtMsg { key: "onboarding.bullet_university", pt: "• Universidade desbloqueia 17 grupos — Cônicas, 3D, CAS, Estatística, Complexos, Dinâmica…" },
    PtMsg { key: "onboarding.btn_example", pt: "Testar exemplo" },
    PtMsg { key: "onboarding.btn_empty", pt: "Começar vazio" },
    PtMsg { key: "onboarding.btn_dismiss", pt: "Não mostrar" },
    PtMsg { key: "onboarding.toast_example", pt: "Exemplo carregado — explore o Grafito!" },
    PtMsg { key: "onboarding.about_title", pt: "Sobre o Grafito" },
    PtMsg { key: "onboarding.hint", pt: "Você pode reabrir esta janela em Ajuda → Boas-vindas" },
    // ── cheat (10) ──
    PtMsg { key: "cheat.title", pt: "Atalhos de teclado" },
    PtMsg { key: "cheat.save", pt: "Salvar: Ctrl+S" },
    PtMsg { key: "cheat.undo_redo", pt: "Desfazer / Refazer: Ctrl+Z / Ctrl+Y" },
    PtMsg { key: "cheat.tools_2d", pt: "Ferramentas 2D: F1–F6" },
    PtMsg { key: "cheat.tools_3d", pt: "3D: F8 Esfera · F9 Cubo" },
    PtMsg { key: "cheat.pencil_eraser", pt: "Lápis / Borracha: Ctrl+P / Ctrl+E" },
    PtMsg { key: "cheat.palette_theme", pt: "Paleta / Tema: Ctrl+K / Ctrl+T" },
    PtMsg { key: "cheat.analyze_snap", pt: "Analisar / Ajuste: Ctrl+A / G" },
    PtMsg { key: "cheat.views", pt: "Perspectivas: Ctrl+Shift+1…0" },
    PtMsg { key: "cheat.close", pt: "Cancelar / Fechar: Esc" },
    // ── toast (10) ──
    PtMsg { key: "toast.command_done", pt: "Comando concluído" },
    PtMsg { key: "toast.command_applied", pt: "Comando aplicado no Grafito." },
    PtMsg { key: "toast.saved", pt: "Documento salvo em {path}" },
    PtMsg { key: "toast.opened", pt: "Documento aberto de {path}" },
    PtMsg { key: "toast.exported", pt: "Exportado para {path}" },
    PtMsg { key: "toast.save_cancelled", pt: "Salvamento cancelado" },
    PtMsg { key: "toast.save_error", pt: "Erro ao salvar: {err}" },
    PtMsg { key: "toast.load_error", pt: "Erro ao carregar: {err}" },
    PtMsg { key: "toast.export_error", pt: "Erro ao exportar: {err}" },
    PtMsg { key: "toast.anim_ready", pt: "Animação pronta." },
    // ── app / misc (12) ──
    PtMsg { key: "app.menu_file", pt: "Arquivo" },
    PtMsg { key: "app.menu_edit", pt: "Editar" },
    PtMsg { key: "app.menu_view", pt: "Ver" },
    PtMsg { key: "app.menu_help", pt: "Ajuda" },
    PtMsg { key: "assistant.composer_hint", pt: "Escreva uma mensagem…" },
    PtMsg { key: "assistant.limit_hint", pt: "Caracteres usados do limite de entrada · Enter envia, Shift+Enter pula linha" },
    PtMsg { key: "assistant.copied", pt: "Mensagem copiada." },
    PtMsg { key: "assistant.generating", pt: "Gerando animação…" },
    PtMsg { key: "assistant.teaching_started", pt: "Aula iniciada: {topic}" },
    PtMsg { key: "panel.cas_empty", pt: "Sem resultado — execute um comando CAS" },
    PtMsg { key: "common.cancel", pt: "Cancelar" },
    PtMsg { key: "common.retry", pt: "Tentar de novo" },
];

/// Texto PT de `key`, o `None` si la clave sigue en fallback ES/EN
/// (las 87 `toolbar.tool` hoy: ver test `pt_reports_tool_fallback`).
/// Lookup lineal como [`t`]: el overlay es chico (<100 claves).
pub fn pt(key: &'static str) -> Option<&'static str> {
    let mut i = 0;
    while i < PT_MESSAGES.len() {
        if PT_MESSAGES[i].key == key {
            return Some(PT_MESSAGES[i].pt);
        }
        i += 1;
    }
    None
}

/// Cobertura del overlay PT: `(cubiertas, total del catálogo)`.
/// El numerador lo fija el test `pt_covers_main_ui_keys` en 78.
pub fn pt_coverage() -> (usize, usize) {
    (PT_MESSAGES.len(), MESSAGES.len())
}

// ── Números (display + parse tolerante) ──

/// Formatea un número sólo para mostrar (nunca para persistir ni calcular).
///
/// - ES: coma decimal (`3,14`); EN: punto (`3.14`). Sin separador de miles.
/// - `-0.0` se muestra como `"0"`.
/// - `NaN` → `"NaN"`; `+∞` → `"∞"`; `-∞` → `"-∞"` (igual en ambas lenguas).
pub fn format_number(value: f64, locale: Locale) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        if value > 0.0 {
            return "∞".to_string();
        }
        return "-∞".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let plain = format!("{value}");
    match locale {
        Locale::Es => plain.replace('.', ","),
        Locale::En => plain,
    }
}

/// Interpreta texto del usuario como `f64` aceptando la coma decimal del ES.
///
/// Documentado: `,` se mapea a `.` (`"3,14"` → `3.14`). Si el texto mezcla
/// `.` y `,` (miles ambiguo, p. ej. `"1.234,56"`) devuelve `None` en lugar de
/// adivinar. Acepta `"∞"`/`"-∞"` además de lo que acepta `str::parse`.
/// Recorta espacios externos; cadena vacía → `None`. Sin `unwrap`.
pub fn parse_number_tolerant(text: &str) -> Option<f64> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains('.') && s.contains(',') {
        return None;
    }
    if s == "∞" || s == "+∞" {
        return Some(f64::INFINITY);
    }
    if s == "-∞" {
        return Some(f64::NEG_INFINITY);
    }
    let normalized: String = if s.contains(',') {
        s.replace(',', ".")
    } else {
        s.to_string()
    };
    normalized.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        cheat_sheet_msg, format_number, group_label, onboarding_msg, palette_action,
        palette_footer, parse_number_tolerant, pt, pt_coverage, toast_msg, tool_label, Locale,
        CHEAT_KEYS, GROUP_SLUGS, MESSAGES, MSG_COUNT, ONBOARDING_KEYS, PT_MESSAGES, TOAST_KEYS,
    };

    #[test]
    fn msg_count_matches_table() {
        assert_eq!(
            MESSAGES.len(),
            MSG_COUNT,
            "MSG_COUNT debe seguir a MESSAGES"
        );
        assert_eq!(MSG_COUNT, 165);
    }

    #[test]
    fn no_duplicate_keys() {
        let mut keys: Vec<&str> = MESSAGES.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        let mut i = 1;
        while i < keys.len() {
            assert_ne!(keys[i - 1], keys[i], "clave duplicada: {}", keys[i]);
            i += 1;
        }
    }

    #[test]
    fn every_key_has_both_languages() {
        assert!(!MESSAGES.is_empty());
        for m in MESSAGES {
            assert!(!m.key.is_empty(), "clave vacía");
            assert!(!m.es.is_empty(), "ES vacío en {}", m.key);
            assert!(!m.en.is_empty(), "EN vacío en {}", m.key);
        }
    }

    #[test]
    fn t_fallback_returns_key_for_unknown() {
        assert_eq!(t_unknown(), "does.not.exist");
    }

    fn t_unknown() -> &'static str {
        super::t("does.not.exist", Locale::Es)
    }

    #[test]
    fn t_spanish_matches_current_ui() {
        // ES idéntico a los literales actuales (toolbar / paleta / onboarding).
        let es = Locale::Es;
        assert_eq!(super::t("toolbar.group.move", es), "Seleccionar");
        assert_eq!(super::t("toolbar.tool.circle", es), "Circulo centro-punto");
        assert_eq!(super::t("toolbar.tool.parallel", es), "Paralela");
        assert_eq!(super::t("toolbar.tool.arc", es), "Arco 3 puntos");
        assert_eq!(super::t("toolbar.tool.sector", es), "Sector circular");
        assert_eq!(super::t("toolbar.tool.button", es), "Botón");
        assert_eq!(super::t("toolbar.tool.image", es), "Imagen");
        assert_eq!(
            super::t("toolbar.tool.trig_animation", es),
            "Animación trigonométrica"
        );
        assert_eq!(super::t("toolbar.tool.pencil", es), "Lapiz");
        assert_eq!(super::t("palette.action.point", es), "Herramienta Punto");
        assert_eq!(super::t("palette.empty", es), "No se encontraron comandos");
        assert_eq!(super::t("onboarding.title", es), "Bienvenido a Grafito");
        assert_eq!(super::t("onboarding.btn_example", es), "Probar ejemplo");
        assert_eq!(super::t("onboarding.btn_empty", es), "Empezar vacío");
        assert_eq!(super::t("onboarding.btn_dismiss", es), "No mostrar");
        assert_eq!(
            super::t("onboarding.toast_example", es),
            "Ejemplo cargado — ¡explora Grafito!"
        );
        assert_eq!(super::t("toast.save_cancelled", es), "Guardado cancelado");
        assert_eq!(
            super::t("assistant.limit_hint", es),
            "Caracteres usados del límite de entrada · Enter envía, Shift+Enter salta"
        );
    }

    #[test]
    fn group_labels_cover_18_groups() {
        assert_eq!(GROUP_SLUGS.len(), 18);
        for slug in GROUP_SLUGS {
            assert!(!group_label(slug, Locale::Es).is_empty(), "slug {slug}");
            assert!(!group_label(slug, Locale::En).is_empty(), "slug {slug}");
        }
        assert_eq!(group_label("move", Locale::Es), "Seleccionar");
        assert_eq!(group_label("transform", Locale::Es), "Transformar");
        assert_eq!(group_label("transform", Locale::En), "Transform");
        assert_eq!(group_label("dynamics", Locale::En), "Dynamics");
        assert_eq!(group_label("nope", Locale::Es), "");
        // Herramientas y acciones también resuelven en ambas lenguas.
        assert_eq!(tool_label("sphere3d", Locale::Es), "Esfera");
        assert_eq!(tool_label("sphere3d", Locale::En), "Sphere");
        // F3d: los 11 slugs F3a ya resuelven en ambas lenguas.
        assert_eq!(tool_label("translate", Locale::Es), "Traslada");
        assert_eq!(tool_label("reflect", Locale::En), "Reflect");
        assert_eq!(tool_label("compass", Locale::Es), "Compás");
        assert_eq!(tool_label("semicircle", Locale::En), "Semicircle");
        assert_eq!(tool_label("spline", Locale::Es), "Spline");
        assert_eq!(tool_label("prism3d", Locale::En), "Prism");
        assert_eq!(tool_label("tetrahedron3d", Locale::Es), "Tetraedro");
        assert_eq!(tool_label("checkbox", Locale::En), "Checkbox");
        assert_eq!(tool_label("inputbox", Locale::Es), "Caja de entrada");
        assert_eq!(tool_label("nope", Locale::En), "");
        assert_eq!(palette_action("save", Locale::Es), "Guardar");
        assert_eq!(palette_action("save", Locale::En), "Save");
        assert_eq!(palette_action("nope", Locale::Es), "");
    }

    #[test]
    fn palette_footer_formats_both_locales() {
        assert_eq!(
            palette_footer(3, 213, Locale::Es),
            "3 de 213 · ↑↓ navegar · Enter abrir · Esc cerrar"
        );
        assert_eq!(
            palette_footer(3, 213, Locale::En),
            "3 of 213 · ↑↓ navigate · Enter open · Esc close"
        );
    }

    #[test]
    fn format_number_es_coma_en_punto_sin_miles() {
        assert_eq!(format_number(3.25, Locale::Es), "3,25");
        assert_eq!(format_number(3.25, Locale::En), "3.25");
        assert_eq!(format_number(1000.5, Locale::Es), "1000,5");
        assert_eq!(format_number(1000.5, Locale::En), "1000.5");
        assert_eq!(format_number(0.0, Locale::Es), "0");
        assert_eq!(format_number(-0.0, Locale::En), "0");
        assert_eq!(format_number(f64::NAN, Locale::Es), "NaN");
        assert_eq!(format_number(f64::INFINITY, Locale::En), "∞");
        assert_eq!(format_number(f64::NEG_INFINITY, Locale::Es), "-∞");
    }

    #[test]
    fn parse_number_tolerant_comma_and_invalid() {
        assert_eq!(parse_number_tolerant("3,25"), Some(3.25));
        assert_eq!(parse_number_tolerant("3.25"), Some(3.25));
        assert_eq!(parse_number_tolerant("  -2,5  "), Some(-2.5));
        assert_eq!(parse_number_tolerant("∞"), Some(f64::INFINITY));
        assert_eq!(parse_number_tolerant("-∞"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_number_tolerant(""), None);
        assert_eq!(parse_number_tolerant("   "), None);
        // Miles ambiguo: no adivinar.
        assert_eq!(parse_number_tolerant("1.234,56"), None);
        assert_eq!(parse_number_tolerant("abc"), None);
        // Ida y vuelta display→parse en ES.
        let shown = format_number(2.5, Locale::Es);
        assert_eq!(shown, "2,5");
        assert_eq!(parse_number_tolerant(&shown), Some(2.5));
    }

    #[test]
    fn helper_prefixes_resolve_all_keys() {
        assert_eq!(ONBOARDING_KEYS.len(), 11);
        assert_eq!(CHEAT_KEYS.len(), 10);
        assert_eq!(TOAST_KEYS.len(), 10);
        for k in ONBOARDING_KEYS {
            assert!(!onboarding_msg(k, Locale::Es).is_empty());
            assert!(!onboarding_msg(k, Locale::En).is_empty());
        }
        for k in CHEAT_KEYS {
            assert!(!cheat_sheet_msg(k, Locale::Es).is_empty());
            assert!(!cheat_sheet_msg(k, Locale::En).is_empty());
        }
        for k in TOAST_KEYS {
            assert!(!toast_msg(k, Locale::Es).is_empty());
            assert!(!toast_msg(k, Locale::En).is_empty());
        }
        assert_eq!(onboarding_msg("btn_example", Locale::Es), "Probar ejemplo");
        assert_eq!(cheat_sheet_msg("save", Locale::En), "Save: Ctrl+S");
        assert_eq!(toast_msg("anim_ready", Locale::Es), "Animación lista.");
    }

    #[test]
    fn pt_covers_main_ui_keys() {
        // F3d: overlay parcial PT — 78 claves principales, sin duplicados ni vacíos,
        // cada una existente en el catálogo ES/EN.
        assert_eq!(PT_MESSAGES.len(), 78);
        assert_eq!(pt_coverage(), (78, 165));
        let mut keys: Vec<&str> = PT_MESSAGES.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        let mut i = 1;
        while i < keys.len() {
            assert_ne!(keys[i - 1], keys[i], "clave PT duplicada: {}", keys[i]);
            i += 1;
        }
        for entry in PT_MESSAGES {
            assert!(!entry.key.is_empty(), "clave PT vacía");
            assert!(!entry.pt.is_empty(), "PT vacío en {}", entry.key);
            assert!(
                MESSAGES.iter().any(|m| m.key == entry.key),
                "clave PT fuera del catálogo: {}",
                entry.key
            );
        }
        // Los 18 grupos resuelven por clave completa.
        for slug in GROUP_SLUGS {
            let mut full = String::from("toolbar.group.");
            full.push_str(slug);
            let mut found = false;
            for entry in PT_MESSAGES {
                if entry.key == full {
                    found = true;
                    break;
                }
            }
            assert!(found, "grupo sin PT: {slug}");
        }
        // Paleta completa: 14 acciones + título + vacío + pie.
        for key in [
            "palette.action.point",
            "palette.action.line",
            "palette.action.circle",
            "palette.action.polygon",
            "palette.action.function",
            "palette.action.pencil",
            "palette.action.eraser",
            "palette.action.save",
            "palette.action.export_svg",
            "palette.action.export_png",
            "palette.action.export_tikz",
            "palette.action.zoom_fit",
            "palette.action.toggle_grid",
            "palette.action.toggle_dark",
            "palette.title",
            "palette.empty",
            "palette.footer_nav",
        ] {
            assert!(pt(key).is_some(), "paleta sin PT: {key}");
        }
        // Onboarding / cheat / toast / misc completos.
        for suffix in ONBOARDING_KEYS {
            let mut full = String::from("onboarding.");
            full.push_str(suffix);
            let mut found = false;
            for entry in PT_MESSAGES {
                if entry.key == full {
                    found = true;
                    break;
                }
            }
            assert!(found, "onboarding sin PT: {suffix}");
        }
        for suffix in CHEAT_KEYS {
            let mut full = String::from("cheat.");
            full.push_str(suffix);
            let mut found = false;
            for entry in PT_MESSAGES {
                if entry.key == full {
                    found = true;
                    break;
                }
            }
            assert!(found, "cheat sin PT: {suffix}");
        }
        for suffix in TOAST_KEYS {
            let mut full = String::from("toast.");
            full.push_str(suffix);
            let mut found = false;
            for entry in PT_MESSAGES {
                if entry.key == full {
                    found = true;
                    break;
                }
            }
            assert!(found, "toast sin PT: {suffix}");
        }
        assert_eq!(pt("toolbar.group.transform"), Some("Transformar"));
        assert_eq!(pt("palette.title"), Some("Paleta de Comandos"));
        assert_eq!(pt("does.not.exist"), None);
    }

    #[test]
    fn pt_placeholders_preserved() {
        // Los placeholders del call-site viajan intactos al PT.
        for (key, marker) in [
            ("toast.saved", "{path}"),
            ("toast.opened", "{path}"),
            ("toast.exported", "{path}"),
            ("toast.save_error", "{err}"),
            ("toast.load_error", "{err}"),
            ("toast.export_error", "{err}"),
            ("assistant.teaching_started", "{topic}"),
        ] {
            let text = pt(key).expect("clave principal con PT");
            assert!(
                text.contains(marker),
                "{key} PT debe contener {marker}: {text}"
            );
        }
    }

    #[test]
    fn pt_reports_tool_fallback() {
        // Medición honesta del recorte F3d: las 87 `toolbar.tool` quedan en
        // fallback ES/EN. `pt()` devuelve `None` y el call-site usa EN.
        let mut tool_total = 0;
        let mut tool_covered = 0;
        for m in MESSAGES {
            if m.key.starts_with("toolbar.tool.") {
                tool_total += 1;
                if pt(m.key).is_some() {
                    tool_covered += 1;
                }
            }
        }
        assert_eq!(tool_total, 87);
        assert_eq!(
            tool_covered, 0,
            "tools en PT: el recorte F3d es 0 a propósito"
        );
        assert_eq!(pt("toolbar.tool.translate"), None);
        assert_eq!(tool_label("translate", Locale::En), "Translate");
    }
}
