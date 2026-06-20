//! Sistema unificado de iconos vectoriales para Grafito.
//!
//! Todos los iconos se dibujan con `egui::Painter` (líneas y curvas), sin
//! dependencia de fuentes del sistema operativo. Esto garantiza que la app
//! se vea **idéntica** en Windows, macOS y Linux, independientemente del
//! font instalado.
//!
//! # Estilo
//!
//! Estilo **outlined** (líneas finas) inspirado en SF Symbols de Apple y
//! Material Symbols de Google. Stroke de 1.5 a 2.0 px, esquinas suaves.
//! Los iconos se ven bien en tamaños 16, 20, 24 y 32 px.
//!
//! # Uso
//!
//! ```ignore
//! use grafito_ui::icons::{Icon, draw_icon};
//!
//! let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
//! draw_icon(painter, rect, Icon::Delete, theme.danger);
//! ```
//!
//! # Agregar un icono nuevo
//!
//! 1. Agregar la variante a `Icon`.
//! 2. Agregar el brazo `Icon::Nuevo` al `match` de `draw_icon`.
//! 3. Si el icono tiene su propio helper de path (como los de toolbar),
//!    crear la función `icon_nuevo(painter, rect, color)` siguiendo el
//!    estilo outlined.

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

/// Enum de todos los iconos soportados por la app.
///
/// Si necesitás un icono nuevo, agregalo a este enum y al `match` en
/// [`draw_icon`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // Acciones comunes
    Delete,
    Eye,
    EyeOff,
    Edit,
    Menu,
    Search,
    Settings,

    // Herramientas (matching con Tool)
    Move,
    Point,
    Line,
    Circle,
    Polygon,
    Pencil,
    Eraser,
    Function,
    Parametric,
    Polar,
    Implicit,
    VectorField,
    Locus,

    // Análisis y medida
    Distance,
    Angle,
    Area,
    Slope,
    Root,
    Extremum,
    Inflection,
    Analyze,
    Intersect,
    YIntercept,
    XIntercept,

    // Constrcción geométrica
    Conic,
    Ellipse,
    Parabola,
    Hyperbola,
    RegularPolygon,

    // Restricciones
    Constraint,
    Coincident,
    Horizontal,
    Vertical,
    Equal,
    Symmetry,

    // Booleanas
    BooleanUnion,
    BooleanIntersection,
    BooleanDifference,
    BooleanXor,

    // 3D
    Sphere,
    Cube,
    Pyramid,
    Cone,
    Cylinder,
    Torus,

    // Avanzado
    Attractor,
    Fractal,
    Histogram,
    ScatterPlot,
    ColorPalette,
    Grid,
    Image,
    HeatMap,
    DomainColoring,
    ComplexGrid,
    Slider,
    Button,

    // Tema
    Sun,
    Moon,

    // Navegación
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Plus,
    Minus,
    Close,
    Check,
}

