//! Centralized tool interaction dispatch.
//!
//! All tool behavior lives here instead of scattered across
//! render_2d.rs, render_3d.rs, lib.rs, bridge.rs, and dto.rs.
//! Adding a new tool only requires adding one arm here + one Tool variant.

use grafito_core::{Document, GeoObject, ObjectId};
use grafito_geometry::Point2;
use grafito_ui::Tool;

#[derive(Debug, Clone, Default)]
pub struct ToolState {
    pub pending: Vec<Point2>,
    pub driver: Option<ObjectId>,
    pub driven: Option<ObjectId>,
    pub measure_src: Option<ObjectId>,
    pub selection_rect: Option<(Point2, Point2)>,
}

impl ToolState {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.driver = None;
        self.driven = None;
        self.measure_src = None;
        self.selection_rect = None;
    }
}

pub struct ToolResult {
    pub objects: Vec<GeoObject>,
    pub message: Option<String>,
    pub reset_tool: bool,
}

pub fn dispatch_tool(
    tool: Tool,
    state: &mut ToolState,
    document: &mut Document,
    world: Point2,
) -> ToolResult {
    match tool {
        Tool::Select => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::Point => {
            let obj = GeoObject::Point(grafito_core::PointObj::new(world));
            ToolResult {
                objects: vec![obj],
                message: None,
                reset_tool: false,
            }
        }
        Tool::Line => handle_multi_point(state, world, 2, |pts| {
            GeoObject::Line(grafito_core::LineObj::new(pts[0], pts[1]))
        }),
        Tool::Circle => handle_multi_point(state, world, 2, |pts| {
            let r = pts[0].distance(&pts[1]);
            GeoObject::Circle(grafito_core::CircleObj::new(pts[0], r))
        }),
        Tool::Segment => handle_two_click_line(
            state,
            world,
            grafito_core::LineKind::Segment,
            "s",
            "Select start point",
            "Segment created",
        ),
        Tool::Ray => handle_two_click_line(
            state,
            world,
            grafito_core::LineKind::Ray,
            "r",
            "Select start point",
            "Ray created",
        ),
        Tool::Vector => handle_two_click_line(
            state,
            world,
            grafito_core::LineKind::Segment,
            "v",
            "Select start point",
            "Vector created",
        ),
        Tool::RegularPolygon => handle_regular_polygon(state, world),
        Tool::Polygon => handle_polygon(state, document, world),
        Tool::Function => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::Point3D => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::Sphere3D => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::Cube3D => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::Attractor => {
            let cmd = "Lorenz[]".to_string();
            let mut c = cmd;
            grafito_command::commands::process_input(document, &mut c);
            ToolResult {
                objects: vec![],
                message: Some("Lorenz attractor created".into()),
                reset_tool: true,
            }
        }
        Tool::Fractal => {
            let cmd = "Mandelbrot[]".to_string();
            let mut c = cmd;
            grafito_command::commands::process_input(document, &mut c);
            ToolResult {
                objects: vec![],
                message: Some("Mandelbrot fractal created".into()),
                reset_tool: true,
            }
        }
        Tool::Histogram => {
            let cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
            let mut c = cmd;
            grafito_command::commands::process_input(document, &mut c);
            ToolResult {
                objects: vec![],
                message: Some("Histogram created".into()),
                reset_tool: true,
            }
        }
        Tool::ScatterPlot => {
            let cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
            let mut c = cmd;
            grafito_command::commands::process_input(document, &mut c);
            ToolResult {
                objects: vec![],
                message: Some("Scatter plot created".into()),
                reset_tool: true,
            }
        }
        Tool::Tangent => handle_tangent(state, document, world),
        Tool::Perpendicular => handle_perpendicular(state, document, world),
        Tool::Locus => handle_locus(state, world),
        Tool::Distance => handle_measure(state, document, world, "Distance"),
        Tool::Angle => handle_measure(state, document, world, "Angle"),
        Tool::Area => handle_measure(state, document, world, "Area"),
        Tool::Slope => handle_measure(state, document, world, "Slope"),
        Tool::Midpoint => handle_multi_point(state, world, 2, |pts| {
            let mx = (pts[0].x + pts[1].x) * 0.5;
            let my = (pts[0].y + pts[1].y) * 0.5;
            GeoObject::Point(grafito_core::PointObj::new(Point2::new(mx, my)).with_label("M"))
        }),
        Tool::Slider => {
            // Crear slider: usa el sistema de variables + VariableMeta
            let name = format!("v{}", document.variables.len());
            document.set_variable(name.clone(), 0.0);
            document.variable_meta.insert(
                name.clone(),
                grafito_core::VariableMeta {
                    position: world,
                    min: -5.0,
                    max: 5.0,
                    step: 0.1,
                    visible: true,
                },
            );
            ToolResult {
                objects: vec![],
                message: Some(format!("Slider '{}' created", name)),
                reset_tool: true,
            }
        }
        Tool::Button => {
            let label = format!("btn{}", document.objects_iter().count());
            let obj = GeoObject::Text(grafito_core::TextObj::new(&label, world));
            ToolResult {
                objects: vec![obj],
                message: None,
                reset_tool: true,
            }
        }
        Tool::Image => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
        Tool::DomainColoring | Tool::HeatMap | Tool::ComplexGrid => {
            let cmd = match tool {
                Tool::DomainColoring => "DomainColoring[z^2+1, -2, 2, -2, 2]".to_string(),
                Tool::HeatMap => "HeatMap[sin(x)*cos(y), -3, 3, -3, 3]".to_string(),
                _ => "ComplexGrid[z^3-1, -2, 2, -2, 2]".to_string(),
            };
            let mut c = cmd;
            grafito_command::commands::process_input(document, &mut c);
            ToolResult {
                objects: vec![],
                message: Some("Visualization created".into()),
                reset_tool: true,
            }
        }
    }
}

