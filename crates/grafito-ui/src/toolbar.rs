//! Horizontal toolbar with 10 tool groups, each with dropdown.
//! Icons are drawn with egui::Painter — no Unicode dependency.
//! Pattern: one icon per group (last used tool), ▾ opens sub-menu.

use egui::{pos2, vec2, Color32, Painter, Rect, Shape, Stroke, Ui};
use std::f32::consts::TAU;

use crate::Tool;

type ToolEntry = (Tool, &'static str, &'static str);

const GROUP_MOVE: &[ToolEntry] = &[
    (Tool::Select, "↖ Seleccionar", "F1"),
];

const GROUP_POINT: &[ToolEntry] = &[
    (Tool::Point, "· Punto", "F2"),
    (Tool::Point3D, "· Punto 3D", "F7"),
    (Tool::Midpoint, "M Punto medio", ""),
];

const GROUP_LINE: &[ToolEntry] = &[
    (Tool::Line, "╱ Recta", "F3"),
    (Tool::Line, "─ Segmento", ""),
    (Tool::Perpendicular, "⊥ Perpendicular", ""),
];

const GROUP_CIRCLE: &[ToolEntry] = &[
    (Tool::Circle, "○ Círculo centro-punto", "F4"),
    (Tool::Circle, "◎ Círculo centro-radio", ""),
    (Tool::Tangent, "⌒ Tangente", ""),
];

const GROUP_POLYGON: &[ToolEntry] = &[
    (Tool::Polygon, "△ Polígono", "F5"),
    (Tool::Polygon, "⬡ Polígono regular", ""),
];

const GROUP_CONIC: &[ToolEntry] = &[
    (Tool::Select, "◯ Elipse", ""),
    (Tool::Select, "∪ Parábola", ""),
    (Tool::Select, "⊃ Hipérbola", ""),
];

const GROUP_CURVE: &[ToolEntry] = &[
    (Tool::Function, "f(x) Función", "F6"),
    (Tool::Select, "(x,y) Paramétrica 2D", ""),
    (Tool::Select, "r(θ) Polar", ""),
    (Tool::Locus, "⌒ Lugar geométrico", ""),
];

const GROUP_MEASURE: &[ToolEntry] = &[
    (Tool::Distance, "↔ Distancia", ""),
    (Tool::Angle, "∠ Ángulo", ""),
    (Tool::Area, "⬜ Área", ""),
    (Tool::Slope, "m Pendiente", ""),
];

const GROUP_3D: &[ToolEntry] = &[
    (Tool::Point3D, "● Punto 3D", ""),
    (Tool::Sphere3D, "◯ Esfera", "F8"),
    (Tool::Cube3D, "□ Cubo", "F9"),
    (Tool::Select, "△ Pirámide", ""),
    (Tool::Select, "▲ Cono", ""),
    (Tool::Select, "▭ Cilindro", ""),
    (Tool::Select, "◎ Toro", ""),
    (Tool::Select, "⏣ Hipercubo 4D", ""),
];

const GROUP_ADVANCED: &[ToolEntry] = &[
    (Tool::Attractor, "≈ Atractor", ""),
    (Tool::Fractal, "❄ Fractal", ""),
    (Tool::Histogram, "📊 Histograma", ""),
    (Tool::ScatterPlot, "·· Dispersión", ""),
    (Tool::DomainColoring, "🌈 Domain Coloring", ""),
    (Tool::HeatMap, "🔥 Heat Map", ""),
    (Tool::ComplexGrid, "🌀 Complex Grid", ""),
    (Tool::Slider, "═ Deslizador", ""),
    (Tool::Button, "☑ Checkbox/Botón", ""),
    (Tool::Image, "🖼 Imagen", ""),
    (Tool::Select, "T Texto", ""),
];

// ── Vector icon drawing functions ──

fn icon_move(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.38;
    // Arrow vertices: top-left, bottom-middle, inner-corner, right-middle
    let pts = vec![
        c + vec2(-s, -s),           // Top-left tip
        c + vec2(-s*0.2, s*0.8),    // Bottom tip
        c + vec2(-s*0.1, s*0.2),    // Inner corner
        c + vec2(s*0.8, s*0.3),     // Right tip
        c + vec2(-s, -s),           // Back to start to close the path
    ];
    painter.add(Shape::line(pts, Stroke::new(2.0, color)));
}

fn icon_point(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.18;
    painter.circle_filled(c, r.max(2.5), color);
    let mark = rect.width() * 0.32;
    let dim = color.gamma_multiply(0.4);
    painter.line_segment([c - vec2(mark,0.0), c + vec2(mark,0.0)], Stroke::new(1.0, dim));
    painter.line_segment([c - vec2(0.0,mark), c + vec2(0.0,mark)], Stroke::new(1.0, dim));
}

fn icon_line(painter: &Painter, rect: Rect, color: Color32) {
    let m = rect.width() * 0.22;
    let a = rect.min + vec2(m, m * 3.2);
    let b = rect.max - vec2(m * 3.2, m);
    painter.line_segment([a, b], Stroke::new(2.0, color));
    painter.circle_filled(a, 2.2, color);
    painter.circle_filled(b, 2.2, color);
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
    painter.add(Shape::convex_polygon(vec![p1, p2, p3], Color32::TRANSPARENT, Stroke::new(2.0, color)));
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
    for i in 0..n { painter.line_segment([pts[i], pts[(i+1)%n]], Stroke::new(1.8, color)); }
    painter.circle_filled(c, 2.0, color);
}