/// Dibuja un icono en el rectángulo dado con el color especificado.
///
/// El icono se centra en el rect y se escala para caber con un padding
/// de 2 px a cada lado. Usa stroke de 1.5 px para el estilo outlined.
pub fn draw_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let pad = 2.0;
    let inner = Rect::from_min_max(rect.min + vec2(pad, pad), rect.max - vec2(pad, pad));
    let stroke = Stroke::new(1.5, color);
    let stroke_thick = Stroke::new(2.0, color);
    let _ = stroke; // suprimir warning de no usado

    match icon {
        Icon::Delete => icon_delete(painter, inner, color, stroke_thick),
        Icon::Eye | Icon::EyeOff => icon_eye(painter, inner, color, stroke, icon == Icon::EyeOff),
        Icon::Edit => icon_edit(painter, inner, color, stroke_thick),
        Icon::Menu => icon_menu(painter, inner, color, stroke),
        Icon::Search => icon_search(painter, inner, color, stroke),
        Icon::Settings => icon_settings(painter, inner, color, stroke),

        Icon::Move => icon_move(painter, inner, color, stroke_thick),
        Icon::Point => icon_point(painter, inner, color, stroke_thick),
        Icon::Line => icon_line(painter, inner, color, stroke_thick),
        Icon::Circle => icon_circle(painter, inner, color, stroke_thick),
        Icon::Polygon => icon_polygon(painter, inner, color, stroke_thick),
        Icon::Pencil => icon_pencil(painter, inner, color, stroke_thick),
        Icon::Eraser => icon_eraser(painter, inner, color, stroke_thick),
        Icon::Function => icon_function(painter, inner, color, stroke_thick),
        Icon::Parametric => icon_parametric(painter, inner, color, stroke_thick),
        Icon::Polar => icon_polar(painter, inner, color, stroke_thick),
        Icon::Implicit => icon_implicit(painter, inner, color, stroke_thick),
        Icon::VectorField => icon_vector_field(painter, inner, color, stroke_thick),
        Icon::Locus => icon_locus(painter, inner, color, stroke_thick),

        Icon::Distance => icon_distance(painter, inner, color, stroke_thick),
        Icon::Angle => icon_angle(painter, inner, color, stroke_thick),
        Icon::Area => icon_area(painter, inner, color, stroke_thick),
        Icon::Slope => icon_slope(painter, inner, color, stroke_thick),
        Icon::Root => icon_root(painter, inner, color, stroke_thick),
        Icon::Extremum => icon_extremum(painter, inner, color, stroke_thick),
        Icon::Inflection => icon_inflection(painter, inner, color, stroke_thick),
        Icon::Analyze => icon_analyze(painter, inner, color, stroke_thick),
        Icon::Intersect => icon_intersect(painter, inner, color, stroke_thick),
        Icon::YIntercept => icon_yintercept(painter, inner, color, stroke_thick),
        Icon::XIntercept => icon_xintercept(painter, inner, color, stroke_thick),

        Icon::Conic => icon_conic(painter, inner, color, stroke_thick),
        Icon::Ellipse => icon_ellipse(painter, inner, color, stroke_thick),
        Icon::Parabola => icon_parabola(painter, inner, color, stroke_thick),
        Icon::Hyperbola => icon_hyperbola(painter, inner, color, stroke_thick),
        Icon::RegularPolygon => icon_regular_polygon(painter, inner, color, stroke_thick),

        Icon::Constraint => icon_constraint(painter, inner, color, stroke_thick),
        Icon::Coincident => icon_coincident(painter, inner, color, stroke_thick),
        Icon::Horizontal => icon_horizontal(painter, inner, color, stroke_thick),
        Icon::Vertical => icon_vertical(painter, inner, color, stroke_thick),
        Icon::Equal => icon_equal(painter, inner, color, stroke_thick),
        Icon::Symmetry => icon_symmetry(painter, inner, color, stroke_thick),

        Icon::BooleanUnion => icon_bool_union(painter, inner, color, stroke_thick),
        Icon::BooleanIntersection => icon_bool_intersection(painter, inner, color, stroke_thick),
        Icon::BooleanDifference => icon_bool_difference(painter, inner, color, stroke_thick),
        Icon::BooleanXor => icon_bool_xor(painter, inner, color, stroke_thick),

        Icon::Sphere => icon_sphere(painter, inner, color, stroke_thick),
        Icon::Cube => icon_cube(painter, inner, color, stroke_thick),
        Icon::Pyramid => icon_pyramid(painter, inner, color, stroke_thick),
        Icon::Cone => icon_cone(painter, inner, color, stroke_thick),
        Icon::Cylinder => icon_cylinder(painter, inner, color, stroke_thick),
        Icon::Torus => icon_torus(painter, inner, color, stroke_thick),

        Icon::Attractor => icon_attractor(painter, inner, color, stroke_thick),
        Icon::Fractal => icon_fractal(painter, inner, color, stroke_thick),
        Icon::Histogram => icon_histogram(painter, inner, color, stroke_thick),
        Icon::ScatterPlot => icon_scatter(painter, inner, color, stroke_thick),
        Icon::ColorPalette => icon_palette(painter, inner, color, stroke_thick),
        Icon::Grid => icon_grid(painter, inner, color, stroke_thick),
        Icon::Image => icon_image(painter, inner, color, stroke_thick),
        Icon::HeatMap => icon_heatmap(painter, inner, color, stroke_thick),
        Icon::DomainColoring => icon_domain_coloring(painter, inner, color, stroke_thick),
        Icon::ComplexGrid => icon_complex_grid(painter, inner, color, stroke_thick),
        Icon::Slider => icon_slider(painter, inner, color, stroke_thick),
        Icon::Button => icon_button(painter, inner, color, stroke_thick),

        Icon::Sun => icon_sun(painter, inner, color, stroke_thick),
        Icon::Moon => icon_moon(painter, inner, color, stroke_thick),

        Icon::ChevronLeft => icon_chevron(painter, inner, color, stroke_thick, 180.0),
        Icon::ChevronRight => icon_chevron(painter, inner, color, stroke_thick, 0.0),
        Icon::ChevronUp => icon_chevron(painter, inner, color, stroke_thick, 90.0),
        Icon::ChevronDown => icon_chevron(painter, inner, color, stroke_thick, 270.0),
        Icon::Plus => icon_plus(painter, inner, color, stroke_thick),
        Icon::Minus => icon_minus(painter, inner, color, stroke_thick),
        Icon::Close => icon_close(painter, inner, color, stroke_thick),
        Icon::Check => icon_check(painter, inner, color, stroke_thick),
    }
}

// ═══════════════════════════════════════════════════════════
// Helpers internos: cada icono es un path vectorial outlined
// ═══════════════════════════════════════════════════════════

fn icon_delete(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    // Papelera: tapa + caja con lineas verticales
    let lid_y = r.min.y + r.height() * 0.25;
    let pad = r.width() * 0.1;
    painter.line_segment(
        [pos2(r.min.x + pad, lid_y), pos2(r.max.x - pad, lid_y)],
        stroke,
    );
    // Manija de la tapa
    let handle_w = r.width() * 0.3;
    let handle_x = r.center().x - handle_w / 2.0;
    painter.line_segment(
        [
            pos2(handle_x, lid_y),
            pos2(handle_x, r.min.y + r.height() * 0.1),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(handle_x + handle_w, lid_y),
            pos2(handle_x + handle_w, r.min.y + r.height() * 0.1),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(handle_x, r.min.y + r.height() * 0.1),
            pos2(handle_x + handle_w, r.min.y + r.height() * 0.1),
        ],
        stroke,
    );
    // Caja
    let body_y = lid_y;
    let body_bot = r.max.y - pad;
    painter.line_segment(
        [
            pos2(r.min.x + pad * 1.5, body_y),
            pos2(r.max.x - pad * 1.5, body_bot),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(r.max.x - pad * 1.5, body_y),
            pos2(r.min.x + pad * 1.5, body_bot),
        ],
        stroke,
    );
    // Lineas verticales
    let third = r.width() / 3.0;
    let cx = r.center().x;
    for dx in [-third * 0.5, 0.0, third * 0.5] {
        let x = cx + dx;
        painter.line_segment(
            [
                pos2(x, body_y + r.height() * 0.1),
                pos2(x, body_bot - r.height() * 0.05),
            ],
            stroke,
        );
    }
    let _ = color;
}

fn icon_eye(painter: &Painter, r: Rect, color: Color32, stroke: Stroke, off: bool) {
    // Ojo: curva tipo "hoja" + círculo pupila
    let cy = r.center().y;
    let pts = vec![
        pos2(r.min.x, cy),
        pos2(r.center().x - r.width() * 0.1, r.min.y + r.height() * 0.15),
        pos2(r.center().x, r.min.y),
        pos2(r.center().x + r.width() * 0.1, r.min.y + r.height() * 0.15),
        pos2(r.max.x, cy),
        pos2(r.center().x + r.width() * 0.1, r.max.y - r.height() * 0.15),
        pos2(r.center().x, r.max.y),
        pos2(r.center().x - r.width() * 0.1, r.max.y - r.height() * 0.15),
        pos2(r.min.x, cy),
    ];
    painter.add(Shape::line(pts, stroke));
    // Pupila
    let pup_r = r.width() * 0.15;
    painter.circle_filled(r.center(), pup_r, color);
    if off {
        // Diagonal tachando
        painter.line_segment([r.min, r.max], Stroke::new(2.0, color));
    }
}