fn handle_multi_point(
    state: &mut ToolState,
    world: Point2,
    needed: usize,
    create: fn(&[Point2]) -> GeoObject,
) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= needed {
        let pts = state.pending[..needed].to_vec();
        let obj = create(&pts);
        state.pending.clear();
        ToolResult {
            objects: vec![obj],
            message: None,
            reset_tool: false,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some(format!(
                "{}° point ({} of {})",
                needed,
                state.pending.len(),
                needed
            )),
            reset_tool: false,
        }
    }
}

fn handle_polygon(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 3 {
        let first = state.pending[0];
        let dist = world.distance(&first);
        let scale = document.view().scale;
        if dist < 20.0 / scale {
            let verts = state.pending.clone();
            state.pending.clear();
            let obj = GeoObject::Polygon(grafito_core::PolygonObj::new(verts));
            return ToolResult {
                objects: vec![obj],
                message: None,
                reset_tool: false,
            };
        }
    }
    ToolResult {
        objects: vec![],
        message: Some(format!("Point {} added", state.pending.len())),
        reset_tool: false,
    }
}

fn handle_locus(state: &mut ToolState, _world: Point2) -> ToolResult {
    if state.driver.is_none() {
        ToolResult {
            objects: vec![],
            message: Some("Select moving point".into()),
            reset_tool: false,
        }
    } else if state.driven.is_none() {
        ToolResult {
            objects: vec![],
            message: Some("Select dependent point".into()),
            reset_tool: false,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some("Locus computed".into()),
            reset_tool: true,
        }
    }
}

