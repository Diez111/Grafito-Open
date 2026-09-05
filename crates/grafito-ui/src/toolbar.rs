//! Horizontal toolbar with tool groups, each with dropdown.
//! Icons are drawn with egui::Painter — no Unicode dependency.
//! Pattern: one icon per group (last used tool), ASCII fallback opens sub-menu.

use egui::{pos2, vec2, Color32, Painter, Rect, Shape, Stroke, Ui};
use std::f32::consts::TAU;

use crate::animation::interpolate_color;
use crate::i18n::{group_label, tool_label, Locale};
use crate::icons::{draw_icon, Icon};
use crate::theme::current_theme;
use crate::tokens::{BREAKPOINT_COMPACT, RADIUS_MD};
use crate::Tool;

/// Una entrada de la toolbar: `(Tool, etiqueta, atajo)`.
pub type ToolEntry = (Tool, &'static str, &'static str);

const TOOL_MENU_PREFERRED_WIDTH: f32 = 220.0;
const TOOL_MENU_ITEM_HEIGHT: f32 = 30.0;
const TOOL_MENU_SCREEN_MARGIN: f32 = 16.0;
const TOOL_MENU_VERTICAL_RESERVE: f32 = 72.0;
pub const COMPACT_TOOLBAR_MAX_WIDTH: f32 = BREAKPOINT_COMPACT;
/// Lado reservado para cada selector de grupo de herramientas.
pub const TOOLBAR_BUTTON_SIZE: f32 = 36.0;
/// Espacio vertical que rodea la única fila de herramientas.
pub const TOOLBAR_VERTICAL_PADDING: f32 = 4.0;
/// Altura total del chrome de herramientas, incluidos sus márgenes.
pub const TOOLBAR_PANEL_HEIGHT: f32 = TOOLBAR_BUTTON_SIZE + 2.0 * TOOLBAR_VERTICAL_PADDING;

const GROUP_MOVE: &[ToolEntry] = &[(Tool::Select, "Seleccionar", "F1")];

const GROUP_POINT: &[ToolEntry] = &[
    (Tool::Point, "Punto", "F2"),
    (Tool::Midpoint, "M Punto medio", ""),
];

const GROUP_LINE: &[ToolEntry] = &[
    (Tool::Line, "Recta", "F3"),
    (Tool::Segment, "Segmento", ""),
    (Tool::Ray, "Semirrecta", ""),
    (Tool::Vector, "Vector", ""),
    (Tool::Perpendicular, "Perpendicular", ""),
    (Tool::Parallel, "Paralela", ""),
];

const GROUP_CIRCLE: &[ToolEntry] = &[
    (Tool::Circle, "Circulo centro-punto", "F4"),
    (Tool::Tangent, "Tangente", ""),
    (Tool::Arc, "Arco 3 puntos", ""),
    (Tool::Sector, "Sector circular", ""),
];

const GROUP_POLYGON: &[ToolEntry] = &[
    (Tool::Polygon, "Poligono", "F5"),
    (Tool::RegularPolygon, "Poligono regular", ""),
];

const GROUP_PENCIL: &[ToolEntry] = &[(Tool::Pencil, "Lapiz", "Ctrl+P")];

const GROUP_ERASER: &[ToolEntry] = &[(Tool::Eraser, "Borrador", "Ctrl+E")];

const GROUP_CONIC: &[ToolEntry] = &[
    (Tool::EllipseByFoci, "Elipse por focos", ""),
    (
        Tool::ParabolaByFocusDirectrix,
        "Parabola foco-directriz",
        "",
    ),
    (Tool::HyperbolaByFoci, "Hiperbola por focos", ""),
    (Tool::ConicByFivePoints, "Conica por 5 puntos", ""),
];

const GROUP_CURVE: &[ToolEntry] = &[
    (Tool::Function, "f(x) Función", "F6"),
    (Tool::ParametricCurve2D, "(x,y) Paramétrica 2D", ""),
    (Tool::PolarCurve, "r(t) Polar", ""),
    (Tool::ImplicitCurve, "F(x,y)=0 Implícita", ""),
    (Tool::VectorField2D, "Campo vectorial", ""),
    (Tool::Locus, "Lugar geométrico", ""),
];

const GROUP_MEASURE: &[ToolEntry] = &[
    (Tool::Distance, "Distancia", ""),
    (Tool::Angle, "Angulo", ""),
    (Tool::Area, "Area", ""),
    (Tool::Slope, "m Pendiente", ""),
];

const GROUP_ANALYSIS: &[ToolEntry] = &[
    (Tool::Root, "Raices", ""),
    (Tool::Extremum, "Extremos", ""),
    (Tool::Inflection, "Inflexion", ""),
    (Tool::YIntercept, "Interseccion Y", ""),
    (Tool::XIntercept, "Interseccion X", ""),
    (Tool::Intersect, "Interseccion", ""),
    (Tool::Analyze, "Analizar", ""),
];

// Nota dedup: `Tool::Locus` vive solo en GROUP_CURVE (canónico para las
// perspectivas analíticas) — no duplicar aquí; cada Tool un solo grupo.
const GROUP_CONSTRAINT: &[ToolEntry] = &[
    (Tool::Coincident, "Coincidente", ""),
    (Tool::DistanceConstraint, "Distancia", ""),
    (Tool::AngleConstraint, "Angulo", ""),
    (Tool::Horizontal, "Horizontal", ""),
    (Tool::Vertical, "Vertical", ""),
    (Tool::EqualLength, "= Igual longitud", ""),
    (Tool::Symmetry, "Simetria", ""),
];

const GROUP_BOOLEAN: &[ToolEntry] = &[
    (Tool::PolygonUnion, "Union", ""),
    (Tool::PolygonIntersection, "Interseccion", ""),
    (Tool::PolygonDifference, "Diferencia", ""),
    (Tool::PolygonXor, "XOR", ""),
];

const GROUP_3D: &[ToolEntry] = &[
    (Tool::Point3D, "Punto 3D", ""),
    (Tool::Segment3D, "Segmento 3D", ""),
    (Tool::Line3D, "Recta 3D", ""),
    (Tool::Plane3D, "Plano 3D", ""),
    (Tool::Sphere3D, "Esfera", "F8"),
    (Tool::Cube3D, "Cubo", "F9"),
    (Tool::Cylinder3D, "Cilindro", ""),
    (Tool::Cone3D, "Cono", ""),
    (Tool::Torus3D, "Toro", ""),
    (Tool::MoebiusStrip, "Mobius", ""),
    (Tool::Surface3D, "z Superficie", ""),
    (Tool::ParametricCurve3D, "(x,y,z) Curva 3D", ""),
    (Tool::VectorField3D, "Campo 3D", ""),
    (Tool::HyperSurface4D, "4D Hipersuperficie", ""),
];

const GROUP_4D: &[ToolEntry] = &[
    (
        Tool::Tesseract4D,
        "Teseracto 4D: objeto centrado y proyectado",
        "",
    ),
    (
        Tool::Hypercube5D,
        "Hipercubo 5D: objeto centrado y proyectado",
        "",
    ),
];

// Nota dedup: `Tool::Attractor` vive solo en GROUP_DYNAMICS ("Atractor 3D") —
// no duplicar aquí; cada Tool un solo grupo.
const GROUP_ADVANCED: &[ToolEntry] = &[
    (Tool::Fractal, "Fractal", ""),
    (Tool::Histogram, "Histograma", ""),
    (Tool::ScatterPlot, "Dispersion", ""),
    (Tool::DomainColoring, "Domain Coloring", ""),
    (Tool::HeatMap, "Heat Map", ""),
    (Tool::ComplexGrid, "Complex Grid", ""),
    (Tool::Slider, "Deslizador", ""),
];

const GROUP_DYNAMICS: &[ToolEntry] = &[(Tool::Attractor, "Atractor 3D", "")];

/// Identificador de un grupo de herramientas de la toolbar.
///
/// Cada variante resuelve su icono vectorial y su lista estática de
/// [`ToolEntry`] mediante [`ToolGroupId::def`]. Esto permite a las
/// perspectivas referenciar grupos de forma compacta (`&'static [ToolGroupId]`)
/// sin perder la asociación grupo↔icono y sin asignaciones en tiempo de
/// ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGroupId {
    Move,
    Point,
    Line,
    Circle,
    Polygon,
    Pencil,
    Eraser,
    Conic,
    Curve,
    Measure,
    Analysis,
    Constraint,
    Boolean,
    ThreeD,
    FourD,
    Advanced,
    Dynamics,
}

