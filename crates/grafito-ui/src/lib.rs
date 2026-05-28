//! Grafito UI — UI components and panels built with egui.

pub mod theme;
pub mod animation;
pub mod toast;
pub mod command_palette;

use grafito_core::{Document, ObjectId};
use egui::{Ui, Response, Color32};

pub enum AlgebraAction {
    Delete(ObjectId),
    ToggleVisibility(ObjectId),
}

pub fn algebra_view(ui: &mut Ui, document: &mut Document, selected: &mut Option<ObjectId>) -> Vec<AlgebraAction> {
    let mut actions = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Collect object IDs first to avoid mutable borrow issues while iterating
        let object_ids: Vec<ObjectId> = document.objects_iter().map(|(id, _)| *id).collect();
        
        for id in object_ids {
            let is_selected = selected.map(|s| s == id).unwrap_or(false);
            
            // Outer frame for the item to give it nice hover effects and padding
            let mut frame = egui::Frame::default().inner_margin(egui::vec2(16.0, 12.0));
            if is_selected {
                frame.fill = if ui.visuals().dark_mode { Color32::from_gray(35) } else { Color32::from_rgb(245, 245, 250) };
            }
            
            let response = frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(obj) = document.get_object(id) {
                        let color = match obj.name() {
                            "Point" | "Point3D" => Color32::from_rgb(50, 100, 255),
                            "Line" => Color32::from_rgb(100, 110, 120),
                            "Function" => Color32::from_rgb(16, 185, 129),
                            "Circle" | "Ellipse" | "Sphere3D" | "Cube3D" | "Polygon" => Color32::from_rgb(239, 68, 68),
                            _ => Color32::GRAY,
                        };
                        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 5.0, color);
                        
                        ui.add_space(8.0);
                        let text = format!("{}: {}", obj.label(), obj.name());
                        ui.label(egui::RichText::new(text).size(15.0).color(if ui.visuals().dark_mode { Color32::WHITE } else { Color32::BLACK }));
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(egui::RichText::new("🗑").size(14.0)).frame(false)).on_hover_text("Delete object").clicked() {
                                actions.push(AlgebraAction::Delete(id));
                            }
                            let eye = if obj.is_visible() { "👁" } else { "Ø" };
                            if ui.add(egui::Button::new(egui::RichText::new(eye).size(14.0)).frame(false)).on_hover_text("Toggle visibility").clicked() {
                                actions.push(AlgebraAction::ToggleVisibility(id));
                            }
                        });
                    }
                });
                
                // Show properties inline if selected
                if is_selected {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("Properties").color(Color32::from_gray(120)).size(14.0));
                    ui.add_space(8.0);
                    
                    if let Some(obj) = document.get_object_mut(id) {
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(format!("Type: {}", obj.name())).size(13.0));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(format!("Label: {}", obj.label())).size(13.0));
                                ui.add_space(4.0);
                                let mut vis = obj.is_visible();
                                if ui.checkbox(&mut vis, egui::RichText::new("Visible").size(13.0)).changed() {
                                    obj.set_visible(vis);
                                }
                            });
                        });
                    }
                }
            }).response;
            
            // Add interaction to select the item
            let interact_resp = ui.interact(response.rect, ui.id().with(id), egui::Sense::click());
            if interact_resp.clicked() {
                if is_selected {
                    *selected = None;
                } else {
                    *selected = Some(id);
                }
            }
            
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                ui.painter().line_segment([rect.left_top(), rect.right_top()], egui::Stroke::new(1.0, Color32::from_gray(240)));
            });
        }
    });
    actions
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
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
    ui.horizontal(|ui| {
        tool_btn(ui, current_tool, Tool::Select,   "Select", "F1");
        tool_btn(ui, current_tool, Tool::Point,    "Point", "F2");
        tool_btn(ui, current_tool, Tool::Line,     "Line", "F3");
        tool_btn(ui, current_tool, Tool::Circle,   "Circle", "F4");
        tool_btn(ui, current_tool, Tool::Polygon,  "Polygon", "F5");
        ui.separator();
        tool_btn(ui, current_tool, Tool::Function, "Function", "F6");
    }).response
}

fn tool_btn(ui: &mut Ui, current: &mut Tool, tool: Tool, name: &str, _key: &str) -> egui::Response {
    let selected = *current == tool;
    let size = egui::vec2(44.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() { *current = tool; }
    if response.secondary_clicked() { *current = tool; }

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
        painter.rect_stroke(rect, 8.0, egui::Stroke::new(1.5, ui.visuals().hyperlink_color));
    }

    let text_color = if selected { ui.visuals().hyperlink_color } else { ui.visuals().text_color() };
    let stroke = egui::Stroke::new(2.0, text_color);
    let c = rect.center();
    
    match tool {
        Tool::Select => {
            painter.text(c, egui::Align2::CENTER_CENTER, "↖", egui::FontId::new(24.0, egui::FontFamily::Proportional), text_color);
        }
        Tool::Point => {
            painter.circle_filled(c, 4.0, text_color);
        }
        Tool::Line => {
            painter.line_segment([c - egui::vec2(10.0, -10.0), c + egui::vec2(10.0, -10.0)], stroke);
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
            painter.text(c, egui::Align2::CENTER_CENTER, "f(x)", egui::FontId::new(16.0, egui::FontFamily::Proportional), text_color);
        }
    }

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
