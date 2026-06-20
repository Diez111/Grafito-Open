//! Grafito UI — Componentes y paneles de interfaz construidos con egui.
//!
//! Provee la toolbar, la paleta de comandos, el panel de álgebra, el selector
//! de color, temas y la enumeración [`Tool`] que sincroniza el modo de
//! interacción del canvas.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_ui::{Tool, AlgebraAction};
//!
//! let mut tool = Tool::default();
//! assert_eq!(tool, Tool::Select);
//!
//! tool = Tool::Point;
//! assert_eq!(tool.cursor_icon(), egui::CursorIcon::Crosshair);
//! ```

pub mod animation;
pub mod color_picker;
pub mod command_palette;
pub mod icons;
pub mod keyboard;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod toolbar;

use egui::{Color32, Response, Ui};
use grafito_core::{Document, ObjectId};

use crate::theme::current_theme;

pub enum AlgebraAction {
    Delete(ObjectId),
    ToggleVisibility(ObjectId),
}

pub fn algebra_view(
    ui: &mut Ui,
    document: &mut Document,
    selected: &mut Option<ObjectId>,
) -> Vec<AlgebraAction> {
    let mut actions = Vec::new();
    let theme = current_theme(ui.ctx());

    // Empty state (PR 6 polish): cuando no hay objetos, mostrar mensaje
    // amigable con icono vectorial e instrucciones.
    if document.objects_iter().count() == 0 {
        ui.vertical_centered(|ui| {
            ui.add_space(32.0);
            // Icono vectorial grande
            let (icon_rect, _) =
                ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
            if ui.is_rect_visible(icon_rect) {
                crate::icons::draw_icon(
                    ui.painter(),
                    icon_rect,
                    crate::icons::Icon::Point,
                    theme.text_tertiary,
                );
            }
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("Sin objetos")
                    .size(15.0)
                    .color(theme.text_secondary),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Escribí en la barra inferior\npara crear tu primer objeto")
                    .size(12.0)
                    .color(theme.text_tertiary),
            );
        });
        return actions;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Collect object IDs first to avoid mutable borrow issues while iterating
        let object_ids: Vec<ObjectId> = document.objects_iter().map(|(id, _)| *id).collect();

        for id in object_ids {
            let is_selected = selected.map(|s| s == id).unwrap_or(false);
            let is_hovered = false;

            // Outer frame for the item to give it nice hover effects and padding
            let mut frame = egui::Frame::default().inner_margin(egui::vec2(16.0, 12.0));
            if is_selected {
                frame.fill = theme.selection_bg;
            }

            let response = frame
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(obj) = document.get_object(id) {
                            // Color del objeto según la leyenda semántica del tema
                            let color = match obj.name() {
                                "Point" | "Point3D" => theme.object_point,
                                "Line" => theme.object_line,
                                "Function" => theme.object_function,
                                "Circle" | "Ellipse" | "Sphere3D" | "Cube3D" | "Polygon" => {
                                    theme.object_polygon
                                }
                                name if name.contains("Conic")
                                    || name == "Parabola"
                                    || name == "Hyperbola" =>
                                {
                                    theme.object_conic
                                }
                                _ => theme.object_line,
                            };
                            let (dot_rect, _) = ui
                                .allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 5.0, color);

                            ui.add_space(8.0);
                            let text = format!("{}: {}", obj.label(), obj.name());
                            ui.label(
                                egui::RichText::new(text)
                                    .size(15.0)
                                    .color(theme.text_primary),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Botón de eliminar (icono vectorial outlined)
                                    let (del_rect, del_resp) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::click(),
                                    );
                                    if ui.is_rect_visible(del_rect) {
                                        icons::draw_icon(
                                            ui.painter(),
                                            del_rect,
                                            icons::Icon::Delete,
                                            theme.text_secondary,
                                        );
                                    }
                                    if del_resp.on_hover_text("Eliminar objeto").clicked() {
                                        actions.push(AlgebraAction::Delete(id));
                                    }
                                    // Botón de visibilidad (iconos eye/eye_off)
                                    let (eye_rect, eye_resp) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::click(),
                                    );
                                    if ui.is_rect_visible(eye_rect) {
                                        icons::draw_icon(
                                            ui.painter(),
                                            eye_rect,
                                            if obj.is_visible() {
                                                icons::Icon::Eye
                                            } else {
                                                icons::Icon::EyeOff
                                            },
                                            theme.text_secondary,
                                        );
                                    }
                                    if eye_resp.on_hover_text("Alternar visibilidad").clicked() {
                                        actions.push(AlgebraAction::ToggleVisibility(id));
                                    }
                                },
                            );
                        }
                    });

                    // Show properties inline if selected
                    if is_selected {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("Propiedades")
                                .color(theme.text_label)
                                .size(14.0),
                        );
                        ui.add_space(8.0);

                        if let Some(obj) = document.get_object_mut(id) {
                            ui.horizontal(|ui| {
                                ui.add_space(24.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("Tipo: {}", obj.name()))
                                            .size(13.0)
                                            .color(theme.text_secondary),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(format!("Etiqueta: {}", obj.label()))
                                            .size(13.0)
                                            .color(theme.text_secondary),
                                    );
                                    ui.add_space(4.0);
                                    let mut vis = obj.is_visible();
                                    if ui
                                        .checkbox(
                                            &mut vis,
                                            egui::RichText::new("Visible")
                                                .size(13.0)
                                                .color(theme.text_primary),
                                        )
                                        .changed()
                                    {
                                        obj.set_visible(vis);
                                    }
                                });
                            });
                        }
                    }
                })
                .response;

            // Add interaction to select the item
            let interact_resp = ui.interact(response.rect, ui.id().with(id), egui::Sense::click());
            if interact_resp.clicked() {
                if is_selected {
                    *selected = None;
                } else {
                    *selected = Some(id);
                }
            }

            // Hover overlay (PR 6 polish) — rect filled on top del frame
            if interact_resp.hovered() && !is_selected {
                ui.painter().rect_filled(
                    response.rect,
                    egui::Rounding::same(6.0),
                    theme.hover_overlay,
                );
            }

            // Separator entre items
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 1.0),
                    egui::Sense::hover(),
                );
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    egui::Stroke::new(1.0, theme.separator),
                );
            });

            // Suprimir warning de variable no usada
            let _ = is_hovered;
        }
    });
    actions
}