impl ToolGroupId {
    /// Devuelve el icono y la lista de herramientas del grupo.
    pub const fn def(self) -> (IconFn, &'static [ToolEntry]) {
        match self {
            ToolGroupId::Move => (icon_move, GROUP_MOVE),
            ToolGroupId::Point => (icon_point, GROUP_POINT),
            ToolGroupId::Line => (icon_line, GROUP_LINE),
            ToolGroupId::Circle => (icon_circle, GROUP_CIRCLE),
            ToolGroupId::Polygon => (icon_polygon, GROUP_POLYGON),
            ToolGroupId::Pencil => (icon_pencil, GROUP_PENCIL),
            ToolGroupId::Eraser => (icon_eraser, GROUP_ERASER),
            ToolGroupId::Conic => (icon_conic, GROUP_CONIC),
            ToolGroupId::Curve => (icon_curve, GROUP_CURVE),
            ToolGroupId::Measure => (icon_measure, GROUP_MEASURE),
            ToolGroupId::Analysis => (icon_analysis, GROUP_ANALYSIS),
            ToolGroupId::Constraint => (icon_constraint, GROUP_CONSTRAINT),
            ToolGroupId::Boolean => (icon_boolean, GROUP_BOOLEAN),
            ToolGroupId::ThreeD => (icon_3d, GROUP_3D),
            ToolGroupId::FourD => (icon_four_d, GROUP_4D),
            ToolGroupId::Advanced => (icon_advanced, GROUP_ADVANCED),
            ToolGroupId::Dynamics => (icon_dynamics, GROUP_DYNAMICS),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            ToolGroupId::Move => "Seleccionar",
            ToolGroupId::Point => "Puntos",
            ToolGroupId::Line => "Rectas",
            ToolGroupId::Circle => "Círculos",
            ToolGroupId::Polygon => "Polígonos",
            ToolGroupId::Pencil => "Trazo",
            ToolGroupId::Eraser => "Borrar",
            ToolGroupId::Conic => "Cónicas",
            ToolGroupId::Curve => "Curvas",
            ToolGroupId::Measure => "Medición",
            ToolGroupId::Analysis => "Análisis",
            ToolGroupId::Constraint => "Restricciones",
            ToolGroupId::Boolean => "Booleanas",
            ToolGroupId::ThreeD => "3D",
            ToolGroupId::FourD => "4D proyectado",
            ToolGroupId::Advanced => "Avanzado",
            ToolGroupId::Dynamics => "Dinámica",
        }
    }
}

impl ToolGroupId {
    /// Slug estable del grupo (`"move"`, …, `"dynamics"`) para [`group_label`].
    pub const fn slug(self) -> &'static str {
        match self {
            ToolGroupId::Move => "move",
            ToolGroupId::Point => "point",
            ToolGroupId::Line => "line",
            ToolGroupId::Circle => "circle",
            ToolGroupId::Polygon => "polygon",
            ToolGroupId::Pencil => "pencil",
            ToolGroupId::Eraser => "eraser",
            ToolGroupId::Conic => "conic",
            ToolGroupId::Curve => "curve",
            ToolGroupId::Measure => "measure",
            ToolGroupId::Analysis => "analysis",
            ToolGroupId::Constraint => "constraint",
            ToolGroupId::Boolean => "boolean",
            ToolGroupId::ThreeD => "threed",
            ToolGroupId::FourD => "fourd",
            ToolGroupId::Advanced => "advanced",
            ToolGroupId::Dynamics => "dynamics",
        }
    }

    /// Etiqueta del grupo en el idioma pedido. ES idéntica a [`ToolGroupId::label`].
    pub fn label_localized(self, locale: Locale) -> &'static str {
        group_label(self.slug(), locale)
    }
}

/// Todos los grupos en el orden clásico de la toolbar (sin `ThreeD`).
pub const ALL_GROUPS: &[ToolGroupId] = &[
    ToolGroupId::Move,
    ToolGroupId::Point,
    ToolGroupId::Line,
    ToolGroupId::Circle,
    ToolGroupId::Polygon,
    ToolGroupId::Pencil,
    ToolGroupId::Eraser,
    ToolGroupId::Conic,
    ToolGroupId::Curve,
    ToolGroupId::Measure,
    ToolGroupId::Analysis,
    ToolGroupId::Constraint,
    ToolGroupId::Boolean,
    ToolGroupId::Advanced,
];

// ── Progressive disclosure F5 Scandinavian — filtrado por nivel pedagógico ──
// Sin laberinto: Primary muestra solo lo esencial, Secondary añade medición/análisis,
// University expone todo. Los slices son `&'static` para evitar asignaciones.
// Se proveen helpers tanto tipados (PedagogicalLevel) como ligeros (u32) para
// no acoplar toolbar a pedagogy si hiciera falta (feature/udl.rs).

/// Límite superior inclusivo de `level_value` para Primary (5 grupos).
pub const TOOLBAR_LEVEL_PRIMARY_MAX: u32 = 4;
/// Límite superior inclusivo de `level_value` para Secondary (8 grupos).
pub const TOOLBAR_LEVEL_SECONDARY_MAX: u32 = 10;

/// Primary (level_value `0..=TOOLBAR_LEVEL_PRIMARY_MAX`): 5 grupos esenciales — Mover, Punto, Recta, Círculo, Polígono.
pub const PRIMARY_TOOL_GROUPS: &[ToolGroupId] = &[
    ToolGroupId::Move,
    ToolGroupId::Point,
    ToolGroupId::Line,
    ToolGroupId::Circle,
    ToolGroupId::Polygon,
];

/// Secondary (`TOOLBAR_LEVEL_PRIMARY_MAX+1..=TOOLBAR_LEVEL_SECONDARY_MAX`): Primary + Lápiz, Medición, Análisis = 8 grupos.
pub const SECONDARY_TOOL_GROUPS: &[ToolGroupId] = &[
    ToolGroupId::Move,
    ToolGroupId::Point,
    ToolGroupId::Line,
    ToolGroupId::Circle,
    ToolGroupId::Polygon,
    ToolGroupId::Pencil,
    ToolGroupId::Measure,
    ToolGroupId::Analysis,
];

/// University (`>TOOLBAR_LEVEL_SECONDARY_MAX`): todos los grupos — Secondary + Constraint, Boolean, Advanced, Dynamics, ThreeD/FourD, Eraser, Conic, Curve.
pub const UNIVERSITY_TOOL_GROUPS: &[ToolGroupId] = &[
    ToolGroupId::Move,
    ToolGroupId::Point,
    ToolGroupId::Line,
    ToolGroupId::Circle,
    ToolGroupId::Polygon,
    ToolGroupId::Pencil,
    ToolGroupId::Eraser,
    ToolGroupId::Conic,
    ToolGroupId::Curve,
    ToolGroupId::Measure,
    ToolGroupId::Analysis,
    ToolGroupId::Constraint,
    ToolGroupId::Boolean,
    ToolGroupId::Advanced,
    ToolGroupId::Dynamics,
    ToolGroupId::ThreeD,
    ToolGroupId::FourD,
];

/// Helper ligero sin dependencia de `grafito-pedagogy`: filtra por `level_value` (u32).
/// `0..=PRIMARY_MAX` → Primary (5), `PRIMARY_MAX+1..=SECONDARY_MAX` → Secondary (8), `>SECONDARY_MAX` → University.
/// No rompe API — devuelve `&'static` sin asignación.
pub fn toolbar_groups_for_level_value(level_value: u32) -> &'static [ToolGroupId] {
    if level_value <= TOOLBAR_LEVEL_PRIMARY_MAX {
        PRIMARY_TOOL_GROUPS
    } else if level_value <= TOOLBAR_LEVEL_SECONDARY_MAX {
        SECONDARY_TOOL_GROUPS
    } else {
        UNIVERSITY_TOOL_GROUPS
    }
}

/// Vec-owned variant for callers that expect `Vec<ToolGroupId>` (spec compat).
pub fn toolbar_groups_for_level_value_owned(level_value: u32) -> Vec<ToolGroupId> {
    toolbar_groups_for_level_value(level_value).to_vec()
}

/// Helper tipado: filtra por `PedagogicalLevel` (usa `level_value()` internamente).
/// Requiere `grafito-pedagogy`; delega al helper ligero para mantener single source.
pub fn toolbar_groups_for_level(
    level: grafito_pedagogy::PedagogicalLevel,
) -> &'static [ToolGroupId] {
    toolbar_groups_for_level_value(level.level_value())
}

/// Alias Vec para `PedagogicalLevel`.
pub fn toolbar_groups_for_level_owned(
    level: grafito_pedagogy::PedagogicalLevel,
) -> Vec<ToolGroupId> {
    toolbar_groups_for_level(level).to_vec()
}