fn handle_measure(
    state: &mut ToolState,
    document: &mut Document,
    _world: Point2,
    measure_type: &str,
) -> ToolResult {
    state.pending.push(_world);
    match measure_type {
        "Distance" if state.pending.len() == 2 => {
            let a = state.pending[0];
            let b = state.pending[1];
            let d = a.distance(&b);
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            // Visual: dotted measurement line
            let mut line = grafito_core::LineObj::new(a, b);
            line.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.9);
            line.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(line));
            // Label with distance value
            let txt = grafito_core::TextObj::new(format!("{:.3}", d), mid);
            document.add_object(grafito_core::GeoObject::Text(txt));
            state.pending.clear();
            ToolResult {
                objects: vec![],
                message: Some(format!("Distance = {:.3}", d)),
                reset_tool: false,
            }
        }
        "Angle" if state.pending.len() == 3 => {
            let a = state.pending[0];
            let b = state.pending[1];
            let c = state.pending[2];
            // Visual: two rays from vertex
            let mut ray1 = grafito_core::LineObj::new_with_kind(b, a, grafito_core::LineKind::Ray);
            ray1.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.7);
            ray1.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(ray1));
            let mut ray2 = grafito_core::LineObj::new_with_kind(b, c, grafito_core::LineKind::Ray);
            ray2.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.7);
            ray2.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(ray2));
            // Compute angle
            let v1 = (a.x - b.x, a.y - b.y);
            let v2 = (c.x - b.x, c.y - b.y);
            let dot = v1.0 * v2.0 + v1.1 * v2.1;
            let m1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
            let m2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();
            let angle = if m1 < 1e-12 || m2 < 1e-12 {
                0.0
            } else {
                (dot / (m1 * m2)).acos().to_degrees()
            };
            // Label at vertex offset
            let lbl_pos = Point2::new(b.x + 0.3, b.y + 0.3);
            let txt = grafito_core::TextObj::new(format!("{:.1}°", angle), lbl_pos);
            document.add_object(grafito_core::GeoObject::Text(txt));
            state.pending.clear();
            ToolResult {
                objects: vec![],
                message: Some(format!("Angle = {:.1}°", angle)),
                reset_tool: false,
            }
        }
        "Area" if state.pending.len() == 1 => {
            // Area: click on a polygon or circle
            let tolerance = 10.0 / document.view().scale;
            if let Some(id) = document.pick_object(state.pending[0], tolerance) {
                let obj = document.get_object(id).cloned();
                if let Some(obj) = obj {
                    let (area, center) = match &obj {
                        grafito_core::GeoObject::Circle(c) => {
                            (std::f64::consts::PI * c.radius * c.radius, c.center)
                        }
                        grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                            let a = polygon_area(&poly.vertices);
                            let cx = poly.vertices.iter().map(|v| v.x).sum::<f64>()
                                / poly.vertices.len() as f64;
                            let cy = poly.vertices.iter().map(|v| v.y).sum::<f64>()
                                / poly.vertices.len() as f64;
                            (a, Point2::new(cx, cy))
                        }
                        _ => (0.0, state.pending[0]),
                    };
                    if area > 0.0 {
                        let txt = grafito_core::TextObj::new(format!("Area = {:.3}", area), center);
                        document.add_object(grafito_core::GeoObject::Text(txt));
                        state.pending.clear();
                        return ToolResult {
                            objects: vec![],
                            message: Some(format!("Area = {:.3}", area)),
                            reset_tool: true,
                        };
                    }
                }
            }
            ToolResult {
                objects: vec![],
                message: Some("Click on a polygon or circle".into()),
                reset_tool: false,
            }
        }
        "Slope" if state.pending.len() == 1 => {
            let tolerance = 10.0 / document.view().scale;
            if let Some(id) = document.pick_object(state.pending[0], tolerance) {
                let obj = document.get_object(id).cloned();
                if let Some(grafito_core::GeoObject::Line(l)) = obj {
                    let slope = if (l.end.x - l.start.x).abs() < 1e-12 {
                        f64::INFINITY
                    } else {
                        (l.end.y - l.start.y) / (l.end.x - l.start.x)
                    };
                    let mid = Point2::new(
                        (l.start.x + l.end.x) * 0.5,
                        (l.start.y + l.end.y) * 0.5 + 0.3,
                    );
                    let s = if slope.is_infinite() {
                        "∞".to_string()
                    } else {
                        format!("{:.3}", slope)
                    };
                    let txt = grafito_core::TextObj::new(format!("m = {}", s), mid);
                    document.add_object(grafito_core::GeoObject::Text(txt));
                    state.pending.clear();
                    return ToolResult {
                        objects: vec![],
                        message: Some(format!("Slope = {}", s)),
                        reset_tool: true,
                    };
                }
            }
            ToolResult {
                objects: vec![],
                message: Some("Click on a line".into()),
                reset_tool: false,
            }
        }
        _ => ToolResult {
            objects: vec![],
            reset_tool: false,
            message: Some(format!(
                "Click {} point(s)",
                if state.pending.len() == 1 {
                    "2nd"
                } else if measure_type == "Angle" {
                    "3rd"
                } else {
                    "on object"
                }
            )),
        },
    }
}

