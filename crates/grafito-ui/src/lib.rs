//! Grafito UI — UI components and panels built with egui.

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
        ui.checkbox(&mut true, "Visible"); // Placeholder
    } else {
        ui.label("No object selected");
    }
}

/// A simple toolbar with tool buttons.
pub fn toolbar(ui: &mut Ui, current_tool: &mut Tool) -> Response {
    ui.horizontal(|ui| {
        ui.selectable_value(current_tool, Tool::Select, "Select");
        ui.selectable_value(current_tool, Tool::Point, "Point");
        ui.selectable_value(current_tool, Tool::Line, "Line");
        ui.selectable_value(current_tool, Tool::Circle, "Circle");
        ui.selectable_value(current_tool, Tool::Polygon, "Polygon");
        ui.selectable_value(current_tool, Tool::Function, "Function");
    }).response
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