/// Filtra una lista arbitraria de grupos por nivel (intersección con el set permitido).
/// Útil para `PerspectiveLayout::visible_tool_groups` + progressive disclosure.
pub fn filter_groups_by_level(groups: &[ToolGroupId], level_value: u32) -> Vec<ToolGroupId> {
    let allowed = toolbar_groups_for_level_value(level_value);
    groups
        .iter()
        .copied()
        .filter(|g| allowed.contains(g))
        .collect()
}

/// Versión tipada de `filter_groups_by_level`.
pub fn filter_groups_by_pedagogical_level(
    groups: &[ToolGroupId],
    level: grafito_pedagogy::PedagogicalLevel,
) -> Vec<ToolGroupId> {
    filter_groups_by_level(groups, level.level_value())
}

/// Slug estable de cada [`Tool`] para [`tool_label`] (76 variantes).
///
/// El `match` es exhaustivo a propósito (sin comodín): añadir una variante a
/// [`Tool`] rompe la compilación hasta darle su slug en el catálogo i18n.
pub fn tool_slug(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "select",
        Tool::Point => "point",
        Tool::Midpoint => "midpoint",
        Tool::Line => "line",
        Tool::Segment => "segment",
        Tool::Ray => "ray",
        Tool::Vector => "vector",
        Tool::Perpendicular => "perpendicular",
        Tool::Parallel => "parallel",
        Tool::Circle => "circle",
        Tool::Tangent => "tangent",
        Tool::Arc => "arc",
        Tool::Sector => "sector",
        Tool::Polygon => "polygon",
        Tool::RegularPolygon => "regular_polygon",
        Tool::Pencil => "pencil",
        Tool::Eraser => "eraser",
        Tool::EllipseByFoci => "ellipse_foci",
        Tool::ParabolaByFocusDirectrix => "parabola_focus",
        Tool::HyperbolaByFoci => "hyperbola_foci",
        Tool::ConicByFivePoints => "conic_five",
        Tool::Function => "function",
        Tool::ParametricCurve2D => "param2d",
        Tool::PolarCurve => "polar",
        Tool::ImplicitCurve => "implicit",
        Tool::VectorField2D => "field2d",
        Tool::Locus => "locus",
        Tool::Distance => "distance",
        Tool::Angle => "angle",
        Tool::Area => "area",
        Tool::Slope => "slope",
        Tool::Root => "root",
        Tool::Extremum => "extremum",
        Tool::Inflection => "inflection",
        Tool::YIntercept => "yintercept",
        Tool::XIntercept => "xintercept",
        Tool::Intersect => "intersect",
        Tool::Analyze => "analyze",
        Tool::Coincident => "coincident",
        Tool::DistanceConstraint => "dist_constraint",
        Tool::AngleConstraint => "angle_constraint",
        Tool::Horizontal => "horizontal",
        Tool::Vertical => "vertical",
        Tool::EqualLength => "equal_length",
        Tool::Symmetry => "symmetry",
        Tool::PolygonUnion => "union",
        Tool::PolygonIntersection => "intersection",
        Tool::PolygonDifference => "difference",
        Tool::PolygonXor => "xor",
        Tool::Point3D => "point3d",
        Tool::Segment3D => "segment3d",
        Tool::Line3D => "line3d",
        Tool::Plane3D => "plane3d",
        Tool::Sphere3D => "sphere3d",
        Tool::Cube3D => "cube3d",
        Tool::Cylinder3D => "cylinder3d",
        Tool::Cone3D => "cone3d",
        Tool::Torus3D => "torus3d",
        Tool::MoebiusStrip => "moebius",
        Tool::Surface3D => "surface3d",
        Tool::ParametricCurve3D => "curve3d",
        Tool::VectorField3D => "field3d",
        Tool::HyperSurface4D => "hypersurface4d",
        Tool::Tesseract4D => "tesseract4d",
        Tool::Hypercube5D => "hypercube5d",
        Tool::Fractal => "fractal",
        Tool::Histogram => "histogram",
        Tool::ScatterPlot => "scatter",
        Tool::DomainColoring => "domain_coloring",
        Tool::HeatMap => "heatmap",
        Tool::ComplexGrid => "complex_grid",
        Tool::Slider => "slider",
        Tool::Button => "button",
        Tool::Image => "image",
        Tool::TrigAnimation => "trig_animation",
        Tool::Attractor => "attractor3d",
    }
}

/// Las 76 variantes de [`Tool`] en orden estable: prueba que cada una tiene
/// slug y etiqueta ES/EN no vacía (ver test `all_76_tools_resolve_both_locales`).
pub const ALL_TOOLS: &[Tool; 76] = &[
    Tool::Select,
    Tool::Point,
    Tool::Midpoint,
    Tool::Line,
    Tool::Segment,
    Tool::Ray,
    Tool::Vector,
    Tool::Perpendicular,
    Tool::Parallel,
    Tool::Circle,
    Tool::Tangent,
    Tool::Arc,
    Tool::Sector,
    Tool::Polygon,
    Tool::RegularPolygon,
    Tool::Pencil,
    Tool::Eraser,
    Tool::EllipseByFoci,
    Tool::ParabolaByFocusDirectrix,
    Tool::HyperbolaByFoci,
    Tool::ConicByFivePoints,
    Tool::Function,
    Tool::ParametricCurve2D,
    Tool::PolarCurve,
    Tool::ImplicitCurve,
    Tool::VectorField2D,
    Tool::Locus,
    Tool::Distance,
    Tool::Angle,
    Tool::Area,
    Tool::Slope,
    Tool::Root,
    Tool::Extremum,
    Tool::Inflection,
    Tool::YIntercept,
    Tool::XIntercept,
    Tool::Intersect,
    Tool::Analyze,
    Tool::Coincident,
    Tool::DistanceConstraint,
    Tool::AngleConstraint,
    Tool::Horizontal,
    Tool::Vertical,
    Tool::EqualLength,
    Tool::Symmetry,
    Tool::PolygonUnion,
    Tool::PolygonIntersection,
    Tool::PolygonDifference,
    Tool::PolygonXor,
    Tool::Point3D,
    Tool::Segment3D,
    Tool::Line3D,
    Tool::Plane3D,
    Tool::Sphere3D,
    Tool::Cube3D,
    Tool::Cylinder3D,
    Tool::Cone3D,
    Tool::Torus3D,
    Tool::MoebiusStrip,
    Tool::Surface3D,
    Tool::ParametricCurve3D,
    Tool::VectorField3D,
    Tool::HyperSurface4D,
    Tool::Tesseract4D,
    Tool::Hypercube5D,
    Tool::Fractal,
    Tool::Histogram,
    Tool::ScatterPlot,
    Tool::DomainColoring,
    Tool::HeatMap,
    Tool::ComplexGrid,
    Tool::Slider,
    Tool::Button,
    Tool::Image,
    Tool::TrigAnimation,
    Tool::Attractor,
];

/// Nombre visible de una herramienta en el idioma pedido.
///
/// ES idéntico a la etiqueta estática de `GROUP_*`; si un slug no resolviera
/// (no ocurre: ver test), se conserva la etiqueta estática como fallback
/// visible en lugar de mostrar vacío.
fn entry_display_name(tool: Tool, fallback: &'static str, locale: Locale) -> &'static str {
    let labeled = tool_label(tool_slug(tool), locale);
    if labeled.is_empty() {
        fallback
    } else {
        labeled
    }
}

// ── A11Y resto (F10-B): foco visible + live-region, render puro sin I/O ──

/// Live-region polite para lectores de pantalla, sin widgets nuevos.
///
/// Etiqueta una respuesta YA existente (`WidgetInfo` → backend AccessKit).
/// No crea `Area`s ni reservas: se verificó que un `Area` flotante rompe el
/// hover/clic de `ui.interact` en egui 0.29 (tests de toasts). Render puro,
/// sin I/O ni spawn. Límite honesto: egui 0.29 no expone rol ARIA-live; esto
/// es lo mejor disponible sin subir egui (P2).
pub fn tag_live_region(response: &egui::Response, text: String) {
    if text.is_empty() {
        return;
    }
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text.clone())
    });
}

/// Texto polite de la herramienta actual. Puro: deriva de `Tool`, sin I/O.
pub fn toolbar_live_text(current: Tool, locale: Locale) -> String {
    let name = tool_label(tool_slug(current), locale);
    let name = if name.is_empty() {
        tool_slug(current)
    } else {
        name
    };
    match locale {
        Locale::Es => format!("Herramienta: {name}"),
        Locale::En => format!("Tool: {name}"),
    }
}

/// Enter/Espacio activan el control enfocado por teclado (paridad con clic).
fn key_activates_focused(ui: &Ui, response: &egui::Response) -> bool {
    response.has_focus()
        && ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        })
}

// ── Vector icon drawing functions ──

