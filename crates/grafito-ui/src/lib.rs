//! Grafito UI — UI components and panels built with egui.

pub mod theme;
pub mod animation;
pub mod toast;
pub mod command_palette;

use grafito_core::{Document, ObjectId};
use egui::{Ui, Response};

/// Display the Algebra View panel listing all objects.
pub fn algebra_view(ui: &mut Ui, document: &Document, selected: &mut Option<ObjectId>) {
    ui.heading("Algebra");
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (id, obj) in document.objects_iter() {
            let label = format!("{}: {} {:?}", obj.label(), obj.name(), id);
            let is_selected = document.is_selected(*id);
            let response = ui.selectable_label(is_selected, label);
            if response.clicked() {
                *selected = Some(*id);
            }
        }
    });
}

/// Display the Properties panel for a selected object.
pub fn properties_panel(ui: &mut Ui, document: &mut Document, id: ObjectId) {
    ui.heading("Properties");
    ui.separator();
    if let Some(obj) = document.get_object_mut(id) {
        ui.label(format!("Type: {}", obj.name()));
        ui.label(format!("Label: {}", obj.label()));
        ui.checkbox(&mut true, "Visible");
        // Measurements
        fn px(val: f64) -> String { format!("{:.2}", val) }
        match obj {
            grafito_core::GeoObject::Line(l) => {
                ui.separator();
                ui.label(format!("Length: {}", px(l.start.distance(&l.end))));
            }
            grafito_core::GeoObject::Circle(c) => {
                ui.separator();
                ui.label(format!("Radius: {}", px(c.radius)));
                ui.label(format!("Area: {}", px(std::f64::consts::PI * c.radius * c.radius)));
                ui.label(format!("Circumference: {}", px(2.0 * std::f64::consts::PI * c.radius)));
            }
            grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                ui.separator();
                let mut perimeter = 0.0;
                for i in 0..poly.vertices.len() {
                    let a = poly.vertices[i];
                    let b = poly.vertices[(i+1)%poly.vertices.len()];
                    perimeter += a.distance(&b);
                }
                ui.label(format!("Perimeter: {}", px(perimeter)));
                // Shoelace area
                let mut area = 0.0;
                for i in 0..poly.vertices.len() {
                    let a = poly.vertices[i];
                    let b = poly.vertices[(i+1)%poly.vertices.len()];
                    area += a.x * b.y - b.x * a.y;
                }
                ui.label(format!("Area: {}", px(area.abs() * 0.5)));
            }
            grafito_core::GeoObject::Point3D(p) => {
                ui.separator();
                ui.label(format!("Pos: ({}, {}, {})", px(p.position.x), px(p.position.y), px(p.position.z)));
            }
            grafito_core::GeoObject::Sphere3D(s) => {
                ui.separator();
                ui.label(format!("Radius: {}", px(s.radius)));
                ui.label(format!("Volume: {}", px(4.0/3.0 * std::f64::consts::PI * s.radius.powi(3))));
                ui.label(format!("Surface Area: {}", px(4.0 * std::f64::consts::PI * s.radius.powi(2))));
            }
            _ => {}
        }
    } else {
        ui.label("No object selected");
    }
}

/// A toolbar with icon buttons and keyboard shortcuts.
pub fn toolbar(ui: &mut Ui, current_tool: &mut Tool) -> Response {
    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
    ui.horizontal(|ui| {
        tool_btn(ui, current_tool, Tool::Select,   "☝", "Select", "F1");
        tool_btn(ui, current_tool, Tool::Point,    "⊙", "Point", "F2");
        tool_btn(ui, current_tool, Tool::Line,     "╱", "Line", "F3");
        tool_btn(ui, current_tool, Tool::Circle,   "○", "Circle", "F4");
        tool_btn(ui, current_tool, Tool::Polygon,  "⬠", "Polygon", "F5");
        ui.separator();
        tool_btn(ui, current_tool, Tool::Function, "f(x)", "Function", "F6");
    }).response
}

fn tool_btn(ui: &mut Ui, current: &mut Tool, tool: Tool, icon: &str, name: &str, _key: &str) -> egui::Response {
    let selected = *current == tool;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(42.0, 28.0), egui::Sense::click());
    if response.clicked() { *current = tool; }
    if response.secondary_clicked() { *current = tool; }

    let visuals = if selected {
        ui.visuals().widgets.open
    } else if response.hovered() {
        ui.visuals().widgets.hovered
    } else {
        ui.visuals().widgets.inactive
    };

    let painter = ui.painter();
    painter.rect_filled(rect, visuals.rounding, visuals.weak_bg_fill);
    if selected || response.hovered() {
        painter.rect_stroke(rect, visuals.rounding, visuals.bg_stroke);
    }

    let font = egui::FontId::new(15.0, egui::FontFamily::Proportional);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, icon, font, visuals.fg_stroke.color);

    response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(name).strong());
        ui.label(egui::RichText::new(format!("Shortcut: {}", _key)).weak());
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Point,
    Line,
    Circle,
    Polygon,
    Function,
}

impl Default for Tool {
    fn default() -> Self {
        Tool::Select
    }
}

impl Tool {
    pub fn cursor_icon(&self) -> egui::CursorIcon {
        match self {
            Tool::Select => egui::CursorIcon::Default,
            _ => egui::CursorIcon::Crosshair,
        }
    }
}