/// Display the Properties panel for a selected object.
pub fn properties_panel(ui: &mut Ui, document: &mut Document, id: ObjectId) {
    let theme = current_theme(ui.ctx());
    ui.heading("Propiedades");
    ui.separator();
    if let Some(obj) = document.get_object_mut(id) {
        // Basic properties
        ui.label(
            egui::RichText::new(format!("Tipo: {}", obj.name()))
                .strong()
                .color(theme.text_primary),
        );
        ui.add_space(4.0);

        // Editable label
        let mut label = obj.label().to_string();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Etiqueta:").color(theme.text_secondary));
            if ui.text_edit_singleline(&mut label).changed() {
                obj.set_label(label);
            }
        });
        ui.add_space(4.0);

        // Visibility checkbox
        let mut visible = obj.is_visible();
        if ui.checkbox(&mut visible, "Visible").changed() {
            obj.set_visible(visible);
        }
        ui.add_space(4.0);

        // Color picker button
        let color = obj.color();
        let color_btn = egui::Button::new(
            egui::RichText::new("■")
                .color(Color32::from_rgba_premultiplied(
                    (color.r * 255.0) as u8,
                    (color.g * 255.0) as u8,
                    (color.b * 255.0) as u8,
                    (color.a * 255.0) as u8,
                ))
                .size(20.0),
        );
        if ui.add(color_btn).on_hover_text("Cambiar color").clicked() {
            // TODO: Open color picker
        }

        ui.separator();
        ui.label(egui::RichText::new("Mediciones").strong());
        ui.add_space(4.0);

        // Measurements
        fn px(val: f64) -> String {
            format!("{:.2}", val)
        }
        match obj {
            grafito_core::GeoObject::Point(p) => {
                ui.label(format!(
                    "Posición: ({}, {})",
                    px(p.position.x),
                    px(p.position.y)
                ));
            }
            grafito_core::GeoObject::Line(l) => {
                let kind_str = match l.kind {
                    grafito_core::LineKind::Segment => "Segmento",
                    grafito_core::LineKind::Ray => "Semirrecta",
                    grafito_core::LineKind::Line => "Recta",
                };
                ui.label(format!("Tipo: {}", kind_str));
                ui.label(format!("Inicio: ({}, {})", px(l.start.x), px(l.start.y)));
                ui.label(format!("Fin: ({}, {})", px(l.end.x), px(l.end.y)));
                ui.label(format!("Longitud: {}", px(l.start.distance(&l.end))));
            }
            grafito_core::GeoObject::Circle(c) => {
                ui.label(format!("Centro: ({}, {})", px(c.center.x), px(c.center.y)));
                ui.label(format!("Radio: {}", px(c.radius)));
                ui.label(format!(
                    "Área: {}",
                    px(std::f64::consts::PI * c.radius * c.radius)
                ));
                ui.label(format!(
                    "Circunferencia: {}",
                    px(2.0 * std::f64::consts::PI * c.radius)
                ));
            }
            grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                ui.label(format!("Vértices: {}", poly.vertices.len()));
                let mut perimeter = 0.0;
                for i in 0..poly.vertices.len() {
                    let a = poly.vertices[i];
                    let b = poly.vertices[(i + 1) % poly.vertices.len()];
                    perimeter += a.distance(&b);
                }
                ui.label(format!("Perímetro: {}", px(perimeter)));
                // Shoelace area
                let mut area = 0.0;
                for i in 0..poly.vertices.len() {
                    let a = poly.vertices[i];
                    let b = poly.vertices[(i + 1) % poly.vertices.len()];
                    area += a.x * b.y - b.x * a.y;
                }
                ui.label(format!("Área: {}", px(area.abs() * 0.5)));
            }
            grafito_core::GeoObject::Function(f) => {
                ui.label(format!("Expresión: {}", f.expr));
                if let Some(min) = f.domain_min {
                    ui.label(format!("Dominio mín: {}", px(min)));
                }
                if let Some(max) = f.domain_max {
                    ui.label(format!("Dominio máx: {}", px(max)));
                }
            }
            grafito_core::GeoObject::Ellipse(e) => {
                ui.label(format!("Centro: ({}, {})", px(e.center.x), px(e.center.y)));
                ui.label(format!("Semieje mayor (rx): {}", px(e.rx)));
                ui.label(format!("Semieje menor (ry): {}", px(e.ry)));
                ui.label(format!("Área: {}", px(std::f64::consts::PI * e.rx * e.ry)));
            }
            grafito_core::GeoObject::Point3D(p) => {
                ui.label(format!(
                    "Posición: ({}, {}, {})",
                    px(p.position.x),
                    px(p.position.y),
                    px(p.position.z)
                ));
            }
            grafito_core::GeoObject::Sphere3D(s) => {
                ui.label(format!(
                    "Centro: ({}, {}, {})",
                    px(s.center.x),
                    px(s.center.y),
                    px(s.center.z)
                ));
                ui.label(format!("Radio: {}", px(s.radius)));
                ui.label(format!(
                    "Volumen: {}",
                    px(4.0 / 3.0 * std::f64::consts::PI * s.radius.powi(3))
                ));
                ui.label(format!(
                    "Área superficial: {}",
                    px(4.0 * std::f64::consts::PI * s.radius.powi(2))
                ));
            }
            grafito_core::GeoObject::Cube3D(c) => {
                ui.label(format!(
                    "Centro: ({}, {}, {})",
                    px(c.center.x),
                    px(c.center.y),
                    px(c.center.z)
                ));
                ui.label(format!("Tamaño: {}", px(c.size)));
                ui.label(format!("Volumen: {}", px(c.size.powi(3))));
                ui.label(format!("Área superficial: {}", px(6.0 * c.size.powi(2))));
            }
            _ => {
                ui.label("No hay mediciones disponibles");
            }
        }
    } else {
        ui.label("Ningún objeto seleccionado");
    }
}