fn icon_move(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.38;
    let pts = vec![
        c + vec2(-s, -s),
        c + vec2(-s * 0.2, s * 0.8),
        c + vec2(-s * 0.1, s * 0.2),
        c + vec2(s * 0.8, s * 0.3),
        c + vec2(-s, -s),
    ];
    painter.add(Shape::line(pts, Stroke::new(2.0, color)));
}

fn icon_point(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.18;
    painter.circle_filled(c, r.max(2.5), color);
    let mark = rect.width() * 0.32;
    let dim = color.gamma_multiply(0.4);
    painter.line_segment(
        [c - vec2(mark, 0.0), c + vec2(mark, 0.0)],
        Stroke::new(1.0, dim),
    );
    painter.line_segment(
        [c - vec2(0.0, mark), c + vec2(0.0, mark)],
        Stroke::new(1.0, dim),
    );
}

fn icon_line(painter: &Painter, rect: Rect, color: Color32) {
    let m = rect.width() * 0.22;
    let a = rect.min + vec2(m, m * 3.2);
    let b = rect.max - vec2(m * 3.2, m);
    painter.line_segment([a, b], Stroke::new(2.0, color));
    painter.circle_filled(a, 2.2, color);
    painter.circle_filled(b, 2.2, color);
}

fn icon_segment(painter: &Painter, rect: Rect, color: Color32) {
    let m = rect.width() * 0.2;
    let a = rect.min + vec2(m, rect.height() * 0.75);
    let b = rect.max - vec2(m, rect.height() * 0.75);
    let sw = Stroke::new(2.0, color);
    painter.line_segment([a, b], sw);
    painter.circle_filled(a, 2.5, color);
    painter.circle_filled(b, 2.5, color);
}

fn icon_ray(painter: &Painter, rect: Rect, color: Color32) {
    let m = rect.width() * 0.2;
    let a = rect.min + vec2(m, rect.height() * 0.75);
    let b = rect.max - vec2(m, rect.height() * 0.75);
    let sw = Stroke::new(2.0, color);
    painter.line_segment([a, b], sw);
    painter.circle_filled(a, 2.5, color);
    let dir = (b - a).normalized();
    let perp = vec2(-dir.y, dir.x);
    painter.line_segment([b, b - dir * 7.0 + perp * 4.0], sw);
    painter.line_segment([b, b - dir * 7.0 - perp * 4.0], sw);
}

fn icon_vector(painter: &Painter, rect: Rect, color: Color32) {
    let m = rect.width() * 0.2;
    let a = rect.min + vec2(m, rect.height() * 0.75);
    let b = rect.max - vec2(m, rect.height() * 0.75);
    let sw = Stroke::new(2.2, color);
    painter.line_segment([a, b], sw);
    let dir = (b - a).normalized();
    let perp = vec2(-dir.y, dir.x);
    painter.line_segment([b, b - dir * 8.0 + perp * 4.5], sw);
    painter.line_segment([b, b - dir * 8.0 - perp * 4.5], sw);
}

fn icon_midpoint(painter: &Painter, rect: Rect, color: Color32) {
    icon_segment(painter, rect, color.gamma_multiply(0.7));
    painter.circle_filled(rect.center(), 3.5, color);
}

fn icon_perpendicular(painter: &Painter, rect: Rect, color: Color32) {
    let sw = Stroke::new(2.0, color);
    let c = rect.center();
    let h = rect.width() * 0.34;
    painter.line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], sw);
    painter.line_segment([c, c - vec2(0.0, h)], sw);
    let s = rect.width() * 0.12;
    painter.line_segment([c + vec2(s, 0.0), c + vec2(s, -s)], Stroke::new(1.4, color));
    painter.line_segment(
        [c + vec2(0.0, -s), c + vec2(s, -s)],
        Stroke::new(1.4, color),
    );
}

fn icon_tangent(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.28;
    let sw = Stroke::new(1.8, color);
    painter.circle_stroke(c, r, sw);
    painter.line_segment(
        [c + vec2(-r * 0.95, -r), c + vec2(r * 0.95, -r)],
        Stroke::new(2.0, color),
    );
    painter.circle_filled(c + vec2(0.0, -r), 2.2, color);
}

fn icon_circle(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.36;
    painter.circle_stroke(c, r, Stroke::new(2.0, color));
    painter.circle_filled(c, 2.5, color);
}

fn icon_polygon(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.35;
    let p1 = c + vec2(0.0, -s);
    let p2 = c + vec2(-s * 0.87, s * 0.5);
    let p3 = c + vec2(s * 0.87, s * 0.5);
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3],
        Color32::TRANSPARENT,
        Stroke::new(2.0, color),
    ));
    painter.circle_filled(p1, 2.0, color);
    painter.circle_filled(p2, 2.0, color);
    painter.circle_filled(p3, 2.0, color);
}

fn icon_conic(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let rx = rect.width() * 0.36;
    let ry = rect.width() * 0.22;
    let n = 16;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        pts.push(c + vec2(rx * a.cos(), ry * a.sin()));
    }
    for i in 0..n {
        painter.line_segment([pts[i], pts[(i + 1) % n]], Stroke::new(1.8, color));
    }
    painter.circle_filled(c, 2.0, color);
}

fn icon_curve(painter: &Painter, rect: Rect, color: Color32) {
    let n = 22;
    let w = rect.width() * 0.78;
    let h = rect.height() * 0.44;
    let sx = rect.center().x - w * 0.5;
    let sy = rect.center().y;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        pts.push(pos2(sx + t * w, sy + (t * TAU).sin() * h * 0.7));
    }
    painter.add(Shape::line(pts, Stroke::new(2.0, color)));
}

fn icon_measure(painter: &Painter, rect: Rect, color: Color32) {
    let y = rect.center().y;
    let x0 = rect.min.x + rect.width() * 0.16;
    let x1 = rect.max.x - rect.width() * 0.16;
    painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.0, color));
    for i in 0..4 {
        let x = x0 + (i as f32 / 3.0) * (x1 - x0);
        painter.line_segment(
            [pos2(x, y - 5.0), pos2(x, y + 5.0)],
            Stroke::new(1.0, color),
        );
    }
}

fn icon_3d(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.28;
    let ftl = c + vec2(-s, -s * 0.5);
    let ftr = c + vec2(s * 0.5, -s);
    let fbr = c + vec2(s * 0.5, s * 0.3);
    let fbl = c + vec2(-s, s * 0.8);
    let btl = ftl + vec2(-s * 0.45, -s * 0.45);
    let btr = ftr + vec2(-s * 0.45, -s * 0.45);
    let sw = Stroke::new(1.5, color);
    painter.line_segment([ftl, ftr], sw);
    painter.line_segment([ftr, fbr], sw);
    painter.line_segment([fbr, fbl], sw);
    painter.line_segment([fbl, ftl], sw);
    painter.line_segment([ftl, btl], sw);
    painter.line_segment([ftr, btr], sw);
    painter.line_segment([btl, btr], sw);
}

fn icon_tesseract_4d(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.27;
    let outer = [
        c + vec2(-s, -s),
        c + vec2(s, -s),
        c + vec2(s, s),
        c + vec2(-s, s),
    ];
    let inner = [
        c + vec2(-s * 0.62, -s * 0.46),
        c + vec2(s * 0.62, -s * 0.46),
        c + vec2(s * 0.62, s * 0.78),
        c + vec2(-s * 0.62, s * 0.78),
    ];
    let stroke = Stroke::new(1.5, color);
    for index in 0..4 {
        painter.line_segment([outer[index], outer[(index + 1) % 4]], stroke);
        painter.line_segment([inner[index], inner[(index + 1) % 4]], stroke);
        painter.line_segment([outer[index], inner[index]], stroke);
    }
}

fn icon_hypercube_5d(painter: &Painter, rect: Rect, color: Color32) {
    icon_tesseract_4d(painter, rect, color.gamma_multiply(0.8));
    let c = rect.center();
    let r = rect.width() * 0.1;
    painter.circle_stroke(c, r, Stroke::new(1.5, color));
    painter.line_segment(
        [c - vec2(r * 1.5, 0.0), c + vec2(r * 1.5, 0.0)],
        Stroke::new(1.2, color),
    );
    painter.line_segment(
        [c - vec2(0.0, r * 1.5), c + vec2(0.0, r * 1.5)],
        Stroke::new(1.2, color),
    );
}

fn icon_four_d(painter: &Painter, rect: Rect, color: Color32) {
    icon_tesseract_4d(painter, rect, color);
}

fn icon_plane(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let sw = Stroke::new(1.8, color);
    let p1 = c + vec2(-9.0, 5.0);
    let p2 = c + vec2(-1.0, -7.0);
    let p3 = c + vec2(10.0, -3.0);
    let p4 = c + vec2(2.0, 9.0);
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3, p4],
        Color32::TRANSPARENT,
        sw,
    ));
}