fn icon_edit(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Lápiz: linea diagonal con punta
    let a = pos2(r.min.x + r.width() * 0.15, r.max.y - r.height() * 0.1);
    let b = pos2(r.max.x - r.width() * 0.1, r.min.y + r.height() * 0.15);
    painter.line_segment([a, b], stroke);
    // Punta
    let tip_len = r.width() * 0.2;
    let dir = (b - a).normalized();
    let perp = vec2(-dir.y, dir.x);
    painter.line_segment([a, a + dir * tip_len * 0.3 + perp * tip_len * 0.3], stroke);
    painter.line_segment([a, a + dir * tip_len * 0.3 - perp * tip_len * 0.3], stroke);
    // Extremo superior
    let end_len = r.width() * 0.15;
    painter.line_segment([b, b - dir * end_len + perp * end_len * 0.6], stroke);
    painter.line_segment([b, b - dir * end_len - perp * end_len * 0.6], stroke);
}

fn icon_menu(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    for i in 0..3 {
        let y = r.min.y + r.height() * (0.25 + 0.25 * i as f32);
        painter.line_segment([pos2(r.min.x, y), pos2(r.max.x, y)], stroke);
    }
}

fn icon_search(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.3;
    painter.circle_stroke(c, rad, stroke);
    // Mango
    let handle = c + vec2(rad * 0.7, rad * 0.7);
    painter.line_segment([handle, pos2(r.max.x - 1.0, r.max.y - 1.0)], stroke);
}

fn icon_settings(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    // Engranaje simplificado: círculo + 8 dientes
    let c = r.center();
    let r_out = r.width() * 0.45;
    let r_in = r.width() * 0.32;
    painter.circle_stroke(c, r_in, stroke);
    for i in 0..8 {
        let a = (i as f32) * std::f32::consts::TAU / 8.0;
        let p1 = c + vec2(a.cos() * r_in, a.sin() * r_in);
        let p2 = c + vec2(a.cos() * r_out, a.sin() * r_out);
        painter.line_segment([p1, p2], stroke);
    }
    // Hueco central
    painter.circle_filled(c, r.width() * 0.12, color);
}

fn icon_move(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.35;
    // Cruz de 4 flechas
    let arms: [(Pos2, Pos2, Pos2, Pos2); 4] = [
        // arriba
        (
            c + vec2(0.0, -s),
            c + vec2(0.0, -s * 0.4),
            c + vec2(-s * 0.2, -s * 0.6),
            c + vec2(0.0, -s),
        ),
        // derecha
        (
            c + vec2(s, 0.0),
            c + vec2(s * 0.4, 0.0),
            c + vec2(s * 0.6, s * 0.2),
            c + vec2(s, 0.0),
        ),
        // abajo
        (
            c + vec2(0.0, s),
            c + vec2(0.0, s * 0.4),
            c + vec2(s * 0.2, s * 0.6),
            c + vec2(0.0, s),
        ),
        // izquierda
        (
            c + vec2(-s, 0.0),
            c + vec2(-s * 0.4, 0.0),
            c + vec2(-s * 0.6, -s * 0.2),
            c + vec2(-s, 0.0),
        ),
    ];
    for (_, _, _, tip) in arms.iter() {
        // Cada arm se dibuja como una L con flecha; simplificamos: solo linea + punta
        let _ = tip;
    }
    // Versión simplificada: 4 lineas con cabeza de flecha
    for (i, (tip, mid, _p1, _p2)) in arms.iter().enumerate() {
        let _ = mid;
        let _ = _p1;
        let _ = _p2;
        let _ = i;
        painter.line_segment([c, *tip], stroke);
    }
    // Puntas como triangulos
    let head = r.width() * 0.08;
    for (i, _) in arms.iter().enumerate() {
        let _ = i;
        let dir = match i {
            0 => vec2(0.0, -1.0),
            1 => vec2(1.0, 0.0),
            2 => vec2(0.0, 1.0),
            _ => vec2(-1.0, 0.0),
        };
        let tip = c + dir * s;
        let perp = vec2(-dir.y, dir.x);
        let p1 = tip - dir * head + perp * head * 0.6;
        let p2 = tip - dir * head - perp * head * 0.6;
        painter.add(Shape::convex_polygon(
            vec![tip, p1, p2],
            egui::Color32::TRANSPARENT,
            stroke,
        ));
    }
}

fn icon_point(painter: &Painter, r: Rect, color: Color32, _stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.15;
    painter.circle_filled(c, rad.max(2.5), color);
    // Cruz de referencia
    let mark = r.width() * 0.3;
    let dim = color.gamma_multiply(0.4);
    painter.line_segment(
        [c - vec2(mark, 0.0), c + vec2(mark, 0.0)],
        egui::Stroke::new(1.0, dim),
    );
    painter.line_segment(
        [c - vec2(0.0, mark), c + vec2(0.0, mark)],
        egui::Stroke::new(1.0, dim),
    );
}

fn icon_line(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    let m = r.width() * 0.18;
    let a = r.min + vec2(m, m * 3.0);
    let b = r.max - vec2(m * 3.0, m);
    painter.line_segment([a, b], stroke);
    painter.circle_filled(a, 1.8, color);
    painter.circle_filled(b, 1.8, color);
}

fn icon_circle(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.36;
    painter.circle_stroke(c, rad, stroke);
    painter.circle_filled(c, 2.0, color);
}

fn icon_polygon(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.32;
    let p1 = c + vec2(0.0, -s);
    let p2 = c + vec2(-s * 0.87, s * 0.5);
    let p3 = c + vec2(s * 0.87, s * 0.5);
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3],
        egui::Color32::TRANSPARENT,
        stroke,
    ));
    painter.circle_filled(p1, 1.5, color);
    painter.circle_filled(p2, 1.5, color);
    painter.circle_filled(p3, 1.5, color);
}