/// A toolbar with icon buttons and keyboard shortcuts.
/// `is_3d` filters which tools are visible based on the current view mode.
pub fn toolbar(ui: &mut Ui, current_tool: &mut Tool, is_3d: bool) -> Response {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
    ui.horizontal_wrapped(|ui| {
        // Basic tools (work in both modes)
        tool_btn(ui, current_tool, Tool::Select, "Seleccionar", "F1");
        if !is_3d {
            tool_btn(ui, current_tool, Tool::Point, "Punto", "F2");
        }
        tool_btn(ui, current_tool, Tool::Line, "Recta", "F3");
        tool_btn(ui, current_tool, Tool::Circle, "Circunferencia", "F4");
        tool_btn(ui, current_tool, Tool::Polygon, "Polígono", "F5");
        tool_btn(ui, current_tool, Tool::Function, "Función", "F6");
        tool_btn(ui, current_tool, Tool::Pencil, "Lápiz", "");
        tool_btn(ui, current_tool, Tool::Eraser, "Borrador", "");

        ui.separator();

        // 3D-specific tools (only in 3D mode)
        if is_3d {
            tool_btn(ui, current_tool, Tool::Point3D, "Punto 3D", "F7");
            tool_btn(ui, current_tool, Tool::Sphere3D, "Esfera", "F8");
            tool_btn(ui, current_tool, Tool::Cube3D, "Cubo", "F9");
        }

        // Advanced tools (insert commands, work in both modes)
        tool_btn(ui, current_tool, Tool::Attractor, "Atractor", "");
        tool_btn(ui, current_tool, Tool::Fractal, "Fractal", "");

        // Visualization tools (only in 2D mode)
        if !is_3d {
            tool_btn(ui, current_tool, Tool::DomainColoring, "DomColor", "");
            tool_btn(ui, current_tool, Tool::HeatMap, "HeatMap", "");
            tool_btn(ui, current_tool, Tool::ComplexGrid, "CplxGrid", "");
        }

        // Statistics tools (only in 2D mode)
        if !is_3d {
            tool_btn(ui, current_tool, Tool::Histogram, "Histograma", "");
            tool_btn(ui, current_tool, Tool::ScatterPlot, "Dispersión", "");

            ui.separator();

            // Construction tools (only in 2D mode)
            tool_btn(ui, current_tool, Tool::Segment, "Segmento", "");
            tool_btn(ui, current_tool, Tool::Ray, "Semirrecta", "");
            tool_btn(ui, current_tool, Tool::Vector, "Vector", "");
            tool_btn(ui, current_tool, Tool::RegularPolygon, "PolígReg", "");
            tool_btn(ui, current_tool, Tool::Tangent, "Tangente", "");
            tool_btn(ui, current_tool, Tool::Perpendicular, "Perpendicular", "");

            ui.separator();

            // Analysis tools (only in 2D mode)
            tool_btn(ui, current_tool, Tool::Root, "Raíz", "R");
            tool_btn(ui, current_tool, Tool::Extremum, "Extremo", "E");
            tool_btn(ui, current_tool, Tool::Inflection, "Inflexión", "N");
            tool_btn(ui, current_tool, Tool::YIntercept, "InterY", "Ctrl+Shift+Y");
            tool_btn(ui, current_tool, Tool::XIntercept, "InterX", "I");
            tool_btn(ui, current_tool, Tool::Analyze, "Analizar", "Ctrl+A");

            ui.separator();

            // Curve creators (only in 2D mode)
            tool_btn(ui, current_tool, Tool::ParametricCurve2D, "Param2D", "");
            tool_btn(ui, current_tool, Tool::PolarCurve, "Polar", "");
            tool_btn(ui, current_tool, Tool::ImplicitCurve, "Implícita", "");
            tool_btn(ui, current_tool, Tool::VectorField2D, "CampoVec", "");

            ui.separator();

            // Numeric constraints (only in 2D mode)
            tool_btn(ui, current_tool, Tool::DistanceConstraint, "Distancia", "");
            tool_btn(ui, current_tool, Tool::AngleConstraint, "Ángulo", "");
            tool_btn(ui, current_tool, Tool::Coincident, "Coincidente", "");
            tool_btn(ui, current_tool, Tool::Horizontal, "Horizontal", "");
            tool_btn(ui, current_tool, Tool::Vertical, "Vertical", "");
            tool_btn(ui, current_tool, Tool::EqualLength, "LongIgual", "");
            tool_btn(ui, current_tool, Tool::Symmetry, "Simetría", "");

            ui.separator();

            // Conic constructions (only in 2D mode)
            tool_btn(ui, current_tool, Tool::EllipseByFoci, "Elipse", "");
            tool_btn(
                ui,
                current_tool,
                Tool::ParabolaByFocusDirectrix,
                "Parábola",
                "",
            );
            tool_btn(ui, current_tool, Tool::HyperbolaByFoci, "Hipérbola", "");
            tool_btn(ui, current_tool, Tool::ConicByFivePoints, "Cónica5", "");

            ui.separator();

            // Polygon boolean operations (only in 2D mode)
            tool_btn(ui, current_tool, Tool::PolygonUnion, "Unión", "");
            tool_btn(
                ui,
                current_tool,
                Tool::PolygonIntersection,
                "Intersección",
                "",
            );
            tool_btn(ui, current_tool, Tool::PolygonDifference, "Diferencia", "");
            tool_btn(ui, current_tool, Tool::PolygonXor, "Xor", "");
        }
    })
    .response
}