fn icon_surface(painter: &Painter, rect: Rect, color: Color32) {
    let sw = Stroke::new(1.6, color);
    let n = 16;
    for row in 0..3 {
        let mut pts = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let x = rect.min.x + rect.width() * t;
            let y = rect.center().y + (row as f32 - 1.0) * 5.0 + (t * TAU).sin() * 3.0;
            pts.push(pos2(x, y));
        }
        painter.add(Shape::line(pts, sw));
    }
}

fn icon_dynamics(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let sw = Stroke::new(1.5, color);
    let n = 48;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32 * TAU * 2.0;
        let r = rect.width() * (0.08 + 0.26 * i as f32 / n as f32);
        pts.push(c + vec2(r * t.cos(), r * t.sin()));
    }
    painter.add(Shape::line(pts, sw));
    painter.circle_filled(c, 2.4, color);
}

fn icon_advanced(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.28;
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU;
        painter.line_segment(
            [
                c - vec2(r * a.cos(), r * a.sin()),
                c + vec2(r * a.cos(), r * a.sin()),
            ],
            Stroke::new(1.5, color),
        );
    }
    painter.circle_filled(c, 2.8, color);
    painter.circle_stroke(c, r, Stroke::new(1.5, color));
}

fn icon_pencil(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let sw = Stroke::new(1.8, color);
    let tip = c + vec2(-7.0, 7.0);
    let b1 = c + vec2(-4.0, 4.0);
    let b2 = c + vec2(7.0, -7.0);
    let b3 = c + vec2(9.0, -5.0);
    let b4 = c + vec2(-2.0, 6.0);
    painter.line_segment([b1, b2], sw);
    painter.line_segment([b2, b3], sw);
    painter.line_segment([b3, b4], sw);
    painter.line_segment([b4, b1], sw);
    painter.line_segment([b1, tip], sw);
    painter.line_segment([b4, tip], sw);
}

fn icon_eraser(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let sw = Stroke::new(1.8, color);
    let body_a = c + vec2(-7.0, 6.0);
    let body_b = c + vec2(5.0, -6.0);
    let body_c = c + vec2(8.0, -3.0);
    let body_d = c + vec2(-4.0, 9.0);
    painter.line_segment([body_a, body_b], sw);
    painter.line_segment([body_b, body_c], sw);
    painter.line_segment([body_c, body_d], sw);
    painter.line_segment([body_d, body_a], sw);
    painter.line_segment([c + vec2(-3.0, 2.0), c + vec2(2.0, -3.0)], sw);
    painter.line_segment([c + vec2(-1.0, 4.0), c + vec2(4.0, -1.0)], sw);
}

fn icon_analysis(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.35;
    // Crosshair
    painter.line_segment(
        [c + vec2(-s, 0.0), c + vec2(s, 0.0)],
        Stroke::new(1.5, color.gamma_multiply(0.5)),
    );
    painter.line_segment(
        [c + vec2(0.0, -s), c + vec2(0.0, s)],
        Stroke::new(1.5, color.gamma_multiply(0.5)),
    );
    // Curve through origin
    let n = 12;
    let mut pts = Vec::with_capacity(n);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = -s + t * 2.0 * s;
        let y = -s * 0.5 * (t * std::f32::consts::PI).sin();
        pts.push(c + vec2(x, y));
    }
    painter.add(Shape::line(pts, Stroke::new(2.0, color)));
    // Root marker
    painter.circle_filled(c + vec2(0.0, s * 0.5), 3.0, color);
}

fn icon_constraint(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.3;
    let p1 = c + vec2(-s, -s * 0.3);
    let p2 = c + vec2(s, s * 0.3);
    painter.line_segment([p1, p2], Stroke::new(1.5, color.gamma_multiply(0.6)));
    painter.circle_filled(p1, 3.0, color);
    painter.circle_filled(p2, 3.0, color);
    let lk = c + vec2(0.0, -s * 0.7);
    painter.rect_stroke(
        Rect::from_center_size(lk, vec2(6.0, 5.0)),
        1.0,
        Stroke::new(1.5, color),
    );
}

fn icon_boolean(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.28;
    painter.circle_stroke(c + vec2(-s * 0.4, 0.0), s, Stroke::new(1.8, color));
    painter.circle_stroke(c + vec2(s * 0.4, 0.0), s, Stroke::new(1.8, color));
    painter.circle_filled(c, 2.0, color);
}

/// Función de dibujo de icono vectorial para un grupo de la toolbar.
pub type IconFn = fn(&Painter, Rect, Color32);

/// Devuelve el icono vectorial que representa a una herramienta concreta.
pub const fn icon_for_tool(tool: Tool) -> IconFn {
    match tool {
        Tool::Select => icon_move,
        Tool::Point | Tool::Point3D => icon_point,
        Tool::Line | Tool::Line3D => icon_line,
        Tool::Segment | Tool::Segment3D => icon_segment,
        Tool::Ray => icon_ray,
        Tool::Vector => icon_vector,
        Tool::Circle => icon_circle,
        Tool::Polygon => icon_polygon,
        Tool::RegularPolygon => icon_polygon,
        Tool::Pencil => icon_pencil,
        Tool::Eraser => icon_eraser,
        Tool::Function => icon_curve,
        Tool::ParametricCurve2D | Tool::PolarCurve | Tool::ImplicitCurve => icon_curve,
        Tool::VectorField2D | Tool::VectorField3D => icon_vector,
        Tool::Locus => icon_curve,
        Tool::Midpoint => icon_midpoint,
        Tool::Perpendicular | Tool::Parallel => icon_perpendicular,
        Tool::Tangent => icon_tangent,
        Tool::Arc | Tool::Sector => icon_circle,
        Tool::Distance | Tool::Angle | Tool::Area | Tool::Slope => icon_measure,
        Tool::Root
        | Tool::Extremum
        | Tool::Inflection
        | Tool::YIntercept
        | Tool::XIntercept
        | Tool::Intersect
        | Tool::Analyze => icon_analysis,
        Tool::EllipseByFoci
        | Tool::ParabolaByFocusDirectrix
        | Tool::HyperbolaByFoci
        | Tool::ConicByFivePoints => icon_conic,
        Tool::DistanceConstraint
        | Tool::AngleConstraint
        | Tool::Coincident
        | Tool::Horizontal
        | Tool::Vertical
        | Tool::EqualLength
        | Tool::Symmetry => icon_constraint,
        Tool::PolygonUnion
        | Tool::PolygonIntersection
        | Tool::PolygonDifference
        | Tool::PolygonXor => icon_boolean,
        Tool::Plane3D => icon_plane,
        Tool::Sphere3D => icon_circle,
        Tool::Cube3D => icon_3d,
        Tool::Tesseract4D => icon_tesseract_4d,
        Tool::Hypercube5D => icon_hypercube_5d,
        Tool::Cylinder3D | Tool::Cone3D | Tool::Torus3D | Tool::MoebiusStrip => icon_3d,
        Tool::Surface3D | Tool::HyperSurface4D => icon_surface,
        Tool::ParametricCurve3D => icon_curve,
        Tool::Attractor => icon_dynamics,
        Tool::Fractal => icon_advanced,
        Tool::Histogram
        | Tool::ScatterPlot
        | Tool::DomainColoring
        | Tool::HeatMap
        | Tool::ComplexGrid => icon_advanced,
        Tool::Slider | Tool::Button | Tool::Image | Tool::TrigAnimation => icon_advanced,
    }
}

/// Dibuja el icono vectorial compartido para una herramienta concreta.
pub fn draw_tool_icon(painter: &Painter, rect: Rect, tool: Tool, color: Color32) {
    icon_for_tool(tool)(painter, rect, color);
}

// ── Public toolbar ──

/// Toolbar clásica: muestra todos los grupos (más el grupo 3D si `is_3d`).
///
/// Equivalente a [`toolbar_filtered`] con [`ALL_GROUPS`] y, opcionalmente,
/// `ToolGroupId::ThreeD`.
pub fn toolbar(ui: &mut Ui, current_tool: &mut Tool, is_3d: bool) -> egui::Response {
    toolbar_localized(ui, current_tool, is_3d, Locale::Es)
}

/// Toolbar clásica con etiquetas en el idioma pedido (ver [`toolbar`]).
/// ES idéntico al actual; EN traduce grupos y herramientas vía catálogo i18n.
pub fn toolbar_localized(
    ui: &mut Ui,
    current_tool: &mut Tool,
    is_3d: bool,
    locale: Locale,
) -> egui::Response {
    if is_3d {
        let mut groups: Vec<ToolGroupId> = ALL_GROUPS.to_vec();
        groups.push(ToolGroupId::ThreeD);
        groups.push(ToolGroupId::FourD);
        toolbar_filtered_localized(ui, current_tool, &groups, locale)
    } else {
        toolbar_filtered_localized(ui, current_tool, ALL_GROUPS, locale)
    }
}