fn polygon_area(vertices: &[Point2]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = vertices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i].x * vertices[j].y;
        area -= vertices[j].x * vertices[i].y;
    }
    area.abs() / 2.0
}

fn handle_tangent(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 3 {
        let pts = state.pending[..3].to_vec();
        let r = pts[0].distance(&pts[1]);
        let cmd = format!(
            "Tangent[({:.2},{:.2}), {:.3}, ({:.2},{:.2})]",
            pts[0].x, pts[0].y, r, pts[2].x, pts[2].y
        );
        let mut c = cmd;
        grafito_command::commands::process_input(document, &mut c);
        state.pending.clear();
        ToolResult {
            objects: vec![],
            message: Some("Tangents created".into()),
            reset_tool: true,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some(format!("{}° point", state.pending.len() + 1)),
            reset_tool: false,
        }
    }
}

fn handle_perpendicular(
    state: &mut ToolState,
    document: &mut Document,
    world: Point2,
) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let pts = state.pending[..2].to_vec();
        let cmd = format!(
            "PerpendicularBisector[({:.2},{:.2}), ({:.2},{:.2})]",
            pts[0].x, pts[0].y, pts[1].x, pts[1].y
        );
        let mut c = cmd;
        grafito_command::commands::process_input(document, &mut c);
        state.pending.clear();
        ToolResult {
            objects: vec![],
            message: Some("Perpendicular bisector created".into()),
            reset_tool: true,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some("Select 2nd point".into()),
            reset_tool: false,
        }
    }
}

fn handle_two_click_line(
    state: &mut ToolState,
    world: Point2,
    kind: grafito_core::LineKind,
    label: &str,
    first_msg: &str,
    done_msg: &str,
) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let pts = state.pending[..2].to_vec();
        state.pending.clear();
        let obj = GeoObject::Line(
            grafito_core::LineObj::new_with_kind(pts[0], pts[1], kind).with_label(label),
        );
        ToolResult {
            objects: vec![obj],
            message: Some(done_msg.into()),
            reset_tool: false,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some(first_msg.into()),
            reset_tool: false,
        }
    }
}

fn handle_regular_polygon(state: &mut ToolState, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let center = state.pending[0];
        let vertex = state.pending[1];
        let r = center.distance(&vertex);
        let n = 5;
        let start_angle = (vertex.y - center.y).atan2(vertex.x - center.x);
        let verts: Vec<Point2> = (0..n)
            .map(|i| {
                let angle = start_angle + i as f64 / n as f64 * std::f64::consts::TAU;
                Point2::new(center.x + r * angle.cos(), center.y + r * angle.sin())
            })
            .collect();
        state.pending.clear();
        ToolResult {
            objects: vec![GeoObject::Polygon(grafito_core::PolygonObj::new(verts))],
            message: Some("Regular polygon created".into()),
            reset_tool: false,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some("Select center".into()),
            reset_tool: false,
        }
    }
}