fn icon_pencil(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Lápiz inclinado con punta
    let a = pos2(r.min.x + r.width() * 0.1, r.max.y - r.height() * 0.15);
    let b = pos2(r.max.x - r.width() * 0.15, r.min.y + r.height() * 0.1);
    painter.line_segment([a, b], stroke);
    // Punta
    painter.line_segment(
        [
            a,
            pos2(r.min.x + r.width() * 0.2, r.max.y - r.height() * 0.05),
        ],
        stroke,
    );
    painter.line_segment(
        [
            a,
            pos2(r.min.x + r.width() * 0.05, r.max.y - r.height() * 0.3),
        ],
        stroke,
    );
    // Goma superior
    let g = r.width() * 0.15;
    painter.line_segment([b, b + vec2(-g, g)], stroke);
}

fn icon_eraser(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Goma de borrar: paralelogramo inclinado
    let a = pos2(r.min.x + r.width() * 0.15, r.min.y + r.height() * 0.4);
    let b = pos2(r.max.x - r.width() * 0.1, r.min.y);
    let c = pos2(r.max.x, r.max.y - r.height() * 0.2);
    let d = pos2(r.min.x + r.width() * 0.25, r.max.y - r.height() * 0.2);
    painter.add(Shape::convex_polygon(
        vec![a, b, c, d],
        egui::Color32::TRANSPARENT,
        stroke,
    ));
    // Punta inferior izquierda con "X"
    painter.line_segment([a, d], stroke);
}