/// Selector compacto de idioma ES/EN para la barra de herramientas.
///
/// Piel pura: muta `locale` en memoria, sin I/O ni spawn. La persistencia vive
/// en `AppConfig::locale` (grafito-app/src/utils.rs): tras el cambio, el caller
/// guarda la config (ver `save_app_config`). Códigos de idioma, no texto
/// traducible: no necesita claves del catálogo.
pub fn locale_selector(ui: &mut Ui, locale: &mut Locale) -> egui::Response {
    ui.horizontal(|ui| {
        let _ = ui
            .selectable_value(locale, Locale::Es, "ES")
            .on_hover_text("Idioma · Language: Español");
        let _ = ui
            .selectable_value(locale, Locale::En, "EN")
            .on_hover_text("Idioma · Language: English");
    })
    .response
}

pub fn toolbar_uses_overflow(viewport_width: f32) -> bool {
    viewport_width <= COMPACT_TOOLBAR_MAX_WIDTH
}

fn active_toolbar_group(current: Tool, groups: &[ToolGroupId]) -> Option<ToolGroupId> {
    groups.iter().copied().find(|group| {
        if current == Tool::Select {
            *group == ToolGroupId::Move
        } else {
            let (_, tools) = group.def();
            tools.iter().any(|(tool, _, _)| *tool == current)
        }
    })
}

fn compact_toolbar_inline_groups(
    current: Tool,
    groups: &[ToolGroupId],
) -> [Option<ToolGroupId>; 2] {
    let move_group = groups
        .contains(&ToolGroupId::Move)
        .then_some(ToolGroupId::Move);
    let active_group =
        active_toolbar_group(current, groups).filter(|group| Some(*group) != move_group);
    [move_group, active_group]
}

/// Toolbar filtrada: renderiza únicamente los `groups` indicados, en el orden
/// dado. Usada por el sistema de perspectivas para mostrar sólo las
/// herramientas relevantes.
pub fn toolbar_filtered(
    ui: &mut Ui,
    current_tool: &mut Tool,
    groups: &[ToolGroupId],
) -> egui::Response {
    toolbar_filtered_localized(ui, current_tool, groups, Locale::Es)
}

/// Toolbar filtrada con etiquetas en el idioma pedido (ver [`toolbar_filtered`]).
/// ES idéntico al actual; EN traduce grupos y herramientas vía catálogo i18n.
pub fn toolbar_filtered_localized(
    ui: &mut Ui,
    current_tool: &mut Tool,
    groups: &[ToolGroupId],
    locale: Locale,
) -> egui::Response {
    let theme = current_theme(ui.ctx());

    let frame = egui::Frame::none()
        .fill(theme.toolbar_bg)
        .inner_margin(egui::Margin::symmetric(4.0, TOOLBAR_VERTICAL_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
            ui.set_height(TOOLBAR_BUTTON_SIZE);
            if toolbar_uses_overflow(ui.ctx().screen_rect().width()) {
                compact_toolbar(ui, current_tool, groups, locale);
            } else {
                ui.horizontal(|ui| {
                    for &gid in groups {
                        let (_, tools) = gid.def();
                        tool_group(ui, current_tool, tools, locale);
                    }
                });
            }
        });
    // A11Y live-region: anuncia la herramienta vigente etiquetando el frame
    // (respuesta existente, sin widgets nuevos). Lee `current_tool` tras el
    // frame: si cambió en este mismo frame, se anuncia la nueva.
    tag_live_region(&frame.response, toolbar_live_text(*current_tool, locale));
    frame.response
}

/// Toolbar inline para top bar Scandinavian single-bar — sin `Frame` duplicado.
/// Comparte el `Frame` del `TopBottomPanel` padre; solo coloca los grupos.
pub fn toolbar_inline(ui: &mut Ui, current_tool: &mut Tool, groups: &[ToolGroupId]) {
    toolbar_inline_localized(ui, current_tool, groups, Locale::Es)
}

/// Toolbar inline con etiquetas en el idioma pedido (ver [`toolbar_inline`]).
pub fn toolbar_inline_localized(
    ui: &mut Ui,
    current_tool: &mut Tool,
    groups: &[ToolGroupId],
    locale: Locale,
) {
    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
    if toolbar_uses_overflow(ui.ctx().screen_rect().width()) {
        compact_toolbar(ui, current_tool, groups, locale);
    } else {
        ui.horizontal(|ui| {
            for &gid in groups {
                let (_, tools) = gid.def();
                tool_group(ui, current_tool, tools, locale);
            }
        });
    }
}

fn compact_toolbar(ui: &mut Ui, current: &mut Tool, groups: &[ToolGroupId], locale: Locale) {
    let inline_groups = compact_toolbar_inline_groups(*current, groups);
    ui.horizontal(|ui| {
        for group in inline_groups.into_iter().flatten() {
            let (_, tools) = group.def();
            tool_group(ui, current, tools, locale);
        }
        if groups
            .iter()
            .copied()
            .any(|group| !inline_groups.contains(&Some(group)))
        {
            compact_toolbar_overflow(ui, current, groups, inline_groups, locale);
        }
    });
}

fn compact_toolbar_overflow(
    ui: &mut Ui,
    current: &mut Tool,
    groups: &[ToolGroupId],
    inline_groups: [Option<ToolGroupId>; 2],
    locale: Locale,
) {
    let theme = current_theme(ui.ctx());
    let popup_id = ui.make_persistent_id("compact_toolbar_overflow");
    let size = egui::vec2(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Más herramientas")
    });
    let response = response.on_hover_text("Más herramientas");
    // A11Y: foco visible + Enter/Espacio abren el menú (paridad con clic).
    if response.has_focus() {
        theme.paint_focus_ring(ui.painter(), rect);
    }
    let key_activate = key_activates_focused(ui, &response);
    let progress = ui.ctx().animate_bool(
        ui.id().with("compact_toolbar_overflow_state"),
        response.hovered(),
    );
    ui.painter().rect(
        rect,
        RADIUS_MD,
        interpolate_color(Color32::TRANSPARENT, theme.hover_overlay, progress),
        Stroke::new(
            1.0,
            interpolate_color(Color32::TRANSPARENT, theme.separator, progress),
        ),
    );
    draw_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(22.0, 22.0)),
        Icon::Menu,
        interpolate_color(theme.text_secondary, theme.text_primary, progress),
    );

    if response.clicked() || key_activate {
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
    }
    show_compact_toolbar_overflow(
        ui,
        popup_id,
        &response,
        current,
        groups,
        inline_groups,
        locale,
    );
}

fn show_compact_toolbar_overflow(
    ui: &Ui,
    popup_id: egui::Id,
    button: &egui::Response,
    current: &mut Tool,
    groups: &[ToolGroupId],
    inline_groups: [Option<ToolGroupId>; 2],
    locale: Locale,
) {
    if !ui.memory(|memory| memory.is_popup_open(popup_id)) {
        return;
    }

    let menu_width = tool_menu_width(ui.ctx().screen_rect().width());
    let menu_max_height = tool_menu_max_height(ui.ctx().screen_rect().height());
    let mut selected_tool = None;
    let response = egui::Area::new(popup_id.with("constrained_area"))
        .kind(egui::UiKind::Popup)
        .order(egui::Order::Foreground)
        .fixed_pos(button.rect.left_bottom())
        .default_width(menu_width)
        .constrain_to(ui.ctx().screen_rect())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(menu_width);
                ui.label(egui::RichText::new("Más herramientas").strong());
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(menu_max_height)
                    .show(ui, |ui| {
                        for &group in groups {
                            if inline_groups.contains(&Some(group)) {
                                continue;
                            }
                            let (_, tools) = group.def();
                            ui.collapsing(group.label_localized(locale), |ui| {
                                for (tool, name, key) in tools {
                                    let display = entry_display_name(*tool, name, locale);
                                    let selected = *current == *tool;
                                    let response = ui.add_sized(
                                        [menu_width - 12.0, TOOL_MENU_ITEM_HEIGHT],
                                        egui::Button::new(display)
                                            .selected(selected)
                                            .truncate()
                                            .shortcut_text(*key),
                                    );
                                    if response.on_hover_text(display).clicked() {
                                        selected_tool = Some(*tool);
                                    }
                                }
                            });
                        }
                    });
            });
        });

    if let Some(tool) = selected_tool {
        *current = tool;
        ui.memory_mut(|memory| memory.close_popup());
        return;
    }
    let clicked_outside = button.clicked_elsewhere() && response.response.clicked_elsewhere();
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) || clicked_outside {
        ui.memory_mut(|memory| memory.close_popup());
    }
}

