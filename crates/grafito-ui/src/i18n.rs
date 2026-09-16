//! Grafito i18n — catálogo estático ES/EN + overlays PT/IT/FR/DE (tablas estáticas, cero deps).
//!
//! Oleada 2 (Fase E1): deja el catálogo 100% listo para que la Oleada 3 migre
//! los call-sites (`toolbar.rs`, `panels.rs`, `app.rs`, `assistant.rs`,
//! `command_palette.rs`) sin tocar este archivo.
//!
//! - Fuente de verdad: [`MESSAGES`] + [`MSG_COUNT`]. Todo acceso pasa por
//!   [`t`], [`group_label`], [`tool_label`], [`palette_action`],
//!   [`onboarding_msg`], [`cheat_sheet_msg`], [`toast_msg`] o
//!   [`palette_footer`]. Piel pura: sin I/O, sin spawn, sin lógica.
//! - Español idéntico al UI actual (con tildes correctas como
//!   `"Círculo centro-punto"` o `"Lápiz"` — Onda 1 los normaliza).
//! - Números: [`format_number`] es sólo display (ES/PT/IT/FR/DE coma, EN punto, sin
//!   miles, `NaN`/`∞`); [`parse_number_tolerant`] mapea `,`→`.` y rechaza
//!   miles ambiguos (`"1.234,56"` → `None`).
//!
//! ## Ruta a `fluent` (>500 claves)
//!
//! Mientras el catálogo sea <500 claves y sin plurales/género gramatical,
//! estas tablas estáticas son suficientes (lookup lineal, `&'static str`,
//! cero deps, `clippy -D warnings` limpio). Migrar a `fluent` cuando:
//! 1. el catálogo supere ~500 claves (el lineal deja de ser trivial), o
//! 2. se necesiten plurales/género/selectores ICU (`{ $n -> [one] ... *[other] ... }`).
//!
//! Los idiomas extra (`Locale::Pt` frente W2; `Locale::It`/`Fr`/`De` en este
//! frente) NO exigen `fluent`: viven como variantes plenas con fallback
//! XX→ES→EN en [`t`] (overlays [`PT_MESSAGES`]/[`IT_MESSAGES`]/
//! [`FR_MESSAGES`]/[`DE_MESSAGES`] + catálogo ES/EN, ver docs de [`Locale`]).
//! Nada de migración en este frente.

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// Español (rioplatense donde el UI actual lo usa). Idioma por defecto.
    #[default]
    Es,
    /// English.
    En,
    /// Português (europeo neutro en las claves W2; el overlay inicial nació
    /// BR-neutro en F3d y se conserva tal cual donde no choque).
    ///
    /// Variante plena desde W2: [`t`] resuelve PT si la clave está en
    /// [`PT_MESSAGES`], si no cae a ES (default, siempre completo) y en
    /// última instancia a EN. Ningún `t(key, Pt)` devuelve vacío.
    Pt,
    /// Italiano. Variante plena: [`t`] resuelve IT si la clave está en
    /// [`IT_MESSAGES`], si no cae a ES y en última instancia a EN.
    /// Ningún `t(key, It)` devuelve vacío.
    It,
    /// Français. Variante plena: [`t`] resuelve FR si la clave está en
    /// [`FR_MESSAGES`], si no cae a ES y en última instancia a EN.
    /// Ningún `t(key, Fr)` devuelve vacío.
    Fr,
    /// Deutsch. Variante plena: [`t`] resuelve DE si la clave está en
    /// [`DE_MESSAGES`], si no cae a ES y en última instancia a EN.
    /// Ningún `t(key, De)` devuelve vacío.
    De,
}

impl Locale {
    /// Código BCP-47 del idioma.
    pub const fn code(self) -> &'static str {
        match self {
            Locale::Es => "es",
            Locale::En => "en",
            Locale::Pt => "pt",
            Locale::It => "it",
            Locale::Fr => "fr",
            Locale::De => "de",
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
    ///
    /// `Pt`/`It`/`Fr`/`De` caen a ES aquí a propósito: el `const` no puede
    /// buscar los overlays; el runtime [`t`] sí resuelve cada overlay primero
    /// y solo usa este fallback cuando la clave no tiene traducción.
    pub const fn get(self, locale: Locale) -> &'static str {
        match locale {
            Locale::Es => self.es,
            Locale::En => self.en,
            Locale::Pt | Locale::It | Locale::Fr | Locale::De => self.es,
        }
    }
}

/// Número total de claves del catálogo. [`MESSAGES`] debe tener exactamente
/// esta longitud (ver test `msg_count_matches_table`).
pub const MSG_COUNT: usize = 190;

/// Catálogo completo ES/EN. Ordenado por dominio:
/// `toolbar.group` (18) + `toolbar.tool` (87) + `palette` (19) +
/// `onboarding` (11) + `cheat` (10) + `toast` (10) + `app`/misc (15) +
/// `anim` (2) + `media.title` (14) + `panel.conformal` (3) = 190.
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
    Msg { key: "toolbar.tool.circle", es: "Círculo centro-punto", en: "Center-point circle" },
    Msg { key: "toolbar.tool.tangent", es: "Tangente", en: "Tangent" },
    Msg { key: "toolbar.tool.polygon", es: "Poligono", en: "Polygon" },
    Msg { key: "toolbar.tool.regular_polygon", es: "Poligono regular", en: "Regular polygon" },
    Msg { key: "toolbar.tool.pencil", es: "Lápiz", en: "Pencil" },
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
    // ── palette (18): 15 acciones UI + título + vacío + pie ──
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
    Msg { key: "palette.action.indicate_selection", es: "Indicar selección", en: "Indicate Selection" },
    Msg { key: "palette.title", es: "Paleta de Comandos", en: "Command Palette" },
    Msg { key: "palette.empty", es: "No se encontraron comandos", en: "No commands found" },
    Msg { key: "palette.custom_tools", es: "Herramientas personalizadas", en: "Custom tools" },
    Msg { key: "palette.footer_nav", es: "↑↓ navegar · Enter abrir · Esc cerrar", en: "↑↓ navigate · Enter open · Esc close" },
    // ── onboarding (11) — ES idéntico a `draw_onboarding_window` (app.rs) ──
    Msg { key: "onboarding.title", es: "Bienvenido a Grafito", en: "Welcome to Grafito" },
    Msg { key: "onboarding.subtitle", es: "Grafito — pizarra geométrica interactiva", en: "Grafito — interactive geometry board" },
    Msg { key: "onboarding.bullet_primary", es: "1. Dibujá un punto y una recta", en: "1. Draw a point and a line" },
    Msg { key: "onboarding.bullet_secondary", es: "2. Pedí “graficá y=x²” en el asistente", en: "2. Ask the assistant for “graficá y=x²”" },
    Msg { key: "onboarding.bullet_tertiary", es: "3. Arrastrá un punto y mirá qué se mueve", en: "3. Drag a point and watch what follows" },
    Msg { key: "onboarding.bullet_university", es: "• Universidad desbloquea 18 grupos — Cónicas, 3D, CAS, Estadística, Complejos, Dinámica…", en: "• University unlocks 18 groups — Conics, 3D, CAS, Statistics, Complex, Dynamics…" },
    Msg { key: "onboarding.btn_example", es: "Probar ejemplo", en: "Try an example" },
    Msg { key: "onboarding.btn_empty", es: "Empezar vacío", en: "Start empty" },
    Msg { key: "onboarding.btn_dismiss", es: "No mostrar de nuevo", en: "Don't show again" },
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
    Msg { key: "assistant.composer_hint", es: "Escribí tu pregunta", en: "Write your question" },
    Msg { key: "assistant.composer_pending", es: "Estoy pensando… esperá que termine para mandar otra pregunta.", en: "Thinking… wait until it finishes before sending another question." },
    Msg { key: "assistant.composer_empty", es: "Escribí algo para activar Enviar.", en: "Write something to enable Send." },
    Msg { key: "assistant.composer_keys", es: "Enter envía · Shift+Enter salto de línea", en: "Enter sends · Shift+Enter new line" },
    Msg { key: "assistant.limit_hint", es: "Caracteres usados del límite de entrada · Enter envía, Shift+Enter salta", en: "Characters used of the input limit · Enter sends, Shift+Enter adds a line" },
    Msg { key: "assistant.copied", es: "Mensaje copiado.", en: "Message copied." },
    Msg { key: "assistant.generating", es: "Armando tu animación… ~20 s", en: "Building your animation… ~20 s" },
    Msg { key: "assistant.teaching_started", es: "Enseñanza iniciada: {topic}", en: "Lesson started: {topic}" },
    Msg { key: "panel.cas_empty", es: "Sin resultado — ejecuta un comando CAS", en: "No result — run a CAS command" },
    Msg { key: "common.cancel", es: "Cancelar", en: "Cancel" },
    Msg { key: "common.retry", es: "Reintentar", en: "Retry" },
    // ── anim (2) — guía y mensaje "sin fotogramas" (antes en `anim_native.rs`).
    // `{motor}`/`{guia}` se sustituyen en el call-site (igual que `{path}` en toast).
    Msg { key: "anim.empty.guide", es: "probá bajar la resolución o reintentá", en: "try lowering the resolution or retry" },
    Msg { key: "anim.empty.message", es: "{motor} no produjo fotogramas; {guia}", en: "{motor} produced no frames; {guia}" },
    // ── media.title (14) — títulos curados de cards de animación (antes en
    // `assistant.rs::titulo_curado`). `{expr}`/`{p0}`/`{p1}`/`{param}` se
    // sustituyen en el call-site; las fijas viajan tal cual.
    Msg { key: "media.title.tangent", es: "Tangente móvil · {expr}", en: "Moving tangent · {expr}" },
    Msg { key: "media.title.area", es: "Área acumulada · {expr} [{p0},{p1}]", en: "Accumulated area · {expr} [{p0},{p1}]" },
    Msg { key: "media.title.sweep", es: "Barrido · {expr} ({param})", en: "Sweep · {expr} ({param})" },
    Msg { key: "media.title.trace", es: "Traza · {expr}", en: "Trace · {expr}" },
    Msg { key: "media.title.morph", es: "Transición", en: "Transition" },
    Msg { key: "media.title.locus", es: "Lugar geométrico", en: "Locus" },
    Msg { key: "media.title.integral", es: "Integral — área bajo la curva", en: "Integral — area under the curve" },
    Msg { key: "media.title.derivative", es: "Derivada como pendiente", en: "Derivative as slope" },
    Msg { key: "media.title.pitagoras", es: "Teorema de Pitágoras", en: "Pythagorean theorem" },
    Msg { key: "media.title.taylor", es: "Serie de Taylor", en: "Taylor series" },
    Msg { key: "media.title.conformal", es: "Mapeo conforme", en: "Conformal map" },
    Msg { key: "media.title.subspace", es: "Span lineal", en: "Linear span" },
    Msg { key: "media.title.fractal", es: "Fractal de Koch", en: "Koch fractal" },
    Msg { key: "media.title.default", es: "Animación", en: "Animation" },
    // ── panel.conformal (3) — sección de mapeo conforme en `panels.rs`.
    Msg { key: "panel.conformal.title", es: "Animación de Mapeo Conforme", en: "Conformal mapping animation" },
    Msg { key: "panel.conformal.animate", es: "Animar deformación (homotopía)", en: "Animate deformation (homotopy)" },
    Msg { key: "panel.conformal.speed", es: "Velocidad", en: "Speed" },
];

// ── Acceso ──

/// Devuelve el texto de `key` en el idioma pedido.
///
/// La clave debe ser `&'static str` (literal en el call-site) para poder
/// devolver `&'static str` sin asignar. Si la clave no existe, devuelve la
/// propia clave (fallback visible que la Oleada 3 detecta en revisión).
/// `Pt`/`It`/`Fr`/`De`: overlay propio primero; si la clave no lo tiene, cae
/// a ES (default, siempre completo) y en última instancia a EN. Jamás vacío.
pub fn t(key: &'static str, locale: Locale) -> &'static str {
    if locale == Locale::Pt {
        if let Some(text) = pt(key) {
            return text;
        }
    }
    if locale == Locale::It {
        if let Some(text) = it(key) {
            return text;
        }
    }
    if locale == Locale::Fr {
        if let Some(text) = fr(key) {
            return text;
        }
    }
    if locale == Locale::De {
        if let Some(text) = de(key) {
            return text;
        }
    }
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
        "indicate_selection" => t("palette.action.indicate_selection", locale),
        _ => "",
    }
}