fn icon_curve(painter: &Painter, rect: Rect, color: Color32) {
    let n = 22;
    let w = rect.width() * 0.78;
    let h = rect.height() * 0.44;
    let sx = rect.center().x - w * 0.5;
    let sy = rect.center().y;
    let mut pts = Vec::with_capacity(n+1);
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
        painter.line_segment([pos2(x, y-5.0), pos2(x, y+5.0)], Stroke::new(1.0, color));
    }
}

fn icon_3d(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width() * 0.28;
    let ftl = c + vec2(-s, -s*0.5);
    let ftr = c + vec2(s*0.5, -s);
    let fbr = c + vec2(s*0.5, s*0.3);
    let fbl = c + vec2(-s, s*0.8);
    let btl = ftl + vec2(-s*0.45, -s*0.45);
    let btr = ftr + vec2(-s*0.45, -s*0.45);
    let sw = Stroke::new(1.5, color);
    painter.line_segment([ftl, ftr], sw);
    painter.line_segment([ftr, fbr], sw);
    painter.line_segment([fbr, fbl], sw);
    painter.line_segment([fbl, ftl], sw);
    painter.line_segment([ftl, btl], sw);
    painter.line_segment([ftr, btr], sw);
    painter.line_segment([btl, btr], sw);
}

fn icon_advanced(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width() * 0.28;
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU;
        painter.line_segment([c - vec2(r*a.cos(), r*a.sin()), c + vec2(r*a.cos(), r*a.sin())], Stroke::new(1.5, color));
    }
    painter.circle_filled(c, 2.8, color);
    painter.circle_stroke(c, r, Stroke::new(1.5, color));
}

type IconFn = fn(&Painter, Rect, Color32);

// ── Public toolbar ──

pub fn toolbar(ui: &mut Ui, current_tool: &mut Tool, is_3d: bool) -> egui::Response {
    let is_dark = ui.visuals().dark_mode;
    let txt = if is_dark { Color32::WHITE } else { Color32::from_rgb(26, 26, 26) };
    let txt_dim = if is_dark { Color32::from_gray(150) } else { Color32::from_gray(100) };
    let accent = Color32::from_rgb(53, 132, 228);
    let bg = if is_dark { Color32::from_rgb(42, 42, 46) } else { Color32::from_rgb(245, 246, 248) };

    egui::Frame::none().fill(bg).inner_margin(egui::Margin::symmetric(3.0, 2.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(1.0, 0.0);
            ui.horizontal(|ui| {
                tool_group(ui, current_tool, icon_move, GROUP_MOVE, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_point, GROUP_POINT, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_line, GROUP_LINE, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_circle, GROUP_CIRCLE, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_polygon, GROUP_POLYGON, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_conic, GROUP_CONIC, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_curve, GROUP_CURVE, accent, txt, txt_dim);
                tool_group(ui, current_tool, icon_measure, GROUP_MEASURE, accent, txt, txt_dim);
                if is_3d {
                    tool_group(ui, current_tool, icon_3d, GROUP_3D, accent, txt, txt_dim);
                }
                tool_group(ui, current_tool, icon_advanced, GROUP_ADVANCED, accent, txt, txt_dim);
            })
        }).response
}

fn tool_group(ui: &mut Ui, current: &mut Tool, icon_fn: IconFn, tools: &[ToolEntry],
              accent: Color32, _txt: Color32, txt_dim: Color32) {
    let is_active = if *current == Tool::Select {
        std::ptr::eq(tools.as_ptr(), GROUP_MOVE.as_ptr())
    } else {
        tools.iter().any(|(t, _, _)| *t == *current)
    };
    let active_tool = tools.iter().find(|(t, _, _)| *t == *current).unwrap_or(&tools[0]);
    let label = active_tool.1;

    let size = egui::vec2(36.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(label);

    // Background
    if is_active || resp.hovered() {
        let fill = if is_active { Color32::from_rgba_unmultiplied(53, 132, 228, 25) }
            else { Color32::from_rgba_unmultiplied(128, 128, 128, 15) };
        ui.painter().rect_filled(rect, 6.0, fill);
    }
    if is_active {
        // Active indicator: bottom border
        let indicator = Rect::from_min_max(
            pos2(rect.min.x + 6.0, rect.max.y - 3.0),
            pos2(rect.max.x - 6.0, rect.max.y),
        );
        ui.painter().rect_filled(indicator, 1.0, accent);
    }

    // Draw the vector icon centered
    let icon_rect = Rect::from_center_size(rect.center(), vec2(22.0, 24.0));
    icon_fn(ui.painter(), icon_rect, if is_active { accent } else { txt_dim });

    if resp.clicked() {
        if let Some((tool, _, _)) = tools.first() { *current = *tool; }
    }
    resp.context_menu(|ui| {
        for (tool, name, key) in tools {
            let key_hint = if key.is_empty() { String::new() } else { format!("  ({})", key) };
            if ui.button(format!("{} {}", name, key_hint)).clicked() {
                *current = *tool; ui.close_menu();
            }
        }
    });
}