fn tool_group(
    ui: &mut Ui,
    current: &mut Tool,
    tools: &[ToolEntry],
    locale: Locale,
) -> egui::Response {
    let theme = current_theme(ui.ctx());
    let is_active = if *current == Tool::Select {
        std::ptr::eq(tools.as_ptr(), GROUP_MOVE.as_ptr())
    } else {
        tools.iter().any(|(t, _, _)| *t == *current)
    };
    let active_tool = tools
        .iter()
        .find(|(t, _, _)| *t == *current)
        .unwrap_or(&tools[0]);
    let label = entry_display_name(active_tool.0, active_tool.1, locale);
    let popup_id = ui.make_persistent_id(("tool_group_menu", tools.as_ptr() as usize));

    let size = egui::vec2(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let resp = resp.on_hover_text(label);
    // A11Y: foco visible (anillo 2px del tema) + Enter/Espacio = clic.
    // El orden Tab lo da egui por orden de creación (orden de `groups`, 5/8/17).
    if resp.has_focus() {
        theme.paint_focus_ring(ui.painter(), rect);
    }
    let key_activate = key_activates_focused(ui, &resp);

    let state_progress = ui.ctx().animate_bool(
        ui.id()
            .with(("toolbar_group_state", tools.as_ptr() as usize)),
        is_active || resp.hovered(),
    );
    let state_fill = if is_active {
        theme.selection_bg
    } else {
        theme.hover_overlay
    };
    let border_color = if is_active {
        theme.accent
    } else {
        theme.separator
    };
    ui.painter().rect(
        rect,
        RADIUS_MD,
        interpolate_color(Color32::TRANSPARENT, state_fill, state_progress),
        Stroke::new(
            1.0,
            interpolate_color(Color32::TRANSPARENT, border_color, state_progress),
        ),
    );
    if is_active {
        let indicator = Rect::from_min_max(
            pos2(rect.min.x + 5.0, rect.max.y - 3.0),
            pos2(rect.max.x - 5.0, rect.max.y),
        );
        ui.painter().rect_filled(indicator, 1.0, theme.accent);
    }

    let icon_rect = Rect::from_center_size(rect.center(), vec2(22.0, 24.0));
    draw_tool_icon(
        ui.painter(),
        icon_rect,
        active_tool.0,
        interpolate_color(theme.text_secondary, theme.text_primary, state_progress),
    );

    if tools.len() > 1 {
        draw_group_menu_indicator(ui.painter(), rect, theme.text_tertiary);
    }

    if resp.clicked() || key_activate {
        if tools.len() > 1 {
            ui.memory_mut(|memory| memory.toggle_popup(popup_id));
        } else if let Some((tool, _, _)) = tools.first() {
            if is_active && *tool == Tool::Pencil && *current == Tool::Pencil {
                *current = Tool::Select;
            } else {
                *current = *tool;
            }
        }
    }
    if tools.len() > 1 {
        show_tool_group_menu(ui, popup_id, &resp, current, tools, locale);
    }
    resp
}

fn show_tool_group_menu(
    ui: &Ui,
    popup_id: egui::Id,
    button: &egui::Response,
    current: &mut Tool,
    tools: &[ToolEntry],
    locale: Locale,
) {
    if !ui.memory(|memory| memory.is_popup_open(popup_id)) {
        return;
    }

    let response = egui::Area::new(popup_id.with("constrained_area"))
        .kind(egui::UiKind::Popup)
        .order(egui::Order::Foreground)
        .fixed_pos(button.rect.left_bottom())
        .default_width(tool_menu_width(ui.ctx().screen_rect().width()))
        .constrain_to(ui.ctx().screen_rect())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| tool_menu(ui, current, tools, locale));
        });

    let clicked_outside = button.clicked_elsewhere() && response.response.clicked_elsewhere();
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) || clicked_outside {
        ui.memory_mut(|memory| memory.close_popup());
    }
}