fn tool_btn(ui: &mut Ui, current: &mut Tool, tool: Tool, name: &str, _key: &str) -> egui::Response {
    let selected = *current == tool;
    let size = egui::vec2(44.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *current = tool;
    }
    if response.secondary_clicked() {
        *current = tool;
    }

    let visuals = if selected {
        ui.visuals().widgets.active
    } else if response.hovered() {
        ui.visuals().widgets.hovered
    } else {
        ui.visuals().widgets.inactive
    };

    let painter = ui.painter();

    if selected || response.hovered() {
        painter.rect_filled(rect, 8.0, visuals.bg_fill);
    }
    if selected {
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(1.5, ui.visuals().hyperlink_color),
        );
    }

    let text_color = if selected {
        ui.visuals().hyperlink_color
    } else {
        ui.visuals().text_color()
    };
    let stroke = egui::Stroke::new(2.0, text_color);
    let c = rect.center();

    match tool {
        Tool::Select => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "↖",
                egui::FontId::new(24.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Point => {
            painter.circle_filled(c, 4.0, text_color);
        }
        Tool::Line => {
            painter.line_segment(
                [c - egui::vec2(10.0, -10.0), c + egui::vec2(10.0, -10.0)],
                stroke,
            );
        }
        Tool::Circle => {
            painter.circle_stroke(c, 10.0, stroke);
            painter.circle_filled(c, 2.0, text_color);
        }
        Tool::Polygon => {
            let p1 = c - egui::vec2(10.0, -8.0);
            let p2 = c + egui::vec2(10.0, -8.0);
            let p3 = c + egui::vec2(0.0, 10.0);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
            painter.line_segment([p3, p1], stroke);
            painter.circle_filled(p1, 2.0, text_color);
            painter.circle_filled(p2, 2.0, text_color);
            painter.circle_filled(p3, 2.0, text_color);
        }
        Tool::Function => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "f(x)",
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Point3D => {
            painter.circle_filled(c, 4.0, text_color);
            painter.text(
                c + egui::vec2(6.0, -6.0),
                egui::Align2::CENTER_CENTER,
                "3",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Sphere3D => {
            painter.circle_stroke(c, 10.0, stroke);
            painter.circle_stroke(c, 6.0, egui::Stroke::new(1.0, text_color));
        }
        Tool::Cube3D => {
            let p1 = c - egui::vec2(8.0, 8.0);
            let p2 = c + egui::vec2(8.0, -8.0);
            let p3 = c + egui::vec2(8.0, 8.0);
            let p4 = c - egui::vec2(8.0, -8.0);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
            painter.line_segment([p3, p4], stroke);
            painter.line_segment([p4, p1], stroke);
            let offset = egui::vec2(4.0, -4.0);
            painter.line_segment([p1 + offset, p2 + offset], stroke);
            painter.line_segment([p2 + offset, p3 + offset], stroke);
            painter.line_segment([p3 + offset, p4 + offset], stroke);
            painter.line_segment([p4 + offset, p1 + offset], stroke);
            painter.line_segment([p1, p1 + offset], stroke);
            painter.line_segment([p2, p2 + offset], stroke);
        }
        Tool::Attractor => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "∞",
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Fractal => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "❄",
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Histogram => {
            let bar_width = 4.0;
            let heights = [8.0, 12.0, 6.0, 10.0];
            for (i, h) in heights.iter().enumerate() {
                let x = c.x - 8.0 + i as f32 * (bar_width + 1.0);
                let rect = egui::Rect::from_min_max(
                    egui::pos2(x, c.y + 8.0 - h),
                    egui::pos2(x + bar_width, c.y + 8.0),
                );
                painter.rect_filled(rect, 0.0, text_color);
            }
        }
        Tool::ScatterPlot => {
            let points = [
                egui::vec2(-8.0, -6.0),
                egui::vec2(-4.0, 4.0),
                egui::vec2(2.0, -2.0),
                egui::vec2(6.0, 6.0),
                egui::vec2(8.0, -4.0),
            ];
            for p in points {
                painter.circle_filled(c + p, 2.5, text_color);
            }
        }
        Tool::Tangent => {
            painter.circle_stroke(c, 8.0, stroke);
            let tangent_start = c + egui::vec2(-10.0, -6.0);
            let tangent_end = c + egui::vec2(10.0, 6.0);
            painter.line_segment([tangent_start, tangent_end], stroke);
        }
        Tool::Perpendicular => {
            painter.line_segment(
                [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [c - egui::vec2(0.0, -10.0), c + egui::vec2(0.0, 10.0)],
                stroke,
            );
        }
        Tool::Segment => {
            painter.line_segment(
                [c - egui::vec2(10.0, -6.0), c + egui::vec2(10.0, 6.0)],
                stroke,
            );
            painter.circle_filled(c - egui::vec2(10.0, -6.0), 2.0, text_color);
            painter.circle_filled(c + egui::vec2(10.0, 6.0), 2.0, text_color);
        }
        Tool::Ray => {
            painter.line_segment(
                [c - egui::vec2(8.0, 0.0), c + egui::vec2(10.0, 0.0)],
                stroke,
            );
            // arrowhead
            let tip = c + egui::vec2(10.0, 0.0);
            painter.line_segment([tip, tip - egui::vec2(5.0, 3.0)], stroke);
            painter.line_segment([tip, tip - egui::vec2(5.0, -3.0)], stroke);
            painter.circle_filled(c - egui::vec2(8.0, 0.0), 2.0, text_color);
        }
        Tool::Vector => {
            painter.line_segment(
                [c - egui::vec2(8.0, -6.0), c + egui::vec2(8.0, 6.0)],
                stroke,
            );
            let tip = c + egui::vec2(8.0, 6.0);
            painter.line_segment([tip, tip - egui::vec2(4.0, 2.0)], stroke);
            painter.line_segment([tip, tip - egui::vec2(2.0, 4.0)], stroke);
        }
        Tool::Pencil => {
            // Lápiz estilizado: cuerpo triangular + punta + trazo curvo.
            let body_top = c - egui::vec2(6.0, 6.0);
            let body_bot_l = c + egui::vec2(-3.0, 5.0);
            let body_bot_r = c + egui::vec2(3.0, 5.0);
            let tip = c + egui::vec2(6.0, 7.0);
            let stroke_w = egui::Stroke::new(1.5, text_color);
            painter.add(egui::Shape::convex_polygon(
                vec![body_top, body_bot_l, body_bot_r],
                Color32::TRANSPARENT,
                stroke_w,
            ));
            painter.line_segment([body_bot_l, tip], stroke_w);
            painter.line_segment([body_bot_r, tip], stroke_w);
            // Pequeño trazo ondulado a la derecha, como "escritura".
            let mut wave = Vec::with_capacity(10);
            for i in 0..=10 {
                let t = i as f32 / 10.0;
                let p = c + egui::vec2(8.0 + t * 6.0, (t * 9.0).sin() * 2.0);
                wave.push(p);
            }
            painter.add(egui::Shape::line(wave, egui::Stroke::new(1.5, text_color)));
        }
        Tool::RegularPolygon => {
            let center = c;
            let radius = 10.0;
            let n = 5;
            let points: Vec<egui::Pos2> = (0..=n)
                .map(|i| {
                    let angle =
                        i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    center + egui::vec2(radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            for i in 0..points.len().saturating_sub(1) {
                painter.line_segment([points[i], points[i + 1]], stroke);
            }
        }
        Tool::Coincident => {
            painter.circle_filled(c - egui::vec2(3.0, 0.0), 3.0, text_color);
            painter.circle_stroke(c + egui::vec2(3.0, 0.0), 3.0, stroke);
        }
        Tool::Horizontal => {
            painter.line_segment(
                [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
                stroke,
            );
        }
        Tool::Vertical => {
            painter.line_segment(
                [c - egui::vec2(0.0, -10.0), c + egui::vec2(0.0, 10.0)],
                stroke,
            );
        }
        Tool::EqualLength => {
            painter.line_segment(
                [c - egui::vec2(8.0, -4.0), c + egui::vec2(8.0, -4.0)],
                stroke,
            );
            painter.line_segment([c - egui::vec2(8.0, 4.0), c + egui::vec2(8.0, 4.0)], stroke);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "=",
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Symmetry => {
            painter.line_segment(
                [c - egui::vec2(0.0, -10.0), c + egui::vec2(0.0, 10.0)],
                stroke,
            );
            for dx in [-4.0f32, 4.0f32] {
                painter.circle_filled(c + egui::vec2(dx, 0.0), 2.5, text_color);
            }
        }
        Tool::EllipseByFoci => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "E",
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                text_color,
            );
            for dx in [-5.0f32, 5.0f32] {
                painter.circle_filled(c + egui::vec2(dx, 0.0), 2.0, text_color);
            }
        }
        Tool::ParabolaByFocusDirectrix => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "P",
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                text_color,
            );
            painter.line_segment([c - egui::vec2(8.0, 6.0), c + egui::vec2(8.0, 6.0)], stroke);
        }
        Tool::HyperbolaByFoci => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "H",
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                text_color,
            );
            for dx in [-5.0f32, 5.0f32] {
                painter.circle_filled(c + egui::vec2(dx, 0.0), 2.0, text_color);
            }
        }
        Tool::ConicByFivePoints => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "C5",
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::PolygonUnion => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "∪",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::PolygonIntersection => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "∩",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::PolygonDifference => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "\\",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::PolygonXor => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "⊕",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::DomainColoring => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "🌈",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::HeatMap => {
            let r1 =
                egui::Rect::from_min_max(c - egui::vec2(10.0, 8.0), c + egui::vec2(-2.0, -8.0));
            let r2 = egui::Rect::from_min_max(c - egui::vec2(0.0, 8.0), c + egui::vec2(8.0, -8.0));
            painter.rect_filled(r1, 0.0, Color32::from_rgb(50, 100, 200));
            painter.rect_filled(r2, 0.0, Color32::from_rgb(200, 50, 50));
        }
        Tool::ComplexGrid => {
            painter.circle_stroke(c, 10.0, stroke);
            painter.line_segment(
                [c - egui::vec2(12.0, 0.0), c + egui::vec2(12.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [c - egui::vec2(0.0, -12.0), c + egui::vec2(0.0, 12.0)],
                stroke,
            );
        }
        Tool::Locus => {
            painter.circle_filled(c, 2.0, text_color);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "⌒",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Midpoint => {
            painter.circle_filled(c, 3.0, text_color);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Distance => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "↔",
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Angle => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "∠",
                egui::FontId::new(20.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Area => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "⬜",
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Slope => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "m",
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Slider => {
            painter.line_segment(
                [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
                stroke,
            );
            painter.circle_filled(c + egui::vec2(4.0, 0.0), 4.0, text_color);
        }
        Tool::Button => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(16.0, 12.0)),
                3.0,
                stroke,
            );
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "OK",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Root => {
            painter.circle_filled(c - egui::vec2(6.0, 0.0), 3.0, text_color);
            painter.circle_filled(c + egui::vec2(6.0, 0.0), 3.0, text_color);
            painter.text(
                c - egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "x₀",
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Extremum => {
            painter.circle_filled(c - egui::vec2(0.0, 6.0), 3.0, text_color);
            painter.text(
                c + egui::vec2(0.0, 8.0),
                egui::Align2::CENTER_CENTER,
                "max",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Intersect => {
            painter.line_segment([c - egui::vec2(8.0, 8.0), c + egui::vec2(8.0, 8.0)], stroke);
            painter.line_segment(
                [c - egui::vec2(8.0, -8.0), c + egui::vec2(8.0, -8.0)],
                stroke,
            );
            painter.circle_filled(c, 3.0, text_color);
        }
        Tool::Inflection => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "I",
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::YIntercept => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "Y₀",
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::XIntercept => {
            painter.line_segment(
                [c - egui::vec2(10.0, 0.0), c + egui::vec2(10.0, 0.0)],
                stroke,
            );
            painter.circle_filled(c, 3.0, text_color);
            painter.text(
                c - egui::vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                "X₀",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Analyze => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::ParametricCurve2D => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "(t)",
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::PolarCurve => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "r(θ)",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::ImplicitCurve => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "f=0",
                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::VectorField2D => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "VF",
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
        Tool::Image => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(16.0, 12.0)),
                2.0,
                stroke,
            );
            painter.line_segment(
                [c - egui::vec2(4.0, -4.0), c + egui::vec2(4.0, 4.0)],
                stroke,
            );
        }
        Tool::Eraser => {
            // Goma de borrar: rectángulo oblicuo con esquina levantada.
            let body_a = c + egui::vec2(-7.0, 6.0);
            let body_b = c + egui::vec2(5.0, -6.0);
            let body_c = c + egui::vec2(8.0, -3.0);
            let body_d = c + egui::vec2(-4.0, 9.0);
            painter.line_segment([body_a, body_b], stroke);
            painter.line_segment([body_b, body_c], stroke);
            painter.line_segment([body_c, body_d], stroke);
            painter.line_segment([body_d, body_a], stroke);
            // Líneas decorativas (como "rozaduras" en una goma de verdad).
            painter.line_segment(
                [c + egui::vec2(-3.0, 2.0), c + egui::vec2(2.0, -3.0)],
                stroke,
            );
            painter.line_segment(
                [c + egui::vec2(-1.0, 4.0), c + egui::vec2(4.0, -1.0)],
                stroke,
            );
        }
        Tool::DistanceConstraint | Tool::AngleConstraint => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                match tool {
                    Tool::DistanceConstraint => "↔C",
                    _ => "∠C",
                },
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
                text_color,
            );
        }
    }

    response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(name).strong());
        ui.label(egui::RichText::new(format!("Atajo: {}", _key)).weak());
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Tool {
    // Basic 2D tools
    #[default]
    Select,
    Point,
    Line,
    Circle,
    Polygon,
    Pencil,
    Function,
    // 3D tools
    Point3D,
    Sphere3D,
    Cube3D,
    // Advanced tools
    Attractor,
    Fractal,
    Histogram,
    ScatterPlot,
    Root,
    Extremum,
    Inflection,
    YIntercept,
    XIntercept,
    Analyze,
    Intersect,
    // Curve creators
    ParametricCurve2D,
    PolarCurve,
    ImplicitCurve,
    VectorField2D,
    // Construction tools
    Segment,
    Ray,
    Vector,
    RegularPolygon,
    Tangent,
    Perpendicular,
    Locus,
    Midpoint,
    // Measurement tools
    Distance,
    Angle,
    Area,
    Slope,
    // Control tools
    Slider,
    Button,
    Image,
    Eraser,
    // Complex & Visualization
    DomainColoring,
    HeatMap,
    ComplexGrid,
    // Numeric constraints
    DistanceConstraint,
    AngleConstraint,
    Coincident,
    Horizontal,
    Vertical,
    EqualLength,
    Symmetry,
    // Conic constructions
    EllipseByFoci,
    ParabolaByFocusDirectrix,
    HyperbolaByFoci,
    ConicByFivePoints,
    // Polygon booleans
    PolygonUnion,
    PolygonIntersection,
    PolygonDifference,
    PolygonXor,
}

impl Tool {
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Point => "Point",
            Tool::Line => "Line",
            Tool::Circle => "Circle",
            Tool::Polygon => "Polygon",
            Tool::Pencil => "Pencil",
            Tool::Function => "Function",
            Tool::Point3D => "Point3D",
            Tool::Sphere3D => "Sphere3D",
            Tool::Cube3D => "Cube3D",
            Tool::Attractor => "Attractor",
            Tool::Fractal => "Fractal",
            Tool::Histogram => "Histogram",
            Tool::ScatterPlot => "ScatterPlot",
            Tool::Root => "Root",
            Tool::Extremum => "Extremum",
            Tool::Inflection => "Inflection",
            Tool::YIntercept => "YIntercept",
            Tool::XIntercept => "XIntercept",
            Tool::Analyze => "Analyze",
            Tool::Intersect => "Intersect",
            Tool::ParametricCurve2D => "ParametricCurve2D",
            Tool::PolarCurve => "PolarCurve",
            Tool::ImplicitCurve => "ImplicitCurve",
            Tool::VectorField2D => "VectorField2D",
            Tool::Segment => "Segment",
            Tool::Ray => "Ray",
            Tool::Vector => "Vector",
            Tool::RegularPolygon => "RegularPolygon",
            Tool::Tangent => "Tangent",
            Tool::Perpendicular => "Perpendicular",
            Tool::Locus => "Locus",
            Tool::Midpoint => "Midpoint",
            Tool::Distance => "Distance",
            Tool::Angle => "Angle",
            Tool::Area => "Area",
            Tool::Slope => "Slope",
            Tool::Slider => "Slider",
            Tool::Button => "Button",
            Tool::Image => "Image",
            Tool::DomainColoring => "DomainColoring",
            Tool::HeatMap => "HeatMap",
            Tool::ComplexGrid => "ComplexGrid",
            Tool::DistanceConstraint => "DistanceConstraint",
            Tool::AngleConstraint => "AngleConstraint",
            Tool::Coincident => "Coincident",
            Tool::Horizontal => "Horizontal",
            Tool::Vertical => "Vertical",
            Tool::EqualLength => "EqualLength",
            Tool::Symmetry => "Symmetry",
            Tool::EllipseByFoci => "EllipseByFoci",
            Tool::ParabolaByFocusDirectrix => "ParabolaByFocusDirectrix",
            Tool::HyperbolaByFoci => "HyperbolaByFoci",
            Tool::ConicByFivePoints => "ConicByFivePoints",
            Tool::PolygonUnion => "PolygonUnion",
            Tool::PolygonIntersection => "PolygonIntersection",
            Tool::PolygonDifference => "PolygonDifference",
            Tool::PolygonXor => "PolygonXor",
            Tool::Eraser => "Eraser",
        }
    }

    pub fn cursor_icon(&self) -> egui::CursorIcon {
        match self {
            Tool::Select => egui::CursorIcon::Default,
            Tool::Point | Tool::Point3D => egui::CursorIcon::Crosshair,
            Tool::Line
            | Tool::Circle
            | Tool::Polygon
            | Tool::Pencil
            | Tool::Function
            | Tool::Sphere3D
            | Tool::Cube3D
            | Tool::Attractor
            | Tool::Fractal
            | Tool::Histogram
            | Tool::ScatterPlot
            | Tool::Root
            | Tool::Extremum
            | Tool::Inflection
            | Tool::YIntercept
            | Tool::XIntercept
            | Tool::Analyze
            | Tool::Intersect
            | Tool::ParametricCurve2D
            | Tool::PolarCurve
            | Tool::ImplicitCurve
            | Tool::VectorField2D
            | Tool::Segment
            | Tool::Ray
            | Tool::Vector
            | Tool::RegularPolygon
            | Tool::Tangent
            | Tool::Perpendicular
            | Tool::Locus
            | Tool::Midpoint
            | Tool::Distance
            | Tool::Angle
            | Tool::Area
            | Tool::Slope
            | Tool::Slider
            | Tool::Button
            | Tool::Image
            | Tool::Eraser
            | Tool::DomainColoring
            | Tool::HeatMap
            | Tool::ComplexGrid
            | Tool::Coincident
            | Tool::Horizontal
            | Tool::Vertical
            | Tool::EqualLength
            | Tool::Symmetry
            | Tool::EllipseByFoci
            | Tool::ParabolaByFocusDirectrix
            | Tool::HyperbolaByFoci
            | Tool::ConicByFivePoints
            | Tool::PolygonUnion
            | Tool::PolygonIntersection
            | Tool::PolygonDifference
            | Tool::PolygonXor => egui::CursorIcon::Crosshair,
            Tool::DistanceConstraint | Tool::AngleConstraint => egui::CursorIcon::Crosshair,
        }
    }
}