/// Pie de la paleta: `"{filtrados} de {total} · {navegación}"`.
/// ES idéntico al formato actual de `command_palette.rs`.
pub fn palette_footer(filtered: usize, total: usize, locale: Locale) -> String {
    match locale {
        Locale::Es | Locale::Pt => format!(
            "{filtered} de {total} · {}",
            t("palette.footer_nav", locale)
        ),
        Locale::It => format!(
            "{filtered} di {total} · {}",
            t("palette.footer_nav", locale)
        ),
        Locale::Fr => format!(
            "{filtered} sur {total} · {}",
            t("palette.footer_nav", locale)
        ),
        Locale::De => format!(
            "{filtered} von {total} · {}",
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
pub const ONBOARDING_KEYS: &[&str; 12] = &[
    "title",
    "subtitle",
    "bullet_primary",
    "bullet_secondary",
    "bullet_tertiary",
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
        "bullet_tertiary" => t("onboarding.bullet_tertiary", locale),
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

/// Sufijos válidos de `media.title.*` (14).
pub const MEDIA_TITLE_KEYS: &[&str; 14] = &[
    "tangent",
    "area",
    "sweep",
    "trace",
    "morph",
    "locus",
    "integral",
    "derivative",
    "pitagoras",
    "taylor",
    "conformal",
    "subspace",
    "fractal",
    "default",
];

/// Título de card de animación por sufijo (`{expr}`/`{p0}`/`{p1}`/`{param}`
/// se sustituyen en el call-site con `str::replace`). Sufijo desconocido →
/// el sufijo.
pub fn media_title_msg(suffix: &'static str, locale: Locale) -> &'static str {
    match suffix {
        "tangent" => t("media.title.tangent", locale),
        "area" => t("media.title.area", locale),
        "sweep" => t("media.title.sweep", locale),
        "trace" => t("media.title.trace", locale),
        "morph" => t("media.title.morph", locale),
        "locus" => t("media.title.locus", locale),
        "integral" => t("media.title.integral", locale),
        "derivative" => t("media.title.derivative", locale),
        "pitagoras" => t("media.title.pitagoras", locale),
        "taylor" => t("media.title.taylor", locale),
        "conformal" => t("media.title.conformal", locale),
        "subspace" => t("media.title.subspace", locale),
        "fractal" => t("media.title.fractal", locale),
        "default" => t("media.title.default", locale),
        _ => suffix,
    }
}

/// Sufijos válidos de `anim.*` (2).
pub const ANIM_KEYS: &[&str; 2] = &["guide", "message"];

/// Mensaje "sin fotogramas" por sufijo (`{motor}`/`{guia}` se sustituyen en
/// el call-site). Sufijo desconocido → el sufijo.
pub fn anim_msg(suffix: &'static str, locale: Locale) -> &'static str {
    match suffix {
        "guide" => t("anim.empty.guide", locale),
        "message" => t("anim.empty.message", locale),
        _ => suffix,
    }
}

// ── Portugués: overlay completo R3.4 (antes parcial F3d, variante plena W2) ──
//
// F3d lo dejó como tabla parcial `clave → texto` con fallback al EN en el
// call-site porque añadir la variante rompía matches exhaustivos fuera del
// frente. W2 levanta esa restricción: `Locale::Pt` existe y `t(key, Pt)`
// resuelve PT→ES→EN solo (ver `t`). R3.4 completa el overlay al 100%:
// 190 claves (18 grupos + 19 paleta + 12 onboarding + 10 cheat + 10 toast +
// 12 app/misc + 2 anim + 14 media.title + 87 `toolbar.tool` + 3
// `panel.conformal`).
// El lint `unwrap_used` sigue prohibido en prod: el fallback se escribe con
// `match` o `if let`.
//
// Cobertura: 190/190 (100%). Medida real en el test `pt_covers_main_ui_keys`
// (imprime el % por `--nocapture`).

/// Una entrada del overlay portugués: clave del catálogo + texto PT.
#[derive(Debug, Clone, Copy)]
pub struct PtMsg {
    /// Clave estable de [`MESSAGES`] (nunca se renombra).
    pub key: &'static str,
    /// Português (BR neutro). Sin vacíos; placeholders `{path}`/`{err}`/`{topic}`
    /// idénticos al ES/EN cuando la clave los lleva.
    pub pt: &'static str,
}

/// Claves principales de UI con traducción PT (190). Ordenado por dominio como
/// [`MESSAGES`]: grupos (18) + paleta (19) + onboarding (12) + cheat (10) +
/// toast (10) + app/misc (12) + anim (2) + media.title (14) + tools (87) +
/// panel.conformal (3).
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
    // ── paleta (18) ──
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
    PtMsg { key: "palette.action.indicate_selection", pt: "Indicar seleção" },
    PtMsg { key: "palette.title", pt: "Paleta de Comandos" },
    PtMsg { key: "palette.empty", pt: "Nenhum comando encontrado" },
    PtMsg { key: "palette.custom_tools", pt: "Ferramentas personalizadas" },
    PtMsg { key: "palette.footer_nav", pt: "↑↓ navegar · Enter abrir · Esc fechar" },
    // ── onboarding (11) ──
    PtMsg { key: "onboarding.title", pt: "Bem-vindo ao Grafito" },
    PtMsg { key: "onboarding.subtitle", pt: "Grafito — lousa geométrica interativa" },
    PtMsg { key: "onboarding.bullet_primary", pt: "• Construa com 5 ferramentas essenciais — Mover, Ponto, Reta, Círculo, Polígono" },
    PtMsg { key: "onboarding.bullet_secondary", pt: "• Secundário adiciona mais 3 — Lápis, Medida, Análise (8 no total)" },
    PtMsg { key: "onboarding.bullet_university", pt: "• Universidade desbloqueia 18 grupos — Cônicas, 3D, CAS, Estatística, Complexos, Dinâmica…" },
    PtMsg { key: "onboarding.bullet_tertiary", pt: "3. Arraste um ponto e veja o que se move" },
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
    PtMsg { key: "assistant.composer_hint", pt: "Escreva sua pergunta" },
    PtMsg { key: "assistant.composer_pending", pt: "A pensar… aguarde antes de enviar outra pergunta." },
    PtMsg { key: "assistant.composer_empty", pt: "Escreva algo para ativar Enviar." },
    PtMsg { key: "assistant.composer_keys", pt: "Enter envia · Shift+Enter nova linha" },
    PtMsg { key: "assistant.limit_hint", pt: "Caracteres usados do limite de entrada · Enter envia, Shift+Enter pula linha" },
    PtMsg { key: "assistant.copied", pt: "Mensagem copiada." },
    PtMsg { key: "assistant.generating", pt: "Gerando animação…" },
    PtMsg { key: "assistant.teaching_started", pt: "Aula iniciada: {topic}" },
    PtMsg { key: "panel.cas_empty", pt: "Sem resultado — execute um comando CAS" },
    PtMsg { key: "common.cancel", pt: "Cancelar" },
    PtMsg { key: "common.retry", pt: "Tentar de novo" },
    // ── anim (2) ──
    PtMsg { key: "anim.empty.guide", pt: "tente reduzir a resolução ou tentar de novo" },
    PtMsg { key: "anim.empty.message", pt: "{motor} não produziu quadros; {guia}" },
    // ── media.title (14) ──
    PtMsg { key: "media.title.tangent", pt: "Tangente móvel · {expr}" },
    PtMsg { key: "media.title.area", pt: "Área acumulada · {expr} [{p0},{p1}]" },
    PtMsg { key: "media.title.sweep", pt: "Varredura · {expr} ({param})" },
    PtMsg { key: "media.title.trace", pt: "Traço · {expr}" },
    PtMsg { key: "media.title.morph", pt: "Transição" },
    PtMsg { key: "media.title.locus", pt: "Lugar geométrico" },
    PtMsg { key: "media.title.integral", pt: "Integral — área sob a curva" },
    PtMsg { key: "media.title.derivative", pt: "Derivada como inclinação" },
    PtMsg { key: "media.title.pitagoras", pt: "Teorema de Pitágoras" },
    PtMsg { key: "media.title.taylor", pt: "Série de Taylor" },
    PtMsg { key: "media.title.conformal", pt: "Mapeamento conforme" },
    PtMsg { key: "media.title.subspace", pt: "Span linear" },
    PtMsg { key: "media.title.fractal", pt: "Fractal de Koch" },
    PtMsg { key: "media.title.default", pt: "Animação" },
    // ── toolbar.tool (87) — R3.4 cierra el recorte F3d/W2 ──
    PtMsg { key: "toolbar.tool.select", pt: "Selecionar" },
    PtMsg { key: "toolbar.tool.point", pt: "Ponto" },
    PtMsg { key: "toolbar.tool.midpoint", pt: "M Ponto médio" },
    PtMsg { key: "toolbar.tool.line", pt: "Reta" },
    PtMsg { key: "toolbar.tool.segment", pt: "Segmento" },
    PtMsg { key: "toolbar.tool.ray", pt: "Semirreta" },
    PtMsg { key: "toolbar.tool.vector", pt: "Vetor" },
    PtMsg { key: "toolbar.tool.perpendicular", pt: "Perpendicular" },
    PtMsg { key: "toolbar.tool.circle", pt: "Círculo centro-ponto" },
    PtMsg { key: "toolbar.tool.tangent", pt: "Tangente" },
    PtMsg { key: "toolbar.tool.polygon", pt: "Polígono" },
    PtMsg { key: "toolbar.tool.regular_polygon", pt: "Polígono regular" },
    PtMsg { key: "toolbar.tool.pencil", pt: "Lápis" },
    PtMsg { key: "toolbar.tool.eraser", pt: "Borracha" },
    PtMsg { key: "toolbar.tool.ellipse_foci", pt: "Elipse por focos" },
    PtMsg { key: "toolbar.tool.parabola_focus", pt: "Parábola foco-diretriz" },
    PtMsg { key: "toolbar.tool.hyperbola_foci", pt: "Hipérbole por focos" },
    PtMsg { key: "toolbar.tool.conic_five", pt: "Cônica por 5 pontos" },
    PtMsg { key: "toolbar.tool.function", pt: "f(x) Função" },
    PtMsg { key: "toolbar.tool.param2d", pt: "(x,y) Paramétrica 2D" },
    PtMsg { key: "toolbar.tool.polar", pt: "r(t) Polar" },
    PtMsg { key: "toolbar.tool.implicit", pt: "F(x,y)=0 Implícita" },
    PtMsg { key: "toolbar.tool.field2d", pt: "Campo vetorial" },
    PtMsg { key: "toolbar.tool.locus", pt: "Lugar geométrico" },
    PtMsg { key: "toolbar.tool.distance", pt: "Distância" },
    PtMsg { key: "toolbar.tool.angle", pt: "Ângulo" },
    PtMsg { key: "toolbar.tool.area", pt: "Área" },
    PtMsg { key: "toolbar.tool.slope", pt: "m Inclinação" },
    PtMsg { key: "toolbar.tool.root", pt: "Raízes" },
    PtMsg { key: "toolbar.tool.extremum", pt: "Extremos" },
    PtMsg { key: "toolbar.tool.inflection", pt: "Inflexão" },
    PtMsg { key: "toolbar.tool.yintercept", pt: "Intersecção Y" },
    PtMsg { key: "toolbar.tool.xintercept", pt: "Intersecção X" },
    PtMsg { key: "toolbar.tool.intersect", pt: "Intersecção" },
    PtMsg { key: "toolbar.tool.analyze", pt: "Analisar" },
    PtMsg { key: "toolbar.tool.coincident", pt: "Coincidente" },
    PtMsg { key: "toolbar.tool.dist_constraint", pt: "Distância" },
    PtMsg { key: "toolbar.tool.angle_constraint", pt: "Ângulo" },
    PtMsg { key: "toolbar.tool.horizontal", pt: "Horizontal" },
    PtMsg { key: "toolbar.tool.vertical", pt: "Vertical" },
    PtMsg { key: "toolbar.tool.equal_length", pt: "= Mesmo comprimento" },
    PtMsg { key: "toolbar.tool.symmetry", pt: "Simetria" },
    PtMsg { key: "toolbar.tool.union", pt: "União" },
    PtMsg { key: "toolbar.tool.intersection", pt: "Intersecção" },
    PtMsg { key: "toolbar.tool.difference", pt: "Diferença" },
    PtMsg { key: "toolbar.tool.xor", pt: "XOR" },
    PtMsg { key: "toolbar.tool.point3d", pt: "Ponto 3D" },
    PtMsg { key: "toolbar.tool.segment3d", pt: "Segmento 3D" },
    PtMsg { key: "toolbar.tool.line3d", pt: "Reta 3D" },
    PtMsg { key: "toolbar.tool.plane3d", pt: "Plano 3D" },
    PtMsg { key: "toolbar.tool.sphere3d", pt: "Esfera" },
    PtMsg { key: "toolbar.tool.cube3d", pt: "Cubo" },
    PtMsg { key: "toolbar.tool.cylinder3d", pt: "Cilindro" },
    PtMsg { key: "toolbar.tool.cone3d", pt: "Cone" },
    PtMsg { key: "toolbar.tool.torus3d", pt: "Toro" },
    PtMsg { key: "toolbar.tool.moebius", pt: "Möbius" },
    PtMsg { key: "toolbar.tool.surface3d", pt: "z Superfície" },
    PtMsg { key: "toolbar.tool.curve3d", pt: "(x,y,z) Curva 3D" },
    PtMsg { key: "toolbar.tool.field3d", pt: "Campo 3D" },
    PtMsg { key: "toolbar.tool.hypersurface4d", pt: "4D Hipersuperfície" },
    PtMsg { key: "toolbar.tool.tesseract4d", pt: "Tesserato 4D: objeto centrado e projetado" },
    PtMsg { key: "toolbar.tool.hypercube5d", pt: "Hipercubo 5D: objeto centrado e projetado" },
    PtMsg { key: "toolbar.tool.fractal", pt: "Fractal" },
    PtMsg { key: "toolbar.tool.histogram", pt: "Histograma" },
    PtMsg { key: "toolbar.tool.scatter", pt: "Dispersão" },
    PtMsg { key: "toolbar.tool.domain_coloring", pt: "Coloração do domínio" },
    PtMsg { key: "toolbar.tool.heatmap", pt: "Mapa de calor" },
    PtMsg { key: "toolbar.tool.complex_grid", pt: "Grade complexa" },
    PtMsg { key: "toolbar.tool.slider", pt: "Deslizante" },
    PtMsg { key: "toolbar.tool.attractor3d", pt: "Atrator 3D" },
    PtMsg { key: "toolbar.tool.parallel", pt: "Paralela" },
    PtMsg { key: "toolbar.tool.arc", pt: "Arco 3 pontos" },
    PtMsg { key: "toolbar.tool.sector", pt: "Setor circular" },
    PtMsg { key: "toolbar.tool.button", pt: "Botão" },
    PtMsg { key: "toolbar.tool.image", pt: "Imagem" },
    PtMsg { key: "toolbar.tool.trig_animation", pt: "Animação trigonométrica" },
    PtMsg { key: "toolbar.tool.translate", pt: "Translada" },
    PtMsg { key: "toolbar.tool.rotate", pt: "Gira" },
    PtMsg { key: "toolbar.tool.dilate", pt: "Homotetia" },
    PtMsg { key: "toolbar.tool.reflect", pt: "Reflete" },
    PtMsg { key: "toolbar.tool.compass", pt: "Compasso" },
    PtMsg { key: "toolbar.tool.semicircle", pt: "Semicírculo" },
    PtMsg { key: "toolbar.tool.spline", pt: "Spline" },
    PtMsg { key: "toolbar.tool.prism3d", pt: "Prisma" },
    PtMsg { key: "toolbar.tool.tetrahedron3d", pt: "Tetraedro" },
    PtMsg { key: "toolbar.tool.checkbox", pt: "Caixa de seleção" },
    PtMsg { key: "toolbar.tool.inputbox", pt: "Caixa de entrada" },
    // ── panel.conformal (3) — R3.4 cierra el fallback ES ──
    PtMsg { key: "panel.conformal.title", pt: "Animação de Mapeamento Conforme" },
    PtMsg { key: "panel.conformal.animate", pt: "Animar deformação (homotopia)" },
    PtMsg { key: "panel.conformal.speed", pt: "Velocidade" },
];

/// Texto PT de `key`, o `None` si la clave no está en el catálogo.
/// Desde R3.4 el overlay es total (190/190): `None` solo para claves
/// inexistentes. Lookup lineal como [`t`]: el overlay es chico (<200 claves).
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
/// El numerador lo fija el test `pt_covers_main_ui_keys` en 190.
pub fn pt_coverage() -> (usize, usize) {
    (PT_MESSAGES.len(), MESSAGES.len())
}

/// Badge honesto del overlay parcial PT (P1b, visible en el selector).
///
/// R3.4: cobertura 100%, el badge ya no se muestra (ver `toolbar.rs`:
/// solo se dibuja si `pt_is_partial()`). Se conserva la constante y el texto
/// con conteo para el hover histórico y el test que pinnea el 100%.
pub const PT_PARTIAL_BADGE: &str = "Português parcial";

/// `true` mientras el overlay PT no cubra el catálogo (R3.4: 190/190 = falso).
pub fn pt_is_partial() -> bool {
    let (cubiertas, total) = pt_coverage();
    cubiertas < total
}

/// Texto del badge con conteo real, p. ej. `"Português parcial · 190/190"`.
/// Puro, sin I/O: el selector lo muestra solo si `pt_is_partial()`.
pub fn pt_partial_badge_text() -> String {
    let (cubiertas, total) = pt_coverage();
    format!("{PT_PARTIAL_BADGE} · {cubiertas}/{total}")
}

// ── Italiano / Français / Deutsch: overlays completos (190/190 c/u) ──
//
// Generados desde `/tmp/opencode/i18n_table.txt` (190 líneas `clave|it|fr|de`,
// mismo orden que `MESSAGES`, texto tal cual sin re-traducir). Patrón idéntico
// al overlay PT (`PT_MESSAGES` + `pt()` + fallback en `t`): cada idioma resuelve
// su overlay primero y cae a ES (default, siempre completo) y luego a EN.
// Ningún `t(key, It|Fr|De)` devuelve vacío. Lookup lineal: el overlay es chico
// (<200 claves). Sin `unwrap` en prod: el fallback se escribe con `if let`.

/// Una entrada de overlay IT/FR/DE: clave del catálogo + texto traducido.
#[derive(Debug, Clone, Copy)]
pub struct OverlayMsg {
    /// Clave estable de [`MESSAGES`] (nunca se renombra).
    pub key: &'static str,
    /// Texto traducido tal cual de la tabla (`{path}`/`{err}`/`{topic}`/
    /// `{motor}`/`{guia}`/`{expr}`/`{p0}`/`{p1}`/`{param}` idénticos al ES/EN
    /// cuando la clave los lleva. Sin vacíos.
    pub text: &'static str,
}

/// Claves principales de UI con traducción al Italiano (190). Ordenado por dominio
/// como [`MESSAGES`]: grupos (18) + tools (87) + paleta (19) + onboarding (12) +
/// cheat (10) + toast (10) + app/misc (12) + anim (2) + media.title (14) +
/// panel.conformal (3).
pub static IT_MESSAGES: &[OverlayMsg] = &[
    // ── grupos (18) ──
    OverlayMsg {
        key: "toolbar.group.move",
        text: "Seleziona",
    },
    OverlayMsg {
        key: "toolbar.group.point",
        text: "Punti",
    },
    OverlayMsg {
        key: "toolbar.group.line",
        text: "Rette",
    },
    OverlayMsg {
        key: "toolbar.group.circle",
        text: "Cerchi",
    },
    OverlayMsg {
        key: "toolbar.group.polygon",
        text: "Poligoni",
    },
    OverlayMsg {
        key: "toolbar.group.pencil",
        text: "Tratto",
    },
    OverlayMsg {
        key: "toolbar.group.eraser",
        text: "Cancella",
    },
    OverlayMsg {
        key: "toolbar.group.conic",
        text: "Coniche",
    },
    OverlayMsg {
        key: "toolbar.group.curve",
        text: "Curve",
    },
    OverlayMsg {
        key: "toolbar.group.measure",
        text: "Misura",
    },
    OverlayMsg {
        key: "toolbar.group.analysis",
        text: "Analisi",
    },
    OverlayMsg {
        key: "toolbar.group.constraint",
        text: "Vincoli",
    },
    OverlayMsg {
        key: "toolbar.group.boolean",
        text: "Booleane",
    },
    OverlayMsg {
        key: "toolbar.group.threed",
        text: "3D",
    },
    OverlayMsg {
        key: "toolbar.group.fourd",
        text: "4D proiettato",
    },
    OverlayMsg {
        key: "toolbar.group.advanced",
        text: "Avanzate",
    },
    OverlayMsg {
        key: "toolbar.group.transform",
        text: "Trasforma",
    },
    OverlayMsg {
        key: "toolbar.group.dynamics",
        text: "Dinamica",
    },
    // ── tools (87) ──
    OverlayMsg {
        key: "toolbar.tool.select",
        text: "Seleziona",
    },
    OverlayMsg {
        key: "toolbar.tool.point",
        text: "Punto",
    },
    OverlayMsg {
        key: "toolbar.tool.midpoint",
        text: "Punto medio",
    },
    OverlayMsg {
        key: "toolbar.tool.line",
        text: "Retta",
    },
    OverlayMsg {
        key: "toolbar.tool.segment",
        text: "Segmento",
    },
    OverlayMsg {
        key: "toolbar.tool.ray",
        text: "Semiretta",
    },
    OverlayMsg {
        key: "toolbar.tool.vector",
        text: "Vettore",
    },
    OverlayMsg {
        key: "toolbar.tool.perpendicular",
        text: "Perpendicolare",
    },
    OverlayMsg {
        key: "toolbar.tool.circle",
        text: "Cerchio centro-punto",
    },
    OverlayMsg {
        key: "toolbar.tool.tangent",
        text: "Tangente",
    },
    OverlayMsg {
        key: "toolbar.tool.polygon",
        text: "Poligono",
    },
    OverlayMsg {
        key: "toolbar.tool.regular_polygon",
        text: "Poligono regolare",
    },
    OverlayMsg {
        key: "toolbar.tool.pencil",
        text: "Matita",
    },
    OverlayMsg {
        key: "toolbar.tool.eraser",
        text: "Gomma",
    },
    OverlayMsg {
        key: "toolbar.tool.ellipse_foci",
        text: "Ellisse per fuochi",
    },
    OverlayMsg {
        key: "toolbar.tool.parabola_focus",
        text: "Parabola fuoco-direttrice",
    },
    OverlayMsg {
        key: "toolbar.tool.hyperbola_foci",
        text: "Iperbole per fuochi",
    },
    OverlayMsg {
        key: "toolbar.tool.conic_five",
        text: "Conica per 5 punti",
    },
    OverlayMsg {
        key: "toolbar.tool.function",
        text: "f(x) Funzione",
    },
    OverlayMsg {
        key: "toolbar.tool.param2d",
        text: "(x,y) Parametrica 2D",
    },
    OverlayMsg {
        key: "toolbar.tool.polar",
        text: "r(t) Polare",
    },
    OverlayMsg {
        key: "toolbar.tool.implicit",
        text: "F(x,y)=0 Implicita",
    },
    OverlayMsg {
        key: "toolbar.tool.field2d",
        text: "Campo vettoriale",
    },
    OverlayMsg {
        key: "toolbar.tool.locus",
        text: "Luogo geometrico",
    },
    OverlayMsg {
        key: "toolbar.tool.distance",
        text: "Distanza",
    },
    OverlayMsg {
        key: "toolbar.tool.angle",
        text: "Angolo",
    },
    OverlayMsg {
        key: "toolbar.tool.area",
        text: "Area",
    },
    OverlayMsg {
        key: "toolbar.tool.slope",
        text: "m Pendenza",
    },
    OverlayMsg {
        key: "toolbar.tool.root",
        text: "Radici",
    },
    OverlayMsg {
        key: "toolbar.tool.extremum",
        text: "Estremi",
    },
    OverlayMsg {
        key: "toolbar.tool.inflection",
        text: "Flesso",
    },
    OverlayMsg {
        key: "toolbar.tool.yintercept",
        text: "Intersezione Y",
    },
    OverlayMsg {
        key: "toolbar.tool.xintercept",
        text: "Intersezione X",
    },
    OverlayMsg {
        key: "toolbar.tool.intersect",
        text: "Intersezione",
    },
    OverlayMsg {
        key: "toolbar.tool.analyze",
        text: "Analizza",
    },
    OverlayMsg {
        key: "toolbar.tool.coincident",
        text: "Coincidente",
    },
    OverlayMsg {
        key: "toolbar.tool.dist_constraint",
        text: "Distanza",
    },
    OverlayMsg {
        key: "toolbar.tool.angle_constraint",
        text: "Angolo",
    },
    OverlayMsg {
        key: "toolbar.tool.horizontal",
        text: "Orizzontale",
    },
    OverlayMsg {
        key: "toolbar.tool.vertical",
        text: "Verticale",
    },
    OverlayMsg {
        key: "toolbar.tool.equal_length",
        text: "= Uguale lunghezza",
    },
    OverlayMsg {
        key: "toolbar.tool.symmetry",
        text: "Simmetria",
    },
    OverlayMsg {
        key: "toolbar.tool.union",
        text: "Unione",
    },
    OverlayMsg {
        key: "toolbar.tool.intersection",
        text: "Intersezione",
    },
    OverlayMsg {
        key: "toolbar.tool.difference",
        text: "Differenza",
    },
    OverlayMsg {
        key: "toolbar.tool.xor",
        text: "XOR",
    },
    OverlayMsg {
        key: "toolbar.tool.point3d",
        text: "Punto 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.segment3d",
        text: "Segmento 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.line3d",
        text: "Retta 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.plane3d",
        text: "Piano 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.sphere3d",
        text: "Sfera",
    },
    OverlayMsg {
        key: "toolbar.tool.cube3d",
        text: "Cubo",
    },
    OverlayMsg {
        key: "toolbar.tool.cylinder3d",
        text: "Cilindro",
    },
    OverlayMsg {
        key: "toolbar.tool.cone3d",
        text: "Cono",
    },
    OverlayMsg {
        key: "toolbar.tool.torus3d",
        text: "Toro",
    },
    OverlayMsg {
        key: "toolbar.tool.moebius",
        text: "Nastro di Möbius",
    },
    OverlayMsg {
        key: "toolbar.tool.surface3d",
        text: "z Superficie",
    },
    OverlayMsg {
        key: "toolbar.tool.curve3d",
        text: "(x,y,z) Curva 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.field3d",
        text: "Campo 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.hypersurface4d",
        text: "Ipersuperficie 4D",
    },
    OverlayMsg {
        key: "toolbar.tool.tesseract4d",
        text: "Tesseratto 4D: oggetto centrato e proiettato",
    },
    OverlayMsg {
        key: "toolbar.tool.hypercube5d",
        text: "Ipercubo 5D: oggetto centrato e proiettato",
    },
    OverlayMsg {
        key: "toolbar.tool.fractal",
        text: "Frattale",
    },
    OverlayMsg {
        key: "toolbar.tool.histogram",
        text: "Istogramma",
    },
    OverlayMsg {
        key: "toolbar.tool.scatter",
        text: "Dispersione",
    },
    OverlayMsg {
        key: "toolbar.tool.domain_coloring",
        text: "Domain Coloring",
    },
    OverlayMsg {
        key: "toolbar.tool.heatmap",
        text: "Heat Map",
    },
    OverlayMsg {
        key: "toolbar.tool.complex_grid",
        text: "Complex Grid",
    },
    OverlayMsg {
        key: "toolbar.tool.slider",
        text: "Cursore",
    },
    OverlayMsg {
        key: "toolbar.tool.attractor3d",
        text: "Attrattore 3D",
    },
    OverlayMsg {
        key: "toolbar.tool.parallel",
        text: "Parallela",
    },
    OverlayMsg {
        key: "toolbar.tool.arc",
        text: "Arco per 3 punti",
    },
    OverlayMsg {
        key: "toolbar.tool.sector",
        text: "Settore circolare",
    },
    OverlayMsg {
        key: "toolbar.tool.button",
        text: "Pulsante",
    },
    OverlayMsg {
        key: "toolbar.tool.image",
        text: "Immagine",
    },
    OverlayMsg {
        key: "toolbar.tool.trig_animation",
        text: "Animazione trigonometrica",
    },
    OverlayMsg {
        key: "toolbar.tool.translate",
        text: "Trasla",
    },
    OverlayMsg {
        key: "toolbar.tool.rotate",
        text: "Ruota",
    },
    OverlayMsg {
        key: "toolbar.tool.dilate",
        text: "Omotetia",
    },
    OverlayMsg {
        key: "toolbar.tool.reflect",
        text: "Rifletti",
    },
    OverlayMsg {
        key: "toolbar.tool.compass",
        text: "Compasso",
    },
    OverlayMsg {
        key: "toolbar.tool.semicircle",
        text: "Semicerchio",
    },
    OverlayMsg {
        key: "toolbar.tool.spline",
        text: "Spline",
    },
    OverlayMsg {
        key: "toolbar.tool.prism3d",
        text: "Prisma",
    },
    OverlayMsg {
        key: "toolbar.tool.tetrahedron3d",
        text: "Tetraedro",
    },
    OverlayMsg {
        key: "toolbar.tool.checkbox",
        text: "Casella",
    },
    OverlayMsg {
        key: "toolbar.tool.inputbox",
        text: "Casella di input",
    },
    // ── paleta (19) ──
    OverlayMsg {
        key: "palette.action.point",
        text: "Strumento Punto",
    },
    OverlayMsg {
        key: "palette.action.line",
        text: "Strumento Retta",
    },
    OverlayMsg {
        key: "palette.action.circle",
        text: "Strumento Circonferenza",
    },
    OverlayMsg {
        key: "palette.action.polygon",
        text: "Strumento Poligono",
    },
    OverlayMsg {
        key: "palette.action.function",
        text: "Strumento Funzione",
    },
    OverlayMsg {
        key: "palette.action.pencil",
        text: "Matita",
    },
    OverlayMsg {
        key: "palette.action.eraser",
        text: "Gomma",
    },
    OverlayMsg {
        key: "palette.action.save",
        text: "Salva",
    },
    OverlayMsg {
        key: "palette.action.export_svg",
        text: "Esporta SVG",
    },
    OverlayMsg {
        key: "palette.action.export_png",
        text: "Esporta PNG",
    },
    OverlayMsg {
        key: "palette.action.export_tikz",
        text: "Esporta TikZ",
    },
    OverlayMsg {
        key: "palette.action.zoom_fit",
        text: "Inquadra tutto",
    },
    OverlayMsg {
        key: "palette.action.toggle_grid",
        text: "Attiva/disattiva griglia",
    },
    OverlayMsg {
        key: "palette.action.toggle_dark",
        text: "Attiva/disattiva tema scuro",
    },
    OverlayMsg {
        key: "palette.action.indicate_selection",
        text: "Indica selezione",
    },
    OverlayMsg {
        key: "palette.title",
        text: "Tavolozza comandi",
    },
    OverlayMsg {
        key: "palette.empty",
        text: "Nessun comando trovato",
    },
    OverlayMsg {
        key: "palette.custom_tools",
        text: "Strumenti personalizzati",
    },
    OverlayMsg {
        key: "palette.footer_nav",
        text: "↑↓ naviga · Invio apri · Esc chiudi",
    },
    // ── onboarding (12) ──
    OverlayMsg {
        key: "onboarding.title",
        text: "Benvenuto in Grafito",
    },
    OverlayMsg {
        key: "onboarding.subtitle",
        text: "Grafito — lavagna geometrica interattiva",
    },
    OverlayMsg {
        key: "onboarding.bullet_primary",
        text: "1. Disegna un punto e una retta",
    },
    OverlayMsg {
        key: "onboarding.bullet_secondary",
        text: "2. Chiedi all'assistente “grafica y=x²”",
    },
    OverlayMsg {
        key: "onboarding.bullet_tertiary",
        text: "3. Trascina un punto e guarda cosa si muove",
    },
    OverlayMsg {
        key: "onboarding.bullet_university",
        text:
            "• L'università sblocca 18 gruppi — Coniche, 3D, CAS, Statistica, Complessi, Dinamica…",
    },
    OverlayMsg {
        key: "onboarding.btn_example",
        text: "Prova un esempio",
    },
    OverlayMsg {
        key: "onboarding.btn_empty",
        text: "Inizia vuoto",
    },
    OverlayMsg {
        key: "onboarding.btn_dismiss",
        text: "Non mostrare più",
    },
    OverlayMsg {
        key: "onboarding.toast_example",
        text: "Esempio caricato — esplora Grafito!",
    },
    OverlayMsg {
        key: "onboarding.about_title",
        text: "Informazioni su Grafito",
    },
    OverlayMsg {
        key: "onboarding.hint",
        text: "Puoi riaprire questa finestra da Aiuto → Benvenuto",
    },
    // ── cheat (10) ──
    OverlayMsg {
        key: "cheat.title",
        text: "Scorciatoie da tastiera",
    },
    OverlayMsg {
        key: "cheat.save",
        text: "Salva: Ctrl+S",
    },
    OverlayMsg {
        key: "cheat.undo_redo",
        text: "Annulla / Ripeti: Ctrl+Z / Ctrl+Y",
    },
    OverlayMsg {
        key: "cheat.tools_2d",
        text: "Strumenti 2D: F1–F6",
    },
    OverlayMsg {
        key: "cheat.tools_3d",
        text: "3D: F8 Sfera · F9 Cubo",
    },
    OverlayMsg {
        key: "cheat.pencil_eraser",
        text: "Matita / Gomma: Ctrl+P / Ctrl+E",
    },
    OverlayMsg {
        key: "cheat.palette_theme",
        text: "Tavolozza / Tema: Ctrl+K / Ctrl+T",
    },
    OverlayMsg {
        key: "cheat.analyze_snap",
        text: "Analizza / Allinea: Ctrl+A / G",
    },
    OverlayMsg {
        key: "cheat.views",
        text: "Prospettive: Ctrl+Shift+1…0",
    },
    OverlayMsg {
        key: "cheat.close",
        text: "Annulla / Chiudi: Esc",
    },
    // ── toast (10) ──
    OverlayMsg {
        key: "toast.command_done",
        text: "Comando completato",
    },
    OverlayMsg {
        key: "toast.command_applied",
        text: "Comando applicato in Grafito.",
    },
    OverlayMsg {
        key: "toast.saved",
        text: "Documento salvato in {path}",
    },
    OverlayMsg {
        key: "toast.opened",
        text: "Documento aperto da {path}",
    },
    OverlayMsg {
        key: "toast.exported",
        text: "Esportato in {path}",
    },
    OverlayMsg {
        key: "toast.save_cancelled",
        text: "Salvataggio annullato",
    },
    OverlayMsg {
        key: "toast.save_error",
        text: "Errore di salvataggio: {err}",
    },
    OverlayMsg {
        key: "toast.load_error",
        text: "Errore di caricamento: {err}",
    },
    OverlayMsg {
        key: "toast.export_error",
        text: "Errore di esportazione: {err}",
    },
    OverlayMsg {
        key: "toast.anim_ready",
        text: "Animazione pronta.",
    },
    // ── app / misc (12) ──
    OverlayMsg {
        key: "app.menu_file",
        text: "File",
    },
    OverlayMsg {
        key: "app.menu_edit",
        text: "Modifica",
    },
    OverlayMsg {
        key: "app.menu_view",
        text: "Vista",
    },
    OverlayMsg {
        key: "app.menu_help",
        text: "Aiuto",
    },
    OverlayMsg {
        key: "assistant.composer_hint",
        text: "Scrivi la tua domanda",
    },
    OverlayMsg {
        key: "assistant.composer_pending",
        text: "Sto pensando… attendi prima di inviare un'altra domanda.",
    },
    OverlayMsg {
        key: "assistant.composer_empty",
        text: "Scrivi qualcosa per attivare Invia.",
    },
    OverlayMsg {
        key: "assistant.composer_keys",
        text: "Invio invia · Shift+Invio a capo",
    },
    OverlayMsg {
        key: "assistant.limit_hint",
        text: "Caratteri usati del limite · Invio invia, Shift+Invio a capo",
    },
    OverlayMsg {
        key: "assistant.copied",
        text: "Messaggio copiato.",
    },
    OverlayMsg {
        key: "assistant.generating",
        text: "Creo la tua animazione… ~20 s",
    },
    OverlayMsg {
        key: "assistant.teaching_started",
        text: "Lezione iniziata: {topic}",
    },
    OverlayMsg {
        key: "panel.cas_empty",
        text: "Nessun risultato — esegui un comando CAS",
    },
    OverlayMsg {
        key: "common.cancel",
        text: "Annulla",
    },
    OverlayMsg {
        key: "common.retry",
        text: "Riprova",
    },
    // ── anim (2) ──
    OverlayMsg {
        key: "anim.empty.guide",
        text: "prova ad abbassare la risoluzione o riprova",
    },
    OverlayMsg {
        key: "anim.empty.message",
        text: "{motor} non ha prodotto fotogrammi; {guia}",
    },
    // ── media.title (14) ──
    OverlayMsg {
        key: "media.title.tangent",
        text: "Tangente mobile · {expr}",
    },
    OverlayMsg {
        key: "media.title.area",
        text: "Area accumulata · {expr} [{p0},{p1}]",
    },
    OverlayMsg {
        key: "media.title.sweep",
        text: "Scansione · {expr} ({param})",
    },
    OverlayMsg {
        key: "media.title.trace",
        text: "Traccia · {expr}",
    },
    OverlayMsg {
        key: "media.title.morph",
        text: "Transizione",
    },
    OverlayMsg {
        key: "media.title.locus",
        text: "Luogo geometrico",
    },
    OverlayMsg {
        key: "media.title.integral",
        text: "Integrale — area sotto la curva",
    },
    OverlayMsg {
        key: "media.title.derivative",
        text: "Derivata come pendenza",
    },
    OverlayMsg {
        key: "media.title.pitagoras",
        text: "Teorema di Pitagora",
    },
    OverlayMsg {
        key: "media.title.taylor",
        text: "Serie di Taylor",
    },
    OverlayMsg {
        key: "media.title.conformal",
        text: "Mappa conforme",
    },
    OverlayMsg {
        key: "media.title.subspace",
        text: "Span lineare",
    },
    OverlayMsg {
        key: "media.title.fractal",
        text: "Frattale di Koch",
    },
    OverlayMsg {
        key: "media.title.default",
        text: "Animazione",
    },
    // ── panel.conformal (3) ──
    OverlayMsg {
        key: "panel.conformal.title",
        text: "Animazione di mappatura conforme",
    },
    OverlayMsg {
        key: "panel.conformal.animate",
        text: "Anima deformazione (omotopia)",
    },
    OverlayMsg {
        key: "panel.conformal.speed",
        text: "Velocità",
    },
];

/// Texto Italiano de `key`, o `None` si la clave no está en el catálogo.
/// El overlay es total (190/190): `None` solo para claves inexistentes.
pub fn it(key: &'static str) -> Option<&'static str> {
    let mut i = 0;
    while i < IT_MESSAGES.len() {
        if IT_MESSAGES[i].key == key {
            return Some(IT_MESSAGES[i].text);
        }
        i += 1;
    }
    None
}

/// Cobertura del overlay Italiano: `(cubiertas, total del catálogo)`.
pub fn it_coverage() -> (usize, usize) {
    (IT_MESSAGES.len(), MESSAGES.len())
}

/// Claves principales de UI con traducción al Français (190). Ordenado por dominio
/// como [`MESSAGES`]: grupos (18) + tools (87) + paleta (19) + onboarding (12) +
/// cheat (10) + toast (10) + app/misc (12) + anim (2) + media.title (14) +
/// panel.conformal (3).
pub static FR_MESSAGES: &[OverlayMsg] = &[
    // ── grupos (18) ──
    OverlayMsg { key: "toolbar.group.move", text: "Sélectionner" },
    OverlayMsg { key: "toolbar.group.point", text: "Points" },
    OverlayMsg { key: "toolbar.group.line", text: "Droites" },
    OverlayMsg { key: "toolbar.group.circle", text: "Cercles" },
    OverlayMsg { key: "toolbar.group.polygon", text: "Polygones" },
    OverlayMsg { key: "toolbar.group.pencil", text: "Tracé" },
    OverlayMsg { key: "toolbar.group.eraser", text: "Effacer" },
    OverlayMsg { key: "toolbar.group.conic", text: "Coniques" },
    OverlayMsg { key: "toolbar.group.curve", text: "Courbes" },
    OverlayMsg { key: "toolbar.group.measure", text: "Mesure" },
    OverlayMsg { key: "toolbar.group.analysis", text: "Analyse" },
    OverlayMsg { key: "toolbar.group.constraint", text: "Contraintes" },
    OverlayMsg { key: "toolbar.group.boolean", text: "Booléens" },
    OverlayMsg { key: "toolbar.group.threed", text: "3D" },
    OverlayMsg { key: "toolbar.group.fourd", text: "4D projetée" },
    OverlayMsg { key: "toolbar.group.advanced", text: "Avancé" },
    OverlayMsg { key: "toolbar.group.transform", text: "Transformer" },
    OverlayMsg { key: "toolbar.group.dynamics", text: "Dynamique" },
    // ── tools (87) ──
    OverlayMsg { key: "toolbar.tool.select", text: "Sélectionner" },
    OverlayMsg { key: "toolbar.tool.point", text: "Point" },
    OverlayMsg { key: "toolbar.tool.midpoint", text: "Milieu" },
    OverlayMsg { key: "toolbar.tool.line", text: "Droite" },
    OverlayMsg { key: "toolbar.tool.segment", text: "Segment" },
    OverlayMsg { key: "toolbar.tool.ray", text: "Demi-droite" },
    OverlayMsg { key: "toolbar.tool.vector", text: "Vecteur" },
    OverlayMsg { key: "toolbar.tool.perpendicular", text: "Perpendiculaire" },
    OverlayMsg { key: "toolbar.tool.circle", text: "Cercle centre-point" },
    OverlayMsg { key: "toolbar.tool.tangent", text: "Tangente" },
    OverlayMsg { key: "toolbar.tool.polygon", text: "Polygone" },
    OverlayMsg { key: "toolbar.tool.regular_polygon", text: "Polygone régulier" },
    OverlayMsg { key: "toolbar.tool.pencil", text: "Crayon" },
    OverlayMsg { key: "toolbar.tool.eraser", text: "Gomme" },
    OverlayMsg { key: "toolbar.tool.ellipse_foci", text: "Ellipse par foyers" },
    OverlayMsg { key: "toolbar.tool.parabola_focus", text: "Parabole foyer-directrice" },
    OverlayMsg { key: "toolbar.tool.hyperbola_foci", text: "Hyperbole par foyers" },
    OverlayMsg { key: "toolbar.tool.conic_five", text: "Conique par 5 points" },
    OverlayMsg { key: "toolbar.tool.function", text: "f(x) Fonction" },
    OverlayMsg { key: "toolbar.tool.param2d", text: "(x,y) Paramétrique 2D" },
    OverlayMsg { key: "toolbar.tool.polar", text: "r(t) Polaire" },
    OverlayMsg { key: "toolbar.tool.implicit", text: "F(x,y)=0 Implicite" },
    OverlayMsg { key: "toolbar.tool.field2d", text: "Champ vectoriel" },
    OverlayMsg { key: "toolbar.tool.locus", text: "Lieu géométrique" },
    OverlayMsg { key: "toolbar.tool.distance", text: "Distance" },
    OverlayMsg { key: "toolbar.tool.angle", text: "Angle" },
    OverlayMsg { key: "toolbar.tool.area", text: "Aire" },
    OverlayMsg { key: "toolbar.tool.slope", text: "m Pente" },
    OverlayMsg { key: "toolbar.tool.root", text: "Racines" },
    OverlayMsg { key: "toolbar.tool.extremum", text: "Extremums" },
    OverlayMsg { key: "toolbar.tool.inflection", text: "Inflexion" },
    OverlayMsg { key: "toolbar.tool.yintercept", text: "Intersection Y" },
    OverlayMsg { key: "toolbar.tool.xintercept", text: "Intersection X" },
    OverlayMsg { key: "toolbar.tool.intersect", text: "Intersection" },
    OverlayMsg { key: "toolbar.tool.analyze", text: "Analyser" },
    OverlayMsg { key: "toolbar.tool.coincident", text: "Coïncident" },
    OverlayMsg { key: "toolbar.tool.dist_constraint", text: "Distance" },
    OverlayMsg { key: "toolbar.tool.angle_constraint", text: "Angle" },
    OverlayMsg { key: "toolbar.tool.horizontal", text: "Horizontale" },
    OverlayMsg { key: "toolbar.tool.vertical", text: "Verticale" },
    OverlayMsg { key: "toolbar.tool.equal_length", text: "= Longueur égale" },
    OverlayMsg { key: "toolbar.tool.symmetry", text: "Symétrie" },
    OverlayMsg { key: "toolbar.tool.union", text: "Union" },
    OverlayMsg { key: "toolbar.tool.intersection", text: "Intersection" },
    OverlayMsg { key: "toolbar.tool.difference", text: "Différence" },
    OverlayMsg { key: "toolbar.tool.xor", text: "XOR" },
    OverlayMsg { key: "toolbar.tool.point3d", text: "Point 3D" },
    OverlayMsg { key: "toolbar.tool.segment3d", text: "Segment 3D" },
    OverlayMsg { key: "toolbar.tool.line3d", text: "Droite 3D" },
    OverlayMsg { key: "toolbar.tool.plane3d", text: "Plan 3D" },
    OverlayMsg { key: "toolbar.tool.sphere3d", text: "Sphère" },
    OverlayMsg { key: "toolbar.tool.cube3d", text: "Cube" },
    OverlayMsg { key: "toolbar.tool.cylinder3d", text: "Cylindre" },
    OverlayMsg { key: "toolbar.tool.cone3d", text: "Cône" },
    OverlayMsg { key: "toolbar.tool.torus3d", text: "Tore" },
    OverlayMsg { key: "toolbar.tool.moebius", text: "Ruban de Möbius" },
    OverlayMsg { key: "toolbar.tool.surface3d", text: "z Surface" },
    OverlayMsg { key: "toolbar.tool.curve3d", text: "(x,y,z) Courbe 3D" },
    OverlayMsg { key: "toolbar.tool.field3d", text: "Champ 3D" },
    OverlayMsg { key: "toolbar.tool.hypersurface4d", text: "Hypersurface 4D" },
    OverlayMsg { key: "toolbar.tool.tesseract4d", text: "Tesseract 4D : objet centré et projeté" },
    OverlayMsg { key: "toolbar.tool.hypercube5d", text: "Hypercube 5D : objet centré et projeté" },
    OverlayMsg { key: "toolbar.tool.fractal", text: "Fractale" },
    OverlayMsg { key: "toolbar.tool.histogram", text: "Histogramme" },
    OverlayMsg { key: "toolbar.tool.scatter", text: "Nuage de points" },
    OverlayMsg { key: "toolbar.tool.domain_coloring", text: "Domain Coloring" },
    OverlayMsg { key: "toolbar.tool.heatmap", text: "Heat Map" },
    OverlayMsg { key: "toolbar.tool.complex_grid", text: "Complex Grid" },
    OverlayMsg { key: "toolbar.tool.slider", text: "Curseur" },
    OverlayMsg { key: "toolbar.tool.attractor3d", text: "Attracteur 3D" },
    OverlayMsg { key: "toolbar.tool.parallel", text: "Parallèle" },
    OverlayMsg { key: "toolbar.tool.arc", text: "Arc par 3 points" },
    OverlayMsg { key: "toolbar.tool.sector", text: "Secteur circulaire" },
    OverlayMsg { key: "toolbar.tool.button", text: "Bouton" },
    OverlayMsg { key: "toolbar.tool.image", text: "Image" },
    OverlayMsg { key: "toolbar.tool.trig_animation", text: "Animation trigonométrique" },
    OverlayMsg { key: "toolbar.tool.translate", text: "Déplacer" },
    OverlayMsg { key: "toolbar.tool.rotate", text: "Rotation" },
    OverlayMsg { key: "toolbar.tool.dilate", text: "Homothétie" },
    OverlayMsg { key: "toolbar.tool.reflect", text: "Réfléchir" },
    OverlayMsg { key: "toolbar.tool.compass", text: "Compas" },
    OverlayMsg { key: "toolbar.tool.semicircle", text: "Demi-cercle" },
    OverlayMsg { key: "toolbar.tool.spline", text: "Spline" },
    OverlayMsg { key: "toolbar.tool.prism3d", text: "Prisme" },
    OverlayMsg { key: "toolbar.tool.tetrahedron3d", text: "Tétraèdre" },
    OverlayMsg { key: "toolbar.tool.checkbox", text: "Case" },
    OverlayMsg { key: "toolbar.tool.inputbox", text: "Zone de saisie" },
    // ── paleta (19) ──
    OverlayMsg { key: "palette.action.point", text: "Outil Point" },
    OverlayMsg { key: "palette.action.line", text: "Outil Droite" },
    OverlayMsg { key: "palette.action.circle", text: "Outil Circonférence" },
    OverlayMsg { key: "palette.action.polygon", text: "Outil Polygone" },
    OverlayMsg { key: "palette.action.function", text: "Outil Fonction" },
    OverlayMsg { key: "palette.action.pencil", text: "Crayon" },
    OverlayMsg { key: "palette.action.eraser", text: "Gomme" },
    OverlayMsg { key: "palette.action.save", text: "Enregistrer" },
    OverlayMsg { key: "palette.action.export_svg", text: "Exporter SVG" },
    OverlayMsg { key: "palette.action.export_png", text: "Exporter PNG" },
    OverlayMsg { key: "palette.action.export_tikz", text: "Exporter TikZ" },
    OverlayMsg { key: "palette.action.zoom_fit", text: "Ajuster la vue" },
    OverlayMsg { key: "palette.action.toggle_grid", text: "Afficher/masquer la grille" },
    OverlayMsg { key: "palette.action.toggle_dark", text: "Basculer en mode sombre" },
    OverlayMsg { key: "palette.action.indicate_selection", text: "Indiquer la sélection" },
    OverlayMsg { key: "palette.title", text: "Palette de commandes" },
    OverlayMsg { key: "palette.empty", text: "Aucune commande trouvée" },
    OverlayMsg { key: "palette.custom_tools", text: "Outils personnalisés" },
    OverlayMsg { key: "palette.footer_nav", text: "↑↓ naviguer · Entrée ouvrir · Échap fermer" },
    // ── onboarding (12) ──
    OverlayMsg { key: "onboarding.title", text: "Bienvenue dans Grafito" },
    OverlayMsg { key: "onboarding.subtitle", text: "Grafito — tableau géométrique interactif" },
    OverlayMsg { key: "onboarding.bullet_primary", text: "1. Dessinez un point et une droite" },
    OverlayMsg { key: "onboarding.bullet_secondary", text: "2. Demandez à l'assistant « trace y=x² »" },
    OverlayMsg { key: "onboarding.bullet_tertiary", text: "3. Faites glisser un point et regardez ce qui bouge" },
    OverlayMsg { key: "onboarding.bullet_university", text: "• L'université débloque 18 groupes — Coniques, 3D, CAS, Statistiques, Complexes, Dynamique…" },
    OverlayMsg { key: "onboarding.btn_example", text: "Essayer un exemple" },
    OverlayMsg { key: "onboarding.btn_empty", text: "Commencer vide" },
    OverlayMsg { key: "onboarding.btn_dismiss", text: "Ne plus afficher" },
    OverlayMsg { key: "onboarding.toast_example", text: "Exemple chargé — explorez Grafito!" },
    OverlayMsg { key: "onboarding.about_title", text: "À propos de Grafito" },
    OverlayMsg { key: "onboarding.hint", text: "Vous pouvez rouvrir cette fenêtre depuis Aide → Bienvenue" },
    // ── cheat (10) ──
    OverlayMsg { key: "cheat.title", text: "Raccourcis clavier" },
    OverlayMsg { key: "cheat.save", text: "Enregistrer : Ctrl+S" },
    OverlayMsg { key: "cheat.undo_redo", text: "Annuler / Rétablir : Ctrl+Z / Ctrl+Y" },
    OverlayMsg { key: "cheat.tools_2d", text: "Outils 2D : F1–F6" },
    OverlayMsg { key: "cheat.tools_3d", text: "3D : F8 Sphère · F9 Cube" },
    OverlayMsg { key: "cheat.pencil_eraser", text: "Crayon / Gomme : Ctrl+P / Ctrl+E" },
    OverlayMsg { key: "cheat.palette_theme", text: "Palette / Thème : Ctrl+K / Ctrl+T" },
    OverlayMsg { key: "cheat.analyze_snap", text: "Analyser / Ajuster : Ctrl+A / G" },
    OverlayMsg { key: "cheat.views", text: "Perspectives : Ctrl+Shift+1…0" },
    OverlayMsg { key: "cheat.close", text: "Annuler / Fermer : Échap" },
    // ── toast (10) ──
    OverlayMsg { key: "toast.command_done", text: "Commande terminée" },
    OverlayMsg { key: "toast.command_applied", text: "Commande appliquée dans Grafito." },
    OverlayMsg { key: "toast.saved", text: "Document enregistré dans {path}" },
    OverlayMsg { key: "toast.opened", text: "Document ouvert depuis {path}" },
    OverlayMsg { key: "toast.exported", text: "Exporté vers {path}" },
    OverlayMsg { key: "toast.save_cancelled", text: "Enregistrement annulé" },
    OverlayMsg { key: "toast.save_error", text: "Échec de l'enregistrement : {err}" },
    OverlayMsg { key: "toast.load_error", text: "Échec du chargement : {err}" },
    OverlayMsg { key: "toast.export_error", text: "Échec de l'exportation : {err}" },
    OverlayMsg { key: "toast.anim_ready", text: "Animation prête." },
    // ── app / misc (12) ──
    OverlayMsg { key: "app.menu_file", text: "Fichier" },
    OverlayMsg { key: "app.menu_edit", text: "Modifier" },
    OverlayMsg { key: "app.menu_view", text: "Affichage" },
    OverlayMsg { key: "app.menu_help", text: "Aide" },
    OverlayMsg { key: "assistant.composer_hint", text: "Écrivez votre question" },
    OverlayMsg { key: "assistant.composer_pending", text: "Je réfléchis… attendez avant d'envoyer une autre question." },
    OverlayMsg { key: "assistant.composer_empty", text: "Écrivez quelque chose pour activer Envoyer." },
    OverlayMsg { key: "assistant.composer_keys", text: "Entrée envoie · Shift+Entrée saut de ligne" },
    OverlayMsg { key: "assistant.limit_hint", text: "Caractères utilisés de la limite · Entrée envoie, Shift+Entrée saute une ligne" },
    OverlayMsg { key: "assistant.copied", text: "Message copié." },
    OverlayMsg { key: "assistant.generating", text: "Création de votre animation… ~20 s" },
    OverlayMsg { key: "assistant.teaching_started", text: "Leçon commencée : {topic}" },
    OverlayMsg { key: "panel.cas_empty", text: "Aucun résultat — exécutez une commande CAS" },
    OverlayMsg { key: "common.cancel", text: "Annuler" },
    OverlayMsg { key: "common.retry", text: "Réessayer" },
    // ── anim (2) ──
    OverlayMsg { key: "anim.empty.guide", text: "essayez de réduire la résolution ou réessayez" },
    OverlayMsg { key: "anim.empty.message", text: "{motor} n'a produit aucune image ; {guia}" },
    // ── media.title (14) ──
    OverlayMsg { key: "media.title.tangent", text: "Tangente mobile · {expr}" },
    OverlayMsg { key: "media.title.area", text: "Aire accumulée · {expr} [{p0},{p1}]" },
    OverlayMsg { key: "media.title.sweep", text: "Balayage · {expr} ({param})" },
    OverlayMsg { key: "media.title.trace", text: "Trace · {expr}" },
    OverlayMsg { key: "media.title.morph", text: "Transition" },
    OverlayMsg { key: "media.title.locus", text: "Lieu géométrique" },
    OverlayMsg { key: "media.title.integral", text: "Intégrale — aire sous la courbe" },
    OverlayMsg { key: "media.title.derivative", text: "Dérivée comme pente" },
    OverlayMsg { key: "media.title.pitagoras", text: "Théorème de Pythagore" },
    OverlayMsg { key: "media.title.taylor", text: "Série de Taylor" },
    OverlayMsg { key: "media.title.conformal", text: "Application conforme" },
    OverlayMsg { key: "media.title.subspace", text: "Vect engendré" },
    OverlayMsg { key: "media.title.fractal", text: "Fractale de Koch" },
    OverlayMsg { key: "media.title.default", text: "Animation" },
    // ── panel.conformal (3) ──
    OverlayMsg { key: "panel.conformal.title", text: "Animation de mapping conforme" },
    OverlayMsg { key: "panel.conformal.animate", text: "Animer la déformation (homotopie)" },
    OverlayMsg { key: "panel.conformal.speed", text: "Vitesse" },
];

/// Texto Français de `key`, o `None` si la clave no está en el catálogo.
/// El overlay es total (190/190): `None` solo para claves inexistentes.
pub fn fr(key: &'static str) -> Option<&'static str> {
    let mut i = 0;
    while i < FR_MESSAGES.len() {
        if FR_MESSAGES[i].key == key {
            return Some(FR_MESSAGES[i].text);
        }
        i += 1;
    }
    None
}

/// Cobertura del overlay Français: `(cubiertas, total del catálogo)`.
pub fn fr_coverage() -> (usize, usize) {
    (FR_MESSAGES.len(), MESSAGES.len())
}

/// Claves principales de UI con traducción al Deutsch (190). Ordenado por dominio
/// como [`MESSAGES`]: grupos (18) + tools (87) + paleta (19) + onboarding (12) +
/// cheat (10) + toast (10) + app/misc (12) + anim (2) + media.title (14) +
/// panel.conformal (3).
pub static DE_MESSAGES: &[OverlayMsg] = &[
    // ── grupos (18) ──
    OverlayMsg { key: "toolbar.group.move", text: "Auswählen" },
    OverlayMsg { key: "toolbar.group.point", text: "Punkte" },
    OverlayMsg { key: "toolbar.group.line", text: "Geraden" },
    OverlayMsg { key: "toolbar.group.circle", text: "Kreise" },
    OverlayMsg { key: "toolbar.group.polygon", text: "Polygone" },
    OverlayMsg { key: "toolbar.group.pencil", text: "Strich" },
    OverlayMsg { key: "toolbar.group.eraser", text: "Löschen" },
    OverlayMsg { key: "toolbar.group.conic", text: "Kegelschnitte" },
    OverlayMsg { key: "toolbar.group.curve", text: "Kurven" },
    OverlayMsg { key: "toolbar.group.measure", text: "Messung" },
    OverlayMsg { key: "toolbar.group.analysis", text: "Analyse" },
    OverlayMsg { key: "toolbar.group.constraint", text: "Bedingungen" },
    OverlayMsg { key: "toolbar.group.boolean", text: "Boolesch" },
    OverlayMsg { key: "toolbar.group.threed", text: "3D" },
    OverlayMsg { key: "toolbar.group.fourd", text: "Projiziertes 4D" },
    OverlayMsg { key: "toolbar.group.advanced", text: "Erweitert" },
    OverlayMsg { key: "toolbar.group.transform", text: "Transformieren" },
    OverlayMsg { key: "toolbar.group.dynamics", text: "Dynamik" },
    // ── tools (87) ──
    OverlayMsg { key: "toolbar.tool.select", text: "Auswählen" },
    OverlayMsg { key: "toolbar.tool.point", text: "Punkt" },
    OverlayMsg { key: "toolbar.tool.midpoint", text: "Mittelpunkt" },
    OverlayMsg { key: "toolbar.tool.line", text: "Gerade" },
    OverlayMsg { key: "toolbar.tool.segment", text: "Strecke" },
    OverlayMsg { key: "toolbar.tool.ray", text: "Strahl" },
    OverlayMsg { key: "toolbar.tool.vector", text: "Vektor" },
    OverlayMsg { key: "toolbar.tool.perpendicular", text: "Senkrechte" },
    OverlayMsg { key: "toolbar.tool.circle", text: "Kreis Mittelpunkt-Punkt" },
    OverlayMsg { key: "toolbar.tool.tangent", text: "Tangente" },
    OverlayMsg { key: "toolbar.tool.polygon", text: "Polygon" },
    OverlayMsg { key: "toolbar.tool.regular_polygon", text: "Regelmäßiges Polygon" },
    OverlayMsg { key: "toolbar.tool.pencil", text: "Stift" },
    OverlayMsg { key: "toolbar.tool.eraser", text: "Radierer" },
    OverlayMsg { key: "toolbar.tool.ellipse_foci", text: "Ellipse durch Brennpunkte" },
    OverlayMsg { key: "toolbar.tool.parabola_focus", text: "Parabel Brennpunkt-Leitlinie" },
    OverlayMsg { key: "toolbar.tool.hyperbola_foci", text: "Hyperbel durch Brennpunkte" },
    OverlayMsg { key: "toolbar.tool.conic_five", text: "Kegelschnitt durch 5 Punkte" },
    OverlayMsg { key: "toolbar.tool.function", text: "f(x) Funktion" },
    OverlayMsg { key: "toolbar.tool.param2d", text: "(x,y) 2D-parametrisch" },
    OverlayMsg { key: "toolbar.tool.polar", text: "r(t) Polar" },
    OverlayMsg { key: "toolbar.tool.implicit", text: "F(x,y)=0 Implizit" },
    OverlayMsg { key: "toolbar.tool.field2d", text: "Vektorfeld" },
    OverlayMsg { key: "toolbar.tool.locus", text: "Ortskurve" },
    OverlayMsg { key: "toolbar.tool.distance", text: "Abstand" },
    OverlayMsg { key: "toolbar.tool.angle", text: "Winkel" },
    OverlayMsg { key: "toolbar.tool.area", text: "Fläche" },
    OverlayMsg { key: "toolbar.tool.slope", text: "m Steigung" },
    OverlayMsg { key: "toolbar.tool.root", text: "Nullstellen" },
    OverlayMsg { key: "toolbar.tool.extremum", text: "Extrema" },
    OverlayMsg { key: "toolbar.tool.inflection", text: "Wendepunkt" },
    OverlayMsg { key: "toolbar.tool.yintercept", text: "Y-Achsenabschnitt" },
    OverlayMsg { key: "toolbar.tool.xintercept", text: "X-Achsenabschnitt" },
    OverlayMsg { key: "toolbar.tool.intersect", text: "Schnittpunkt" },
    OverlayMsg { key: "toolbar.tool.analyze", text: "Analysieren" },
    OverlayMsg { key: "toolbar.tool.coincident", text: "Zusammenfallend" },
    OverlayMsg { key: "toolbar.tool.dist_constraint", text: "Abstand" },
    OverlayMsg { key: "toolbar.tool.angle_constraint", text: "Winkel" },
    OverlayMsg { key: "toolbar.tool.horizontal", text: "Horizontal" },
    OverlayMsg { key: "toolbar.tool.vertical", text: "Vertikal" },
    OverlayMsg { key: "toolbar.tool.equal_length", text: "= Gleiche Länge" },
    OverlayMsg { key: "toolbar.tool.symmetry", text: "Symmetrie" },
    OverlayMsg { key: "toolbar.tool.union", text: "Vereinigung" },
    OverlayMsg { key: "toolbar.tool.intersection", text: "Schnittmenge" },
    OverlayMsg { key: "toolbar.tool.difference", text: "Differenz" },
    OverlayMsg { key: "toolbar.tool.xor", text: "XOR" },
    OverlayMsg { key: "toolbar.tool.point3d", text: "3D-Punkt" },
    OverlayMsg { key: "toolbar.tool.segment3d", text: "3D-Strecke" },
    OverlayMsg { key: "toolbar.tool.line3d", text: "3D-Gerade" },
    OverlayMsg { key: "toolbar.tool.plane3d", text: "3D-Ebene" },
    OverlayMsg { key: "toolbar.tool.sphere3d", text: "Kugel" },
    OverlayMsg { key: "toolbar.tool.cube3d", text: "Würfel" },
    OverlayMsg { key: "toolbar.tool.cylinder3d", text: "Zylinder" },
    OverlayMsg { key: "toolbar.tool.cone3d", text: "Kegel" },
    OverlayMsg { key: "toolbar.tool.torus3d", text: "Torus" },
    OverlayMsg { key: "toolbar.tool.moebius", text: "Möbiusband" },
    OverlayMsg { key: "toolbar.tool.surface3d", text: "z Fläche" },
    OverlayMsg { key: "toolbar.tool.curve3d", text: "(x,y,z) 3D-Kurve" },
    OverlayMsg { key: "toolbar.tool.field3d", text: "3D-Feld" },
    OverlayMsg { key: "toolbar.tool.hypersurface4d", text: "4D-Hyperfläche" },
    OverlayMsg { key: "toolbar.tool.tesseract4d", text: "4D-Tesserakt: zentriertes projiziertes Objekt" },
    OverlayMsg { key: "toolbar.tool.hypercube5d", text: "5D-Hyperwürfel: zentriertes projiziertes Objekt" },
    OverlayMsg { key: "toolbar.tool.fractal", text: "Fraktal" },
    OverlayMsg { key: "toolbar.tool.histogram", text: "Histogramm" },
    OverlayMsg { key: "toolbar.tool.scatter", text: "Streudiagramm" },
    OverlayMsg { key: "toolbar.tool.domain_coloring", text: "Domain Coloring" },
    OverlayMsg { key: "toolbar.tool.heatmap", text: "Heat Map" },
    OverlayMsg { key: "toolbar.tool.complex_grid", text: "Complex Grid" },
    OverlayMsg { key: "toolbar.tool.slider", text: "Schieberegler" },
    OverlayMsg { key: "toolbar.tool.attractor3d", text: "3D-Attraktor" },
    OverlayMsg { key: "toolbar.tool.parallel", text: "Parallele" },
    OverlayMsg { key: "toolbar.tool.arc", text: "3-Punkt-Bogen" },
    OverlayMsg { key: "toolbar.tool.sector", text: "Kreissektor" },
    OverlayMsg { key: "toolbar.tool.button", text: "Schaltfläche" },
    OverlayMsg { key: "toolbar.tool.image", text: "Bild" },
    OverlayMsg { key: "toolbar.tool.trig_animation", text: "Trigonometrische Animation" },
    OverlayMsg { key: "toolbar.tool.translate", text: "Verschieben" },
    OverlayMsg { key: "toolbar.tool.rotate", text: "Drehen" },
    OverlayMsg { key: "toolbar.tool.dilate", text: "Zentrische Streckung" },
    OverlayMsg { key: "toolbar.tool.reflect", text: "Spiegeln" },
    OverlayMsg { key: "toolbar.tool.compass", text: "Zirkel" },
    OverlayMsg { key: "toolbar.tool.semicircle", text: "Halbkreis" },
    OverlayMsg { key: "toolbar.tool.spline", text: "Spline" },
    OverlayMsg { key: "toolbar.tool.prism3d", text: "Prisma" },
    OverlayMsg { key: "toolbar.tool.tetrahedron3d", text: "Tetraeder" },
    OverlayMsg { key: "toolbar.tool.checkbox", text: "Kontrollkästchen" },
    OverlayMsg { key: "toolbar.tool.inputbox", text: "Eingabefeld" },
    // ── paleta (19) ──
    OverlayMsg { key: "palette.action.point", text: "Punkt-Werkzeug" },
    OverlayMsg { key: "palette.action.line", text: "Geraden-Werkzeug" },
    OverlayMsg { key: "palette.action.circle", text: "Kreis-Werkzeug" },
    OverlayMsg { key: "palette.action.polygon", text: "Polygon-Werkzeug" },
    OverlayMsg { key: "palette.action.function", text: "Funktions-Werkzeug" },
    OverlayMsg { key: "palette.action.pencil", text: "Stift" },
    OverlayMsg { key: "palette.action.eraser", text: "Radierer" },
    OverlayMsg { key: "palette.action.save", text: "Speichern" },
    OverlayMsg { key: "palette.action.export_svg", text: "SVG exportieren" },
    OverlayMsg { key: "palette.action.export_png", text: "PNG exportieren" },
    OverlayMsg { key: "palette.action.export_tikz", text: "TikZ exportieren" },
    OverlayMsg { key: "palette.action.zoom_fit", text: "Alles einpassen" },
    OverlayMsg { key: "palette.action.toggle_grid", text: "Raster umschalten" },
    OverlayMsg { key: "palette.action.toggle_dark", text: "Dunkelmodus umschalten" },
    OverlayMsg { key: "palette.action.indicate_selection", text: "Auswahl anzeigen" },
    OverlayMsg { key: "palette.title", text: "Befehlspalette" },
    OverlayMsg { key: "palette.empty", text: "Keine Befehle gefunden" },
    OverlayMsg { key: "palette.custom_tools", text: "Benutzerdefinierte Werkzeuge" },
    OverlayMsg { key: "palette.footer_nav", text: "↑↓ navigieren · Enter öffnen · Esc schließen" },
    // ── onboarding (12) ──
    OverlayMsg { key: "onboarding.title", text: "Willkommen bei Grafito" },
    OverlayMsg { key: "onboarding.subtitle", text: "Grafito — interaktives Geometrie-Board" },
    OverlayMsg { key: "onboarding.bullet_primary", text: "1. Zeichne einen Punkt und eine Gerade" },
    OverlayMsg { key: "onboarding.bullet_secondary", text: "2. Bitte den Assistenten „zeichne y=x²“" },
    OverlayMsg { key: "onboarding.bullet_tertiary", text: "3. Ziehe einen Punkt und beobachte, was sich bewegt" },
    OverlayMsg { key: "onboarding.bullet_university", text: "• Universität schaltet 18 Gruppen frei — Kegelschnitte, 3D, CAS, Statistik, Komplexes, Dynamik…" },
    OverlayMsg { key: "onboarding.btn_example", text: "Beispiel ausprobieren" },
    OverlayMsg { key: "onboarding.btn_empty", text: "Leer starten" },
    OverlayMsg { key: "onboarding.btn_dismiss", text: "Nicht mehr anzeigen" },
    OverlayMsg { key: "onboarding.toast_example", text: "Beispiel geladen — erkunde Grafito!" },
    OverlayMsg { key: "onboarding.about_title", text: "Über Grafito" },
    OverlayMsg { key: "onboarding.hint", text: "Du kannst dieses Fenster über Hilfe → Willkommen erneut öffnen" },
    // ── cheat (10) ──
    OverlayMsg { key: "cheat.title", text: "Tastaturkürzel" },
    OverlayMsg { key: "cheat.save", text: "Speichern: Strg+S" },
    OverlayMsg { key: "cheat.undo_redo", text: "Rückgängig / Wiederholen: Strg+Z / Strg+Y" },
    OverlayMsg { key: "cheat.tools_2d", text: "2D-Werkzeuge: F1–F6" },
    OverlayMsg { key: "cheat.tools_3d", text: "3D: F8 Kugel · F9 Würfel" },
    OverlayMsg { key: "cheat.pencil_eraser", text: "Stift / Radierer: Strg+P / Strg+E" },
    OverlayMsg { key: "cheat.palette_theme", text: "Palette / Design: Strg+K / Strg+T" },
    OverlayMsg { key: "cheat.analyze_snap", text: "Analysieren / Fangen: Strg+A / G" },
    OverlayMsg { key: "cheat.views", text: "Ansichten: Strg+Shift+1…0" },
    OverlayMsg { key: "cheat.close", text: "Abbrechen / Schließen: Esc" },
    // ── toast (10) ──
    OverlayMsg { key: "toast.command_done", text: "Befehl abgeschlossen" },
    OverlayMsg { key: "toast.command_applied", text: "Befehl in Grafito angewendet." },
    OverlayMsg { key: "toast.saved", text: "Dokument gespeichert unter {path}" },
    OverlayMsg { key: "toast.opened", text: "Dokument geöffnet von {path}" },
    OverlayMsg { key: "toast.exported", text: "Exportiert nach {path}" },
    OverlayMsg { key: "toast.save_cancelled", text: "Speichern abgebrochen" },
    OverlayMsg { key: "toast.save_error", text: "Fehler beim Speichern: {err}" },
    OverlayMsg { key: "toast.load_error", text: "Fehler beim Laden: {err}" },
    OverlayMsg { key: "toast.export_error", text: "Fehler beim Exportieren: {err}" },
    OverlayMsg { key: "toast.anim_ready", text: "Animation bereit." },
    // ── app / misc (12) ──
    OverlayMsg { key: "app.menu_file", text: "Datei" },
    OverlayMsg { key: "app.menu_edit", text: "Bearbeiten" },
    OverlayMsg { key: "app.menu_view", text: "Ansicht" },
    OverlayMsg { key: "app.menu_help", text: "Hilfe" },
    OverlayMsg { key: "assistant.composer_hint", text: "Schreib deine Frage" },
    OverlayMsg { key: "assistant.composer_pending", text: "Denke nach… warte, bevor du eine weitere Frage sendest." },
    OverlayMsg { key: "assistant.composer_empty", text: "Schreib etwas, um Senden zu aktivieren." },
    OverlayMsg { key: "assistant.composer_keys", text: "Enter sendet · Shift+Enter Zeilenumbruch" },
    OverlayMsg { key: "assistant.limit_hint", text: "Verwendete Zeichen des Limits · Enter sendet, Shift+Enter Zeilenumbruch" },
    OverlayMsg { key: "assistant.copied", text: "Nachricht kopiert." },
    OverlayMsg { key: "assistant.generating", text: "Erstelle deine Animation… ~20 s" },
    OverlayMsg { key: "assistant.teaching_started", text: "Unterricht gestartet: {topic}" },
    OverlayMsg { key: "panel.cas_empty", text: "Kein Ergebnis — führe einen CAS-Befehl aus" },
    OverlayMsg { key: "common.cancel", text: "Abbrechen" },
    OverlayMsg { key: "common.retry", text: "Wiederholen" },
    // ── anim (2) ──
    OverlayMsg { key: "anim.empty.guide", text: "versuche, die Auflösung zu senken oder wiederhole" },
    OverlayMsg { key: "anim.empty.message", text: "{motor} hat keine Frames erzeugt; {guia}" },
    // ── media.title (14) ──
    OverlayMsg { key: "media.title.tangent", text: "Bewegliche Tangente · {expr}" },
    OverlayMsg { key: "media.title.area", text: "Kumulierte Fläche · {expr} [{p0},{p1}]" },
    OverlayMsg { key: "media.title.sweep", text: "Durchlauf · {expr} ({param})" },
    OverlayMsg { key: "media.title.trace", text: "Spur · {expr}" },
    OverlayMsg { key: "media.title.morph", text: "Übergang" },
    OverlayMsg { key: "media.title.locus", text: "Ortskurve" },
    OverlayMsg { key: "media.title.integral", text: "Integral — Fläche unter der Kurve" },
    OverlayMsg { key: "media.title.derivative", text: "Ableitung als Steigung" },
    OverlayMsg { key: "media.title.pitagoras", text: "Satz des Pythagoras" },
    OverlayMsg { key: "media.title.taylor", text: "Taylorreihe" },
    OverlayMsg { key: "media.title.conformal", text: "Konforme Abbildung" },
    OverlayMsg { key: "media.title.subspace", text: "Lineare Hülle" },
    OverlayMsg { key: "media.title.fractal", text: "Koch-Fraktal" },
    OverlayMsg { key: "media.title.default", text: "Animation" },
    // ── panel.conformal (3) ──
    OverlayMsg { key: "panel.conformal.title", text: "Konforme Abbildungsanimation" },
    OverlayMsg { key: "panel.conformal.animate", text: "Deformation animieren (Homotopie)" },
    OverlayMsg { key: "panel.conformal.speed", text: "Geschwindigkeit" },
];

/// Texto Deutsch de `key`, o `None` si la clave no está en el catálogo.
/// El overlay es total (190/190): `None` solo para claves inexistentes.
pub fn de(key: &'static str) -> Option<&'static str> {
    let mut i = 0;
    while i < DE_MESSAGES.len() {
        if DE_MESSAGES[i].key == key {
            return Some(DE_MESSAGES[i].text);
        }
        i += 1;
    }
    None
}

/// Cobertura del overlay Deutsch: `(cubiertas, total del catálogo)`.
pub fn de_coverage() -> (usize, usize) {
    (DE_MESSAGES.len(), MESSAGES.len())
}
// ── Números (display + parse tolerante) ──

/// Formatea un número sólo para mostrar (nunca para persistir ni calcular).
///
/// - ES/PT/IT/FR/DE: coma decimal (`3,14`); EN: punto (`3.14`). Sin separador de miles.
/// - `-0.0` se muestra como `"0"`.
/// - `NaN` → `"NaN"`; `+∞` → `"∞"`; `-∞` → `"-∞"` (igual en todas las lenguas).
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
        Locale::Es | Locale::Pt | Locale::It | Locale::Fr | Locale::De => plain.replace('.', ","),
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
        anim_msg, cheat_sheet_msg, de, de_coverage, format_number, fr, fr_coverage, group_label,
        it, it_coverage, media_title_msg, onboarding_msg, palette_action, palette_footer,
        parse_number_tolerant, pt, pt_coverage, pt_is_partial, pt_partial_badge_text, toast_msg,
        tool_label, Locale, OverlayMsg, ANIM_KEYS, CHEAT_KEYS, DE_MESSAGES, FR_MESSAGES,
        GROUP_SLUGS, IT_MESSAGES, MEDIA_TITLE_KEYS, MESSAGES, MSG_COUNT, ONBOARDING_KEYS,
        PT_MESSAGES, PT_PARTIAL_BADGE, TOAST_KEYS,
    };

    #[test]
    fn msg_count_matches_table() {
        assert_eq!(
            MESSAGES.len(),
            MSG_COUNT,
            "MSG_COUNT debe seguir a MESSAGES"
        );
        assert_eq!(MSG_COUNT, 190);
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
    fn composer_status_keys_resuelven_en_seis_idiomas() {
        // Ola 3: el composer deja de ser solo-ES en el catálogo (el cableado
        // del locale al composer es P2; las claves ya resuelven en 6 idiomas).
        for key in [
            "assistant.composer_pending",
            "assistant.composer_empty",
            "assistant.composer_keys",
        ] {
            for locale in [
                Locale::Es,
                Locale::En,
                Locale::Pt,
                Locale::It,
                Locale::Fr,
                Locale::De,
            ] {
                let text = super::t(key, locale);
                assert!(!text.is_empty(), "{key} vacío en {locale:?}");
                assert_ne!(text, key, "{key} sin resolver en {locale:?}");
            }
        }
        assert_eq!(
            super::t("assistant.composer_empty", Locale::Es),
            "Escribí algo para activar Enviar."
        );
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
        assert_eq!(super::t("toolbar.tool.circle", es), "Círculo centro-punto");
        assert_eq!(super::t("toolbar.tool.parallel", es), "Paralela");
        assert_eq!(super::t("toolbar.tool.arc", es), "Arco 3 puntos");
        assert_eq!(super::t("toolbar.tool.sector", es), "Sector circular");
        assert_eq!(super::t("toolbar.tool.button", es), "Botón");
        assert_eq!(super::t("toolbar.tool.image", es), "Imagen");
        assert_eq!(
            super::t("toolbar.tool.trig_animation", es),
            "Animación trigonométrica"
        );
        assert_eq!(super::t("toolbar.tool.pencil", es), "Lápiz");
        assert_eq!(super::t("palette.action.point", es), "Herramienta Punto");
        assert_eq!(super::t("palette.empty", es), "No se encontraron comandos");
        assert_eq!(super::t("onboarding.title", es), "Bienvenido a Grafito");
        assert_eq!(super::t("onboarding.btn_example", es), "Probar ejemplo");
        assert_eq!(super::t("onboarding.btn_empty", es), "Empezar vacío");
        assert_eq!(
            super::t("onboarding.btn_dismiss", es),
            "No mostrar de nuevo"
        );
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
        assert_eq!(ONBOARDING_KEYS.len(), 12);
        assert_eq!(CHEAT_KEYS.len(), 10);
        assert_eq!(TOAST_KEYS.len(), 10);
        assert_eq!(MEDIA_TITLE_KEYS.len(), 14);
        assert_eq!(ANIM_KEYS.len(), 2);
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
        for k in MEDIA_TITLE_KEYS {
            assert!(!media_title_msg(k, Locale::Es).is_empty());
            assert!(!media_title_msg(k, Locale::En).is_empty());
        }
        for k in ANIM_KEYS {
            assert!(!anim_msg(k, Locale::Es).is_empty());
            assert!(!anim_msg(k, Locale::En).is_empty());
        }
        assert_eq!(onboarding_msg("btn_example", Locale::Es), "Probar ejemplo");
        assert_eq!(cheat_sheet_msg("save", Locale::En), "Save: Ctrl+S");
        assert_eq!(toast_msg("anim_ready", Locale::Es), "Animación lista.");
        assert_eq!(
            media_title_msg("integral", Locale::Es),
            "Integral — área bajo la curva"
        );
        assert_eq!(media_title_msg("morph", Locale::En), "Transition");
        assert_eq!(
            anim_msg("guide", Locale::Es),
            "probá bajar la resolución o reintentá"
        );
    }

    #[test]
    fn pt_covers_main_ui_keys() {
        // R3.4: overlay total PT — 190 claves, sin duplicados ni vacíos,
        // cada una existente en el catálogo ES/EN.
        assert_eq!(PT_MESSAGES.len(), 190);
        assert_eq!(pt_coverage(), (190, 190));
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
        // Paleta completa: 15 acciones + título + vacío + pie.
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
            "palette.action.indicate_selection",
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
        // R3.4: el recorte F3d/W2 está cerrado — tools, conformal y
        // bullet_tertiary también resuelven directo en PT.
        for key in [
            "toolbar.tool.point",
            "toolbar.tool.translate",
            "toolbar.tool.circle",
            "panel.conformal.title",
            "panel.conformal.animate",
            "panel.conformal.speed",
            "onboarding.bullet_tertiary",
        ] {
            assert!(pt(key).is_some(), "R3.4 sin PT: {key}");
        }
        assert_eq!(pt("toolbar.tool.point"), Some("Ponto"));
        assert_eq!(pt("toolbar.tool.translate"), Some("Translada"));
        assert_eq!(
            pt("panel.conformal.title"),
            Some("Animação de Mapeamento Conforme")
        );
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
            ("anim.empty.guide", "resolução"),
            ("anim.empty.message", "{motor}"),
            ("anim.empty.message", "{guia}"),
            ("media.title.tangent", "{expr}"),
            ("media.title.area", "{expr}"),
            ("media.title.area", "{p0}"),
            ("media.title.area", "{p1}"),
            ("media.title.sweep", "{param}"),
            ("media.title.trace", "{expr}"),
        ] {
            let text = pt(key).expect("clave principal con PT");
            assert!(
                text.contains(marker),
                "{key} PT debe contener {marker}: {text}"
            );
        }
    }

    #[test]
    fn pt_coverage_media() {
        // Overlay puntual (auditoría): `media/title.*` + `anim/empty.*` tienen
        // PT sin pedir fluent completo. Si se agrega una clave `media.*` o
        // `anim.*` al catálogo, este test exige su PT acá también.
        for m in MESSAGES {
            if m.key.starts_with("media.title.") || m.key.starts_with("anim.empty.") {
                assert!(pt(m.key).is_some(), "sin overlay PT puntual: {}", m.key);
            }
        }
        assert_eq!(
            pt("media.title.integral"),
            Some("Integral — área sob a curva")
        );
        assert_eq!(pt("media.title.default"), Some("Animação"));
        assert_eq!(pt("media.title.subspace"), Some("Span linear"));
        assert_eq!(pt("media.title.fractal"), Some("Fractal de Koch"));
        assert_eq!(
            pt("anim.empty.guide"),
            Some("tente reduzir a resolução ou tentar de novo")
        );
        // R3.4: `panel.conformal.*` ya está en el overlay total: `pt()` da
        // `Some` directo en PT, jamás vacío ni fallback.
        assert_eq!(
            pt("panel.conformal.title"),
            Some("Animação de Mapeamento Conforme")
        );
        assert_eq!(
            super::t("panel.conformal.title", Locale::Pt),
            "Animação de Mapeamento Conforme"
        );
    }

    #[test]
    fn pt_reports_tool_fallback() {
        // R3.4: el recorte F3d/W2 está cerrado — las 87 `toolbar.tool` tienen
        // PT directo (antes 0 a propósito con fallback ES).
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
            tool_covered, 87,
            "tools en PT: R3.4 cierra el recorte al 100%"
        );
        assert_eq!(pt("toolbar.tool.translate"), Some("Translada"));
        assert_eq!(tool_label("translate", Locale::En), "Translate");
    }

    #[test]
    fn pt_locale_resolves_overlay_then_spanish_then_key() {
        use super::t;
        // Con PT directo donde hay overlay.
        assert_eq!(t("toolbar.group.move", Locale::Pt), "Selecionar");
        assert_eq!(t("palette.title", Locale::Pt), "Paleta de Comandos");
        assert_eq!(t("toast.saved", Locale::Pt), "Documento salvo em {path}");
        // R3.4: tools con PT directo (antes fallback ES), jamás vacío.
        assert_eq!(t("toolbar.tool.translate", Locale::Pt), "Translada");
        assert_eq!(t("toolbar.tool.circle", Locale::Pt), "Círculo centro-ponto");
        assert!(!t("toolbar.tool.translate", Locale::Pt).is_empty());
        // Clave inexistente: la propia clave (igual que ES/EN).
        assert_eq!(t("does.not.exist", Locale::Pt), "does.not.exist");
        // Helpers por dominio fluyen a PT.
        assert_eq!(group_label("move", Locale::Pt), "Selecionar");
        assert_eq!(palette_action("save", Locale::Pt), "Salvar");
        assert_eq!(onboarding_msg("btn_example", Locale::Pt), "Testar exemplo");
        assert_eq!(cheat_sheet_msg("save", Locale::Pt), "Salvar: Ctrl+S");
        assert_eq!(toast_msg("anim_ready", Locale::Pt), "Animação pronta.");
        // Pie y números en PT (coma como ES).
        assert_eq!(
            palette_footer(3, 213, Locale::Pt),
            "3 de 213 · ↑↓ navegar · Enter abrir · Esc fechar"
        );
        assert_eq!(format_number(3.25, Locale::Pt), "3,25");
        assert_eq!(Locale::Pt.code(), "pt");
        assert_eq!(Locale::default(), Locale::Es);
    }

    #[test]
    fn pt_coverage_prints_real_percentage() {
        // Cobertura PT medida: 190/190 = 100%. Se imprime el % real con
        // `--nocapture`; el assert fija el numerador para que cualquier
        // agregado (o faltante) de PT rompa el test a propósito.
        let (covered, total) = pt_coverage();
        assert_eq!((covered, total), (190, 190));
        let pct = covered as f64 * 100.0 / total as f64;
        eprintln!("cobertura PT: {covered}/{total} = {pct:.1}% (overlay total R3.4)");
        assert!((pct - 100.0).abs() < 0.1, "pct real: {pct}");
    }

    #[test]
    fn pt_partial_badge_pinned() {
        // R3.4: cobertura 100% — el badge parcial ya no se muestra (ver
        // `toolbar.rs`: solo dibuja si `pt_is_partial()`). Se pinnea el 100%
        // y el texto con conteo para el hover histórico.
        assert!(!pt_is_partial(), "R3.4 190/190 = 100%: sin badge parcial");
        assert_eq!(PT_PARTIAL_BADGE, "Português parcial");
        assert_eq!(pt_partial_badge_text(), "Português parcial · 190/190");
        let (covered, total) = pt_coverage();
        assert_eq!((covered, total), (190, 190));
    }

    // ── Overlays IT/FR/DE (190/190 c/u, texto tal cual de la tabla) ──

    /// Aserciones comunes de overlay total: 190 entradas, cobertura 190/190,
    /// sin duplicados ni vacíos, claves dentro del catálogo y en su mismo orden.
    fn assert_overlay_total(
        table: &[OverlayMsg],
        lookup: fn(&'static str) -> Option<&'static str>,
        coverage: fn() -> (usize, usize),
        tag: &str,
    ) {
        assert_eq!(table.len(), 190, "{tag}: overlay total");
        assert_eq!(coverage(), (190, 190), "{tag}: cobertura total");
        let mut keys: Vec<&str> = table.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        let mut i = 1;
        while i < keys.len() {
            assert_ne!(keys[i - 1], keys[i], "{tag}: clave duplicada: {}", keys[i]);
            i += 1;
        }
        for entry in table {
            assert!(!entry.key.is_empty(), "{tag}: clave vacía");
            assert!(
                !entry.text.is_empty(),
                "{tag}: traducción vacía en {}",
                entry.key
            );
            assert!(
                MESSAGES.iter().any(|m| m.key == entry.key),
                "{tag}: clave fuera del catálogo: {}",
                entry.key
            );
        }
        // Mismo orden que el catálogo: el overlay sigue a MESSAGES.
        let mut j = 0;
        while j < MESSAGES.len() {
            assert_eq!(table[j].key, MESSAGES[j].key, "{tag}: orden en {j}");
            j += 1;
        }
        assert!(
            lookup("does.not.exist").is_none(),
            "{tag}: clave inexistente da None"
        );
    }

    #[test]
    fn it_covers_main_ui_keys() {
        assert_overlay_total(IT_MESSAGES, it, it_coverage, "IT");
        assert_eq!(it("toolbar.group.move"), Some("Seleziona"));
        assert_eq!(it("palette.title"), Some("Tavolozza comandi"));
        assert_eq!(it("toolbar.tool.translate"), Some("Trasla"));
        assert_eq!(
            it("panel.conformal.title"),
            Some("Animazione di mappatura conforme")
        );
        assert_eq!(
            it("media.title.integral"),
            Some("Integrale — area sotto la curva")
        );
        assert_eq!(super::t("toolbar.group.move", Locale::It), "Seleziona");
        assert_eq!(
            super::t("toast.saved", Locale::It),
            "Documento salvato in {path}"
        );
    }

    #[test]
    fn fr_covers_main_ui_keys() {
        assert_overlay_total(FR_MESSAGES, fr, fr_coverage, "FR");
        assert_eq!(fr("toolbar.group.move"), Some("Sélectionner"));
        assert_eq!(fr("palette.title"), Some("Palette de commandes"));
        assert_eq!(fr("toolbar.tool.translate"), Some("Déplacer"));
        assert_eq!(
            fr("panel.conformal.title"),
            Some("Animation de mapping conforme")
        );
        assert_eq!(
            fr("media.title.integral"),
            Some("Intégrale — aire sous la courbe")
        );
        assert_eq!(super::t("toolbar.group.move", Locale::Fr), "Sélectionner");
        assert_eq!(
            super::t("toast.saved", Locale::Fr),
            "Document enregistré dans {path}"
        );
    }

    #[test]
    fn de_covers_main_ui_keys() {
        assert_overlay_total(DE_MESSAGES, de, de_coverage, "DE");
        assert_eq!(de("toolbar.group.move"), Some("Auswählen"));
        assert_eq!(de("palette.title"), Some("Befehlspalette"));
        assert_eq!(de("toolbar.tool.translate"), Some("Verschieben"));
        assert_eq!(
            de("panel.conformal.title"),
            Some("Konforme Abbildungsanimation")
        );
        assert_eq!(
            de("media.title.integral"),
            Some("Integral — Fläche unter der Kurve")
        );
        assert_eq!(super::t("toolbar.group.move", Locale::De), "Auswählen");
        assert_eq!(
            super::t("toast.saved", Locale::De),
            "Dokument gespeichert unter {path}"
        );
    }

    #[test]
    fn overlay_placeholders_preserved_it_fr_de() {
        // Para cada clave ES/EN con `{x}`, las 3 traducciones contienen `{x}`.
        let markers = [
            "{path}", "{err}", "{topic}", "{motor}", "{guia}", "{expr}", "{p0}", "{p1}", "{param}",
        ];
        for m in MESSAGES {
            for marker in markers {
                if m.es.contains(marker) || m.en.contains(marker) {
                    for (tag, found) in [("IT", it(m.key)), ("FR", fr(m.key)), ("DE", de(m.key))] {
                        let text = found.expect("overlay total: clave del catálogo");
                        assert!(
                            text.contains(marker),
                            "{tag} {} debe contener {marker}: {text}",
                            m.key
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn t_never_empty_for_any_locale() {
        // Ninguna clave×idioma devuelve vacío (overlays totales + ES/EN completos).
        let locales = [
            Locale::Es,
            Locale::En,
            Locale::Pt,
            Locale::It,
            Locale::Fr,
            Locale::De,
        ];
        for m in MESSAGES {
            for locale in locales {
                assert!(
                    !super::t(m.key, locale).is_empty(),
                    "t vacío en {} × {:?}",
                    m.key,
                    locale
                );
            }
        }
    }

    #[test]
    fn locale_code_roundtrip_it_fr_de() {
        assert_eq!(Locale::It.code(), "it");
        assert_eq!(Locale::Fr.code(), "fr");
        assert_eq!(Locale::De.code(), "de");
        // Pie localizado por idioma (conector propio + navegación del overlay).
        assert_eq!(
            palette_footer(3, 213, Locale::It),
            "3 di 213 · ↑↓ naviga · Invio apri · Esc chiudi"
        );
        assert_eq!(
            palette_footer(3, 213, Locale::Fr),
            "3 sur 213 · ↑↓ naviguer · Entrée ouvrir · Échap fermer"
        );
        assert_eq!(
            palette_footer(3, 213, Locale::De),
            "3 von 213 · ↑↓ navigieren · Enter öffnen · Esc schließen"
        );
        // Coma decimal como ES/PT (solo EN usa punto).
        assert_eq!(format_number(3.25, Locale::It), "3,25");
        assert_eq!(format_number(3.25, Locale::Fr), "3,25");
        assert_eq!(format_number(3.25, Locale::De), "3,25");
        assert_eq!(format_number(3.25, Locale::En), "3.25");
        // Helpers por dominio fluyen a los 3 idiomas.
        assert_eq!(group_label("move", Locale::It), "Seleziona");
        assert_eq!(group_label("move", Locale::Fr), "Sélectionner");
        assert_eq!(group_label("move", Locale::De), "Auswählen");
        assert_eq!(tool_label("translate", Locale::De), "Verschieben");
        assert_eq!(
            palette_action("save", Locale::It),
            it("palette.action.save").expect("overlay total")
        );
        assert_eq!(
            toast_msg("anim_ready", Locale::Fr),
            fr("toast.anim_ready").expect("overlay total")
        );
        assert_eq!(
            media_title_msg("taylor", Locale::De),
            de("media.title.taylor").expect("overlay total")
        );
    }
}