fn draw_group_menu_indicator(painter: &Painter, rect: Rect, color: Color32) {
    let center = pos2(rect.max.x - 6.0, rect.max.y - 7.0);
    let size = 2.5;
    let stroke = Stroke::new(1.25, color);
    painter.line_segment(
        [
            pos2(center.x - size, center.y - size * 0.4),
            pos2(center.x, center.y + size * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(center.x, center.y + size * 0.6),
            pos2(center.x + size, center.y - size * 0.4),
        ],
        stroke,
    );
}

fn tool_menu(ui: &mut Ui, current: &mut Tool, tools: &[ToolEntry], locale: Locale) {
    let menu_width = tool_menu_width(ui.ctx().screen_rect().width());
    let menu_max_height = tool_menu_max_height(ui.ctx().screen_rect().height());
    ui.set_min_width(menu_width);
    ui.set_max_width(menu_width);
    ui.spacing_mut().item_spacing.y = 2.0;

    egui::ScrollArea::vertical()
        .max_height(menu_max_height)
        .show(ui, |ui| {
            for (tool, name, key) in tools {
                let display = entry_display_name(*tool, name, locale);
                let response = ui.add_sized(
                    [menu_width, TOOL_MENU_ITEM_HEIGHT],
                    egui::Button::new(display).truncate().shortcut_text(*key),
                );
                if response.on_hover_text(display).clicked() {
                    *current = *tool;
                    ui.memory_mut(|memory| memory.close_popup());
                }
            }
        });
}

fn tool_menu_width(viewport_width: f32) -> f32 {
    (viewport_width - TOOL_MENU_SCREEN_MARGIN).clamp(0.0, TOOL_MENU_PREFERRED_WIDTH)
}

fn tool_menu_max_height(viewport_height: f32) -> f32 {
    (viewport_height - TOOL_MENU_VERTICAL_RESERVE)
        .max(0.0)
        .min(viewport_height.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_menu_stays_inside_ordinary_narrow_viewports() {
        for viewport_width in [100.0, 180.0, 220.0, 407.0] {
            assert!(tool_menu_width(viewport_width) <= viewport_width);
        }
    }

    #[test]
    fn tool_menu_limits_its_height_on_short_screens() {
        for viewport_height in [50.0, 240.0, 600.0] {
            assert!(tool_menu_max_height(viewport_height) <= viewport_height);
        }
    }

    #[test]
    fn compact_toolbar_uses_overflow_through_its_narrow_width_boundary() {
        for width in [960.0, 1_026.0, 1_360.0] {
            assert!(toolbar_uses_overflow(width));
        }
        assert!(!toolbar_uses_overflow(1_361.0));
    }

    #[test]
    fn compact_toolbar_keeps_move_and_the_active_group_inline() {
        let groups = [
            ToolGroupId::Move,
            ToolGroupId::Point,
            ToolGroupId::Line,
            ToolGroupId::Circle,
        ];

        assert_eq!(
            compact_toolbar_inline_groups(Tool::Line, &groups),
            [Some(ToolGroupId::Move), Some(ToolGroupId::Line)]
        );
        assert_eq!(
            compact_toolbar_inline_groups(Tool::Select, &groups),
            [Some(ToolGroupId::Move), None]
        );
    }

    #[test]
    fn compact_toolbar_keeps_the_active_four_d_group_inline_with_overflow_available() {
        let groups = [
            ToolGroupId::Move,
            ToolGroupId::ThreeD,
            ToolGroupId::FourD,
            ToolGroupId::Pencil,
        ];

        assert_eq!(
            compact_toolbar_inline_groups(Tool::Tesseract4D, &groups),
            [Some(ToolGroupId::Move), Some(ToolGroupId::FourD)]
        );
        assert!(groups
            .iter()
            .any(|group| *group != ToolGroupId::Move && *group != ToolGroupId::FourD));
        assert!(toolbar_uses_overflow(COMPACT_TOOLBAR_MAX_WIDTH));
    }

    #[test]
    fn toolbar_content_fits_inside_its_fixed_host_panel() {
        let ctx = egui::Context::default();
        let mut current = Tool::Select;
        let mut panel_height = 0.0;
        let mut toolbar_height = 0.0;

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 160.0))),
                ..Default::default()
            },
            |ctx| {
                let panel = egui::TopBottomPanel::top("toolbar_layout_test")
                    .exact_height(TOOLBAR_PANEL_HEIGHT)
                    .frame(egui::Frame::none())
                    .show(ctx, |ui| {
                        toolbar_filtered(
                            ui,
                            &mut current,
                            &[
                                ToolGroupId::Move,
                                ToolGroupId::Point,
                                ToolGroupId::Line,
                                ToolGroupId::Circle,
                            ],
                        )
                    });
                panel_height = panel.response.rect.height();
                toolbar_height = panel.inner.rect.height();
            },
        );

        assert_eq!(panel_height, TOOLBAR_PANEL_HEIGHT);
        assert_eq!(toolbar_height, TOOLBAR_PANEL_HEIGHT);
    }

    #[test]
    fn toolbar_group_buttons_share_one_baseline() {
        let ctx = egui::Context::default();
        let mut current = Tool::Select;
        let mut rects = Vec::new();

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 160.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        for group in [
                            ToolGroupId::Move,
                            ToolGroupId::Point,
                            ToolGroupId::Line,
                            ToolGroupId::Circle,
                        ] {
                            let (_, tools) = group.def();
                            rects.push(tool_group(ui, &mut current, tools, Locale::Es).rect);
                        }
                    });
                });
            },
        );

        assert!(rects
            .iter()
            .all(|rect| rect.height() == TOOLBAR_BUTTON_SIZE));
        assert!(rects.windows(2).all(|pair| pair[0].min.y == pair[1].min.y));
    }

    #[test]
    fn locus_is_reachable_from_the_curve_group_used_by_analytic_perspectives() {
        let (_, curve_tools) = ToolGroupId::Curve.def();
        assert!(curve_tools.iter().any(|(tool, _, _)| *tool == Tool::Locus));
    }

    #[test]
    fn primary_secondary_university_counts_via_level_value_constants() {
        assert_eq!(PRIMARY_TOOL_GROUPS.len(), 5);
        assert_eq!(SECONDARY_TOOL_GROUPS.len(), 8);
        assert_eq!(UNIVERSITY_TOOL_GROUPS.len(), 17);
        assert_eq!(
            TOOLBAR_LEVEL_PRIMARY_MAX, 4,
            "primary bound must remain 4 for progressive disclosure"
        );
        assert_eq!(
            TOOLBAR_LEVEL_SECONDARY_MAX, 10,
            "secondary bound must remain 10"
        );
    }

    #[test]
    fn toolbar_groups_for_level_value_maps_via_pedagogical_level_bounds() {
        // Primary: 0..=PRIMARY_MAX — sampled at 0, 2, 4
        for lv in [0, 2, TOOLBAR_LEVEL_PRIMARY_MAX] {
            assert_eq!(
                toolbar_groups_for_level_value(lv),
                PRIMARY_TOOL_GROUPS,
                "level {lv} should map to primary"
            );
        }
        // Secondary: PRIMARY_MAX+1 ..= SECONDARY_MAX — sampled at 5, 8, 10
        for lv in [
            TOOLBAR_LEVEL_PRIMARY_MAX + 1,
            8,
            TOOLBAR_LEVEL_SECONDARY_MAX,
        ] {
            assert_eq!(
                toolbar_groups_for_level_value(lv),
                SECONDARY_TOOL_GROUPS,
                "level {lv} should map to secondary"
            );
        }
        // University: > SECONDARY_MAX — sampled at 11, 12, 15, 100
        for lv in [TOOLBAR_LEVEL_SECONDARY_MAX + 1, 12, 15, 100] {
            assert_eq!(
                toolbar_groups_for_level_value(lv),
                UNIVERSITY_TOOL_GROUPS,
                "level {lv} should map to university"
            );
        }
        // Cross-check typed helper stays in sync (Single source via level_value())
        for (lvl, expected) in [
            (
                grafito_pedagogy::PedagogicalLevel::Primary,
                PRIMARY_TOOL_GROUPS,
            ),
            (
                grafito_pedagogy::PedagogicalLevel::Secondary,
                SECONDARY_TOOL_GROUPS,
            ),
            (
                grafito_pedagogy::PedagogicalLevel::University,
                UNIVERSITY_TOOL_GROUPS,
            ),
            (
                grafito_pedagogy::PedagogicalLevel::UTN(grafito_pedagogy::UTNProgram::AM1),
                UNIVERSITY_TOOL_GROUPS,
            ),
        ] {
            assert_eq!(toolbar_groups_for_level(lvl), expected);
            assert_eq!(toolbar_groups_for_level_value(lvl.level_value()), expected);
        }
    }

    #[test]
    fn filter_groups_by_level_respects_progressive_disclosure() {
        let perspective = [
            ToolGroupId::Move,
            ToolGroupId::Point,
            ToolGroupId::Advanced,
            ToolGroupId::Constraint,
        ];
        // Primary (level 2) only allows Move/Point among these
        let primary = filter_groups_by_level(&perspective, 2);
        assert_eq!(primary, vec![ToolGroupId::Move, ToolGroupId::Point]);
        // Secondary (level 8) still filters out Advanced/Constraint
        let secondary = filter_groups_by_level(&perspective, 8);
        assert_eq!(secondary, vec![ToolGroupId::Move, ToolGroupId::Point]);
        // University (level 15) passes all
        let uni = filter_groups_by_level(&perspective, 15);
        assert_eq!(uni, perspective.to_vec());
    }

    #[test]
    fn all_76_tools_resolve_non_empty_labels_in_both_locales() {
        use crate::i18n::{tool_label, Locale};
        assert_eq!(ALL_TOOLS.len(), 76, "Tool debe seguir en 76 variantes");
        // Sin duplicados (cada variante una sola vez).
        let mut names: Vec<&str> = ALL_TOOLS.iter().map(Tool::name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 76);
        for tool in ALL_TOOLS {
            let slug = tool_slug(*tool);
            assert!(!slug.is_empty(), "sin slug para {:?}", tool);
            assert!(
                !tool_label(slug, Locale::Es).is_empty(),
                "sin etiqueta ES para {slug}"
            );
            assert!(
                !tool_label(slug, Locale::En).is_empty(),
                "sin etiqueta EN para {slug}"
            );
        }
        // Las 6 incorporadas en O2 resuelven (eran el delta 70 -> 76).
        for slug in [
            "parallel",
            "arc",
            "sector",
            "button",
            "image",
            "trig_animation",
        ] {
            assert!(!tool_label(slug, Locale::Es).is_empty(), "slug {slug}");
            assert!(!tool_label(slug, Locale::En).is_empty(), "slug {slug}");
        }
    }

    #[test]
    fn spanish_toolbar_matches_current_static_tables_exactly() {
        use crate::i18n::Locale;
        // Migración neutra: ES vía catálogo == literales GROUP_* actuales.
        for &group in UNIVERSITY_TOOL_GROUPS {
            assert_eq!(group.label_localized(Locale::Es), group.label());
            let (_, tools) = group.def();
            for (tool, static_label, _) in tools {
                assert_eq!(
                    entry_display_name(*tool, static_label, Locale::Es),
                    *static_label,
                    "ES cambió para {:?}",
                    tool
                );
            }
        }
        // EN traduce de verdad (al menos un grupo y una herramienta difieren).
        assert_ne!(
            ToolGroupId::Point.label_localized(crate::i18n::Locale::En),
            ToolGroupId::Point.label()
        );
        assert_ne!(
            entry_display_name(Tool::Point, "Punto", crate::i18n::Locale::En),
            "Punto"
        );
    }

    #[test]
    fn localized_toolbars_render_in_english_without_panic() {
        use crate::i18n::Locale;
        let ctx = egui::Context::default();
        let mut current = Tool::Select;
        let mut locale = Locale::En;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 160.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    toolbar_filtered_localized(
                        ui,
                        &mut current,
                        UNIVERSITY_TOOL_GROUPS,
                        Locale::En,
                    );
                    toolbar_inline_localized(ui, &mut current, UNIVERSITY_TOOL_GROUPS, Locale::En);
                    locale_selector(ui, &mut locale);
                });
            },
        );
        assert_eq!(current, Tool::Select);
        assert_eq!(locale, Locale::En);
    }

    #[test]
    fn toolbar_live_text_names_the_current_tool() {
        use crate::i18n::Locale;
        let es = toolbar_live_text(Tool::Line, Locale::Es);
        assert!(es.starts_with("Herramienta: "), "{es}");
        assert!(es.len() > "Herramienta: ".len());
        let en = toolbar_live_text(Tool::Line, Locale::En);
        assert!(en.starts_with("Tool: "), "{en}");
        // Cambiar de herramienta cambia el anuncio (el lector anuncia el cambio).
        assert_ne!(
            toolbar_live_text(Tool::Line, Locale::Es),
            toolbar_live_text(Tool::Circle, Locale::Es)
        );
    }

    #[test]
    fn live_region_tags_without_new_widgets() {
        use crate::i18n::Locale;
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1_280.0, 160.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    // Vacío: no etiqueta (sin nodo fantasma).
                    let (_, noop) = ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
                    tag_live_region(&noop, String::new());
                    // Con texto: etiqueta la respuesta existente.
                    let (_, tagged) =
                        ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
                    tag_live_region(&tagged, toolbar_live_text(Tool::Line, Locale::Es));
                });
            },
        );
    }
}