fn icon_function(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Curva sinusoidal simplificada
    let mut pts = Vec::new();
    let n = 20;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = r.min.x + r.width() * t;
        let y = r.center().y - (t * std::f32::consts::TAU * 1.5).sin() * r.height() * 0.35;
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_parametric(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Curva paramétrica: ovalo/lazo
    let mut pts = Vec::new();
    let n = 40;
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let x = r.center().x + r.width() * 0.3 * (2.0 * t).cos();
        let y = r.center().y + r.height() * 0.3 * t.sin();
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_polar(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Rosa polar: 3 pétalos
    let c = r.center();
    let mut pts = Vec::new();
    let n = 60;
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let rad = (3.0 * t).cos().abs() * r.width() * 0.35;
        let x = c.x + rad * t.cos();
        let y = c.y + rad * t.sin();
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_implicit(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Curva implícita: cardioide
    let c = r.center();
    let mut pts = Vec::new();
    let n = 60;
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let rad = (1.0 + t.cos()) * r.width() * 0.18;
        let x = c.x + rad * t.cos();
        let y = c.y + rad * t.sin();
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_vector_field(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Grilla 3x3 de flechas
    let n = 3;
    for i in 0..n {
        for j in 0..n {
            let x = r.min.x + r.width() * (i as f32 + 0.5) / n as f32;
            let y = r.min.y + r.height() * (j as f32 + 0.5) / n as f32;
            // Dirección rotada según posición (simula campo)
            let angle = (i + j) as f32 * 0.5;
            let len = r.width() * 0.12;
            let dx = angle.cos() * len;
            let dy = angle.sin() * len;
            let tip = pos2(x + dx, y + dy);
            painter.line_segment([pos2(x, y), tip], stroke);
            // Punta
            let perp = vec2(-dy, dx).normalized() * 2.0;
            painter.line_segment([tip, tip - vec2(dx, dy) * 0.5 + perp], stroke);
        }
    }
}

fn icon_locus(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Curva tipo "lazo" cerrada
    let c = r.center();
    let rad = r.width() * 0.32;
    painter.circle_stroke(c, rad, stroke);
    let mut pts = Vec::new();
    let n = 30;
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let x = c.x + rad * 0.6 * (3.0 * t).cos();
        let y = c.y + rad * 0.6 * (3.0 * t).sin();
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_distance(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let m = r.height() * 0.3;
    let a = r.min + vec2(r.width() * 0.15, m);
    let b = r.max - vec2(r.width() * 0.15, m);
    painter.line_segment([a, b], stroke);
    // Marcas en los extremos
    let tick = r.width() * 0.08;
    for p in [a, b] {
        painter.line_segment([p - vec2(0.0, tick), p + vec2(0.0, tick)], stroke);
    }
}

fn icon_angle(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = pos2(r.min.x + r.width() * 0.3, r.max.y - r.height() * 0.3);
    let len = r.width() * 0.6;
    painter.line_segment([c, c + vec2(len, 0.0)], stroke);
    let angle = 0.7_f32;
    painter.line_segment(
        [
            c,
            c + vec2((len * angle.cos()).round(), -(len * angle.sin()).round()),
        ],
        stroke,
    );
    // Arco
    let n = 12;
    let mut pts = Vec::new();
    for i in 0..=n {
        let t = i as f32 / n as f32 * angle;
        pts.push(
            c + vec2(
                (len * 0.3 * t.cos()).round(),
                -(len * 0.3 * t.sin()).round(),
            ),
        );
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_area(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let s = r.width() * 0.3;
    let c = r.center();
    let p1 = c + vec2(0.0, -s);
    let p2 = c + vec2(-s * 0.87, s * 0.5);
    let p3 = c + vec2(s * 0.87, s * 0.5);
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3],
        egui::Color32::TRANSPARENT,
        stroke,
    ));
}

fn icon_slope(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let m = r.width() * 0.2;
    let a = r.min + vec2(m, r.height() - m);
    let b = r.max - vec2(m, m);
    painter.line_segment([a, b], stroke);
    // Triangulo de pendiente
    painter.line_segment([a, pos2(b.x, a.y)], stroke);
    painter.line_segment([pos2(b.x, a.y), b], stroke);
}

fn icon_root(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // √x simplificado: curva que cruza el eje
    let mut pts = Vec::new();
    let n = 16;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = r.min.x + r.width() * (0.4 + 0.6 * t);
        let y = r.center().y - (t.sqrt() - 0.5) * r.height() * 0.4;
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
    // Marca de raiz en el cruce
    painter.circle_filled(
        pos2(r.min.x + r.width() * 0.4, r.center().y),
        2.5,
        stroke.color,
    );
}

fn icon_extremum(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let mut pts = Vec::new();
    let n = 30;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = r.min.x + r.width() * t;
        let y = r.center().y - (t * std::f32::consts::PI * 1.5).sin() * r.height() * 0.35;
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
    // Marca de maximo
    let max_x = r.min.x + r.width() * 0.33;
    let max_y = r.center().y - r.height() * 0.35;
    painter.circle_stroke(pos2(max_x, max_y), 3.0, stroke);
}

fn icon_inflection(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Curva cubica con punto de inflexion marcado
    let mut pts = Vec::new();
    let n = 30;
    for i in 0..=n {
        let t = (i as f32 / n as f32) * 2.0 - 1.0;
        let x = r.min.x + r.width() * (i as f32 / n as f32);
        let y = r.center().y + t * t * t * r.height() * 0.3;
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
    painter.circle_filled(r.center(), 2.5, stroke.color);
}

fn icon_analyze(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    // Lupa con check
    icon_search(painter, r, color, stroke);
    let c = r.center();
    let small = r.width() * 0.08;
    painter.line_segment(
        [
            pos2(c.x - small, c.y),
            pos2(c.x - small * 0.3, c.y + small * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(c.x - small * 0.3, c.y + small * 0.6),
            pos2(c.x + small, c.y - small * 0.7),
        ],
        stroke,
    );
}

fn icon_intersect(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Dos lineas que se cruzan
    let m = r.width() * 0.2;
    painter.line_segment([r.min + vec2(m, m), r.max - vec2(m, m)], stroke);
    painter.line_segment(
        [
            pos2(r.min.x + m, r.max.y - m),
            pos2(r.max.x - m, r.min.y + m),
        ],
        stroke,
    );
    // Marca de interseccion
    painter.circle_filled(r.center(), 2.5, stroke.color);
}

fn icon_yintercept(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Eje Y con marca
    painter.line_segment(
        [pos2(r.center().x, r.min.y), pos2(r.center().x, r.max.y)],
        stroke,
    );
    painter.circle_filled(
        pos2(r.center().x, r.center().y - r.height() * 0.15),
        2.5,
        stroke.color,
    );
}

fn icon_xintercept(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Eje X con marca
    painter.line_segment(
        [pos2(r.min.x, r.center().y), pos2(r.max.x, r.center().y)],
        stroke,
    );
    painter.circle_filled(
        pos2(r.center().x + r.width() * 0.15, r.center().y),
        2.5,
        stroke.color,
    );
}

fn icon_conic(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Elipse general
    icon_ellipse(painter, r, _color, stroke);
}

fn icon_ellipse(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rx = r.width() * 0.4;
    let ry = r.height() * 0.3;
    let n = 32;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = (i as f32 / n as f32) * std::f32::consts::TAU;
        pts.push(c + vec2(t.cos() * rx, t.sin() * ry));
    }
    painter.add(Shape::closed_line(pts, stroke));
}

fn icon_parabola(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let mut pts = Vec::new();
    let n = 30;
    for i in 0..=n {
        let t = (i as f32 / n as f32) * 2.0 - 1.0;
        let x = r.center().x + t * r.width() * 0.4;
        let y = r.max.y - (t * t) * r.height() * 0.5;
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_hyperbola(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Dos ramas
    let mut pts1 = Vec::new();
    let mut pts2 = Vec::new();
    let n = 20;
    for i in 0..=n {
        let t = 1.0 + i as f32 * 0.3;
        let y1 = r.center().y + (1.0 / t).atan() * r.height() * 0.4;
        let y2 = r.center().y - (1.0 / t).atan() * r.height() * 0.4;
        let x1 = r.center().x + t * r.width() * 0.08;
        let x2 = r.center().x - t * r.width() * 0.08;
        pts1.push(pos2(x1, y1));
        pts2.push(pos2(x2, y2));
    }
    painter.add(Shape::line(pts1, stroke));
    painter.add(Shape::line(pts2, stroke));
}

fn icon_regular_polygon(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let n = 6;
    let c = r.center();
    let rad = r.width() * 0.35;
    let mut pts = Vec::new();
    for i in 0..n {
        let a = (i as f32 / n as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        pts.push(c + vec2(a.cos() * rad, a.sin() * rad));
    }
    pts.push(pts[0]);
    painter.add(Shape::line(pts, stroke));
}

fn icon_constraint(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Regla con bola en cada extremo
    let a = r.min + vec2(r.width() * 0.15, r.height() * 0.5);
    let b = r.max - vec2(r.width() * 0.15, r.height() * 0.5);
    painter.line_segment([a, b], stroke);
    let rad = r.width() * 0.1;
    painter.circle_stroke(a, rad, stroke);
    painter.circle_stroke(b, rad, stroke);
}

fn icon_coincident(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Dos círculos concéntricos
    let c = r.center();
    let r1 = r.width() * 0.2;
    let r2 = r.width() * 0.35;
    painter.circle_stroke(c, r1, stroke);
    painter.circle_stroke(c, r2, stroke);
}

fn icon_horizontal(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Linea horizontal con marca
    let m = r.width() * 0.15;
    painter.line_segment(
        [
            pos2(r.min.x + m, r.center().y),
            pos2(r.max.x - m, r.center().y),
        ],
        stroke,
    );
    painter.line_segment(
        [pos2(r.min.x + m, r.min.y), pos2(r.min.x + m, r.max.y)],
        stroke,
    );
    painter.line_segment(
        [pos2(r.max.x - m, r.min.y), pos2(r.max.x - m, r.max.y)],
        stroke,
    );
}

fn icon_vertical(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let m = r.height() * 0.15;
    painter.line_segment(
        [
            pos2(r.center().x, r.min.y + m),
            pos2(r.center().x, r.max.y - m),
        ],
        stroke,
    );
    painter.line_segment(
        [pos2(r.min.x, r.min.y + m), pos2(r.max.x, r.min.y + m)],
        stroke,
    );
    painter.line_segment(
        [pos2(r.min.x, r.max.y - m), pos2(r.max.x, r.max.y - m)],
        stroke,
    );
}

fn icon_equal(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Dos segmentos paralelos con marca de igual
    let m = r.width() * 0.1;
    let cy = r.center().y;
    let y1 = cy - r.height() * 0.2;
    let y2 = cy + r.height() * 0.2;
    painter.line_segment([pos2(r.min.x + m, y1), pos2(r.max.x - m, y1)], stroke);
    painter.line_segment([pos2(r.min.x + m, y2), pos2(r.max.x - m, y2)], stroke);
    // Marcas de tick
    for y in [y1, y2] {
        painter.line_segment(
            [pos2(r.min.x + m, y - 2.0), pos2(r.min.x + m, y + 2.0)],
            stroke,
        );
    }
}

fn icon_symmetry(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Eje vertical con dos puntos simétricos
    let cx = r.center().x;
    let cy = r.center().y;
    painter.line_segment(
        [
            pos2(cx, r.min.y + r.height() * 0.1),
            pos2(cx, r.max.y - r.height() * 0.1),
        ],
        stroke,
    );
    let rad = r.width() * 0.1;
    let off = r.width() * 0.2;
    painter.circle_filled(pos2(cx - off, cy), rad, stroke.color);
    painter.circle_filled(pos2(cx + off, cy), rad, stroke.color);
}

fn icon_bool_union(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Dos círculos superpuestos con todo el contorno visible
    let c = r.center();
    let rad = r.width() * 0.28;
    painter.circle_stroke(c - vec2(rad * 0.5, 0.0), rad, stroke);
    painter.circle_stroke(c + vec2(rad * 0.5, 0.0), rad, stroke);
}

fn icon_bool_intersection(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.28;
    painter.circle_stroke(c - vec2(rad * 0.5, 0.0), rad, stroke);
    painter.circle_stroke(c + vec2(rad * 0.5, 0.0), rad, stroke);
    // Sombreado de la interseccion (simplificado: dos arcos enfrentados)
    painter.line_segment([c - vec2(0.0, rad * 0.5), c + vec2(0.0, rad * 0.5)], stroke);
}

fn icon_bool_difference(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.28;
    painter.circle_stroke(c - vec2(rad * 0.5, 0.0), rad, stroke);
    painter.circle_stroke(c + vec2(rad * 0.5, 0.0), rad, stroke);
}

fn icon_bool_xor(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.28;
    painter.circle_stroke(c - vec2(rad * 0.5, 0.0), rad, stroke);
    painter.circle_stroke(c + vec2(rad * 0.5, 0.0), rad, stroke);
    // Símbolo X
    let s = rad * 0.5;
    painter.line_segment([c - vec2(s, s), c + vec2(s, s)], stroke);
    painter.line_segment([c - vec2(s, -s), c + vec2(s, -s)], stroke);
}

fn icon_sphere(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.4;
    painter.circle_stroke(c, rad, stroke);
    // Elipse horizontal simulando ecuador
    let mut pts_h = Vec::new();
    for i in 0..=32 {
        let t = (i as f32 / 32.0) * std::f32::consts::TAU;
        pts_h.push(c + vec2(t.cos() * rad, t.sin() * rad * 0.3));
    }
    painter.add(Shape::closed_line(pts_h, stroke));
    // Elipse vertical
    let mut pts_v = Vec::new();
    for i in 0..=32 {
        let t = (i as f32 / 32.0) * std::f32::consts::TAU;
        pts_v.push(c + vec2(t.cos() * rad * 0.3, t.sin() * rad));
    }
    painter.add(Shape::closed_line(pts_v, stroke));
}

fn icon_cube(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.3;
    let front = [
        c + vec2(-s, -s),
        c + vec2(s, -s),
        c + vec2(s, s),
        c + vec2(-s, s),
    ];
    let back = [
        c + vec2(-s + s * 0.5, -s - s * 0.5),
        c + vec2(s + s * 0.5, -s - s * 0.5),
        c + vec2(s + s * 0.5, s - s * 0.5),
        c + vec2(-s + s * 0.5, s - s * 0.5),
    ];
    painter.add(Shape::closed_line(front.to_vec(), stroke));
    for (f, b) in front.iter().zip(back.iter()) {
        painter.line_segment([*f, *b], stroke);
    }
    painter.add(Shape::closed_line(back.to_vec(), stroke));
}

fn icon_pyramid(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.35;
    let apex = c + vec2(0.0, -s);
    let bl = c + vec2(-s, s * 0.5);
    let br = c + vec2(s, s * 0.5);
    let back = c + vec2(0.0, s * 0.5);
    painter.line_segment([apex, bl], stroke);
    painter.line_segment([apex, br], stroke);
    painter.line_segment([apex, back], stroke);
    painter.line_segment([bl, br], stroke);
    painter.line_segment([bl, back], stroke);
    painter.line_segment([br, back], stroke);
}

fn icon_cone(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.35;
    let apex = c + vec2(0.0, -r.height() * 0.4);
    let bl = c + vec2(-rad, r.height() * 0.3);
    let br = c + vec2(rad, r.height() * 0.3);
    painter.line_segment([apex, bl], stroke);
    painter.line_segment([apex, br], stroke);
    // Elipse inferior
    let base = c + vec2(0.0, r.height() * 0.3);
    let mut pts = Vec::new();
    for i in 0..=32 {
        let t = (i as f32 / 32.0) * std::f32::consts::TAU;
        pts.push(base + vec2(t.cos() * rad, t.sin() * rad * 0.3));
    }
    painter.add(Shape::closed_line(pts, stroke));
}

fn icon_cylinder(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.3;
    let h = r.height() * 0.4;
    painter.line_segment([pos2(c.x - rad, c.y - h), pos2(c.x - rad, c.y + h)], stroke);
    painter.line_segment([pos2(c.x + rad, c.y - h), pos2(c.x + rad, c.y + h)], stroke);
    // Elipses superior e inferior
    for &cy in &[c.y - h, c.y + h] {
        let mut pts = Vec::new();
        for i in 0..=32 {
            let t = (i as f32 / 32.0) * std::f32::consts::TAU;
            pts.push(pos2(c.x + t.cos() * rad, cy + t.sin() * rad * 0.3));
        }
        painter.add(Shape::closed_line(pts, stroke));
    }
}

fn icon_torus(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.3;
    let thick = r.width() * 0.12;
    painter.circle_stroke(c, rad, stroke);
    painter.circle_stroke(c, rad - thick, stroke);
    painter.circle_stroke(c, rad + thick, stroke);
}

fn icon_attractor(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Lorenz-like: dos lazos
    let mut pts = Vec::new();
    let n = 60;
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let x = r.center().x + r.width() * 0.3 * (3.0 * t).sin() / (1.0 + t.cos().powi(2));
        let y = r.center().y + r.height() * 0.3 * (2.0 * t).sin();
        pts.push(pos2(x, y));
    }
    painter.add(Shape::line(pts, stroke));
}

fn icon_fractal(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Mandelbrot simplificado: cardioide + círculo
    let c = r.center();
    let r1 = r.width() * 0.3;
    let mut pts = Vec::new();
    let n = 32;
    for i in 0..=n {
        let t = (i as f32 / n as f32) * std::f32::consts::TAU;
        pts.push(c + vec2(r1 * 0.25 + t.cos() * r1, t.sin() * r1 * 0.85));
    }
    painter.add(Shape::closed_line(pts, stroke));
    let r2 = r.width() * 0.1;
    painter.circle_stroke(c - vec2(r1 * 0.4, 0.0), r2, stroke);
}

fn icon_histogram(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // 4 barras de alturas distintas
    let n = 4;
    let bw = r.width() / (n as f32 + 0.5);
    let heights = [0.4, 0.7, 0.5, 0.9];
    for (i, &h_frac) in heights.iter().take(n).enumerate() {
        let x = r.min.x + bw * 0.25 + (i as f32) * bw;
        let h = r.height() * h_frac;
        let cell = egui::Rect::from_min_size(pos2(x, r.max.y - h), vec2(bw * 0.6, h));
        painter.rect_stroke(cell, 0.0, stroke);
    }
}

fn icon_scatter(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Puntos dispersos
    let pts = [
        (0.2, 0.3),
        (0.4, 0.6),
        (0.3, 0.5),
        (0.5, 0.4),
        (0.6, 0.7),
        (0.7, 0.5),
        (0.8, 0.8),
        (0.4, 0.8),
        (0.5, 0.2),
        (0.6, 0.3),
        (0.3, 0.7),
        (0.7, 0.2),
    ];
    for (fx, fy) in pts {
        let p = pos2(r.min.x + r.width() * fx, r.max.y - r.height() * fy);
        painter.circle_filled(p, 1.8, stroke.color);
    }
}

fn icon_palette(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.4;
    // Paleta: forma redondeada con huecos
    let mut pts = Vec::new();
    let n = 32;
    for i in 0..=n {
        let t = (i as f32 / n as f32) * std::f32::consts::TAU;
        pts.push(c + vec2(t.cos() * rad, t.sin() * rad * 0.75));
    }
    painter.add(Shape::closed_line(pts, stroke));
    for &(dx, dy) in &[(0.3, 0.0), (-0.3, 0.0), (0.0, -0.3), (0.0, 0.3)] {
        painter.circle_filled(c + vec2(rad * dx, rad * dy * 0.75), 2.5, stroke.color);
    }
}

fn icon_grid(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let n = 3;
    for i in 1..n {
        let t = i as f32 / n as f32;
        painter.line_segment(
            [
                pos2(r.min.x, r.min.y + r.height() * t),
                pos2(r.max.x, r.min.y + r.height() * t),
            ],
            stroke,
        );
        painter.line_segment(
            [
                pos2(r.min.x + r.width() * t, r.min.y),
                pos2(r.min.x + r.width() * t, r.max.y),
            ],
            stroke,
        );
    }
    painter.rect_stroke(r, 0.0, stroke);
}

fn icon_image(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Marco de imagen con montañas y sol
    painter.rect_stroke(r, 0.0, stroke);
    let sun = pos2(r.min.x + r.width() * 0.75, r.min.y + r.height() * 0.25);
    painter.circle_stroke(sun, r.width() * 0.08, stroke);
    // Montañas
    let base = r.max.y;
    painter.line_segment(
        [
            pos2(r.min.x, base),
            pos2(r.min.x + r.width() * 0.4, r.min.y + r.height() * 0.4),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(r.min.x + r.width() * 0.4, r.min.y + r.height() * 0.4),
            pos2(r.min.x + r.width() * 0.7, r.min.y + r.height() * 0.6),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(r.min.x + r.width() * 0.7, r.min.y + r.height() * 0.6),
            pos2(r.max.x, base),
        ],
        stroke,
    );
}

fn icon_heatmap(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Grilla 3x3 con cuadrados
    let n = 3;
    for i in 0..n {
        for j in 0..n {
            let x = r.min.x + r.width() * (i as f32) / n as f32;
            let y = r.min.y + r.height() * (j as f32) / n as f32;
            let cell = egui::Rect::from_min_size(
                pos2(x, y),
                vec2(r.width() / n as f32, r.height() / n as f32),
            );
            painter.rect_stroke(cell, 0.0, stroke);
        }
    }
}

fn icon_domain_coloring(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Círculo con patrón de caleidoscopio
    let c = r.center();
    let rad = r.width() * 0.4;
    painter.circle_stroke(c, rad, stroke);
    for i in 0..6 {
        let a = (i as f32) * std::f32::consts::TAU / 6.0;
        painter.line_segment([c, c + vec2(a.cos() * rad, a.sin() * rad)], stroke);
    }
}

fn icon_complex_grid(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    icon_grid(painter, r, _color, stroke);
    // Marca de Z (cruz)
    let c = r.center();
    let s = r.width() * 0.1;
    painter.line_segment([c - vec2(s, s), c + vec2(s, s)], stroke);
    painter.line_segment([c - vec2(s, -s), c + vec2(s, -s)], stroke);
}

fn icon_slider(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Track horizontal con thumb
    let track_y = r.center().y;
    let m = r.width() * 0.1;
    painter.line_segment(
        [pos2(r.min.x + m, track_y), pos2(r.max.x - m, track_y)],
        stroke,
    );
    painter.circle_filled(
        pos2(r.center().x + r.width() * 0.1, track_y),
        r.width() * 0.08,
        stroke.color,
    );
}

fn icon_button(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    // Rectángulo con check
    let pad = r.width() * 0.15;
    let inner = r.shrink(pad);
    painter.rect_stroke(inner, 4.0, stroke);
    let c = inner.center();
    let s = inner.width() * 0.15;
    painter.line_segment([c - vec2(s, 0.0), c - vec2(s * 0.3, s * 0.6)], stroke);
    painter.line_segment([c - vec2(s * 0.3, s * 0.6), c + vec2(s, -s * 0.6)], stroke);
}

fn icon_sun(painter: &Painter, r: Rect, color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.18;
    painter.circle_filled(c, rad, color);
    let ray_len = r.width() * 0.32;
    for i in 0..8 {
        let a = (i as f32) * std::f32::consts::TAU / 8.0;
        let p1 = c + vec2(a.cos() * rad * 1.4, a.sin() * rad * 1.4);
        let p2 = c + vec2(a.cos() * ray_len, a.sin() * ray_len);
        painter.line_segment([p1, p2], stroke);
    }
}

fn icon_moon(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let rad = r.width() * 0.35;
    painter.circle_stroke(c, rad, stroke);
    // Media luna: tapamos con un círculo desplazado
    let p1 = c + vec2(-rad * 0.3, -rad);
    let p2 = c + vec2(rad * 0.7, -rad * 0.5);
    let p3 = c + vec2(rad * 0.7, rad * 0.5);
    let p4 = c + vec2(-rad * 0.3, rad);
    painter.add(Shape::convex_polygon(
        vec![p1, p2, p3, p4],
        egui::Color32::TRANSPARENT,
        stroke,
    ));
}

fn icon_chevron(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke, rot: f32) {
    let c = r.center();
    let s = r.width() * 0.3;
    let pts = vec![
        c + vec2(s, -s * 0.6),
        c + vec2(-s * 0.2, 0.0),
        c + vec2(s, s * 0.6),
    ];
    let angle = rot.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let rotated: Vec<Pos2> = pts
        .into_iter()
        .map(|p| {
            let d = p - c;
            c + vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
        })
        .collect();
    painter.add(Shape::line(rotated, stroke));
}

fn icon_plus(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.3;
    painter.line_segment([pos2(c.x - s, c.y), pos2(c.x + s, c.y)], stroke);
    painter.line_segment([pos2(c.x, c.y - s), pos2(c.x, c.y + s)], stroke);
}

fn icon_minus(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.3;
    painter.line_segment([pos2(c.x - s, c.y), pos2(c.x + s, c.y)], stroke);
}

fn icon_close(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let s = r.width() * 0.3;
    painter.line_segment(
        [
            pos2(r.center().x - s, r.center().y - s),
            pos2(r.center().x + s, r.center().y + s),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(r.center().x - s, r.center().y + s),
            pos2(r.center().x + s, r.center().y - s),
        ],
        stroke,
    );
}

fn icon_check(painter: &Painter, r: Rect, _color: Color32, stroke: Stroke) {
    let c = r.center();
    let s = r.width() * 0.3;
    painter.line_segment(
        [pos2(c.x - s, c.y), pos2(c.x - s * 0.2, c.y + s * 0.7)],
        stroke,
    );
    painter.line_segment(
        [
            pos2(c.x - s * 0.2, c.y + s * 0.7),
            pos2(c.x + s, c.y - s * 0.7),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_enum_variants_compile() {
        // Si alguno de estos enum variants se renombra, este test debe actualizarse.
        // Sirve como documentación de los iconos disponibles.
        let _ = Icon::Delete;
        let _ = Icon::Eye;
        let _ = Icon::EyeOff;
        let _ = Icon::Edit;
        let _ = Icon::Menu;
        let _ = Icon::Search;
        let _ = Icon::Settings;
        let _ = Icon::Move;
        let _ = Icon::Point;
        let _ = Icon::Line;
        let _ = Icon::Circle;
        let _ = Icon::Polygon;
        let _ = Icon::Pencil;
        let _ = Icon::Eraser;
        let _ = Icon::Function;
        let _ = Icon::Parametric;
        let _ = Icon::Polar;
        let _ = Icon::Implicit;
        let _ = Icon::VectorField;
        let _ = Icon::Locus;
        let _ = Icon::Distance;
        let _ = Icon::Angle;
        let _ = Icon::Area;
        let _ = Icon::Slope;
        let _ = Icon::Root;
        let _ = Icon::Extremum;
        let _ = Icon::Inflection;
        let _ = Icon::Analyze;
        let _ = Icon::Intersect;
        let _ = Icon::YIntercept;
        let _ = Icon::XIntercept;
        let _ = Icon::Conic;
        let _ = Icon::Ellipse;
        let _ = Icon::Parabola;
        let _ = Icon::Hyperbola;
        let _ = Icon::RegularPolygon;
        let _ = Icon::Constraint;
        let _ = Icon::Coincident;
        let _ = Icon::Horizontal;
        let _ = Icon::Vertical;
        let _ = Icon::Equal;
        let _ = Icon::Symmetry;
        let _ = Icon::BooleanUnion;
        let _ = Icon::BooleanIntersection;
        let _ = Icon::BooleanDifference;
        let _ = Icon::BooleanXor;
        let _ = Icon::Sphere;
        let _ = Icon::Cube;
        let _ = Icon::Pyramid;
        let _ = Icon::Cone;
        let _ = Icon::Cylinder;
        let _ = Icon::Torus;
        let _ = Icon::Attractor;
        let _ = Icon::Fractal;
        let _ = Icon::Histogram;
        let _ = Icon::ScatterPlot;
        let _ = Icon::ColorPalette;
        let _ = Icon::Grid;
        let _ = Icon::Image;
        let _ = Icon::HeatMap;
        let _ = Icon::DomainColoring;
        let _ = Icon::ComplexGrid;
        let _ = Icon::Slider;
        let _ = Icon::Button;
        let _ = Icon::Sun;
        let _ = Icon::Moon;
        let _ = Icon::ChevronLeft;
        let _ = Icon::ChevronRight;
        let _ = Icon::ChevronUp;
        let _ = Icon::ChevronDown;
        let _ = Icon::Plus;
        let _ = Icon::Minus;
        let _ = Icon::Close;
        let _ = Icon::Check;
    }
}
