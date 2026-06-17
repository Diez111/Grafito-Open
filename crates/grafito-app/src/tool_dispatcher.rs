//! Centralized tool interaction dispatch.
//!
//! All tool behavior lives here instead of scattered across
//! render_2d.rs, render_3d.rs, lib.rs, bridge.rs, and dto.rs.
//! Adding a new tool only requires adding one arm here + one Tool variant.

use grafito_command::commands::CommandOutcome;
use grafito_core::{Document, GeoObject, ObjectId};
use grafito_geometry::Point2;
use grafito_ui::Tool;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ToolState {
    pub pending: Vec<Point2>,
    pub driver: Option<ObjectId>,
    pub driven: Option<ObjectId>,
    pub measure_src: Option<ObjectId>,
    pub selection_rect: Option<(Point2, Point2)>,
    pub last_outcome: Option<CommandOutcome>,
    /// ID del último objeto borrado por la herramienta Eraser durante
    /// el arrastre actual. Evita borrar dos veces el mismo objeto en
    /// un solo trazo y permite deshacer todo el trazo en una sola acción.
    pub last_erased: Option<ObjectId>,
    /// ID del PencilObj que se está dibujando actualmente. Se establece
    /// en `drag_started` y se actualiza en cada tick del drag. Al soltar,
    /// si solo tiene 1 punto, se elimina (no es un trazo válido).
    pub drawing_pencil: Option<ObjectId>,
}

#[allow(dead_code)]
impl ToolState {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.driver = None;
        self.driven = None;
        self.measure_src = None;
        self.selection_rect = None;
        self.last_outcome = None;
        self.last_erased = None;
        self.drawing_pencil = None;
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
            state.last_outcome = Some(grafito_command::commands::process_input(document, &mut c));
            ToolResult {
                objects: vec![],
                message: Some("Lorenz attractor created".into()),
                reset_tool: true,
            }
        }
        Tool::Fractal => {
            let cmd = "Mandelbrot[]".to_string();
            let mut c = cmd;
            state.last_outcome = Some(grafito_command::commands::process_input(document, &mut c));
            ToolResult {
                objects: vec![],
                message: Some("Mandelbrot fractal created".into()),
                reset_tool: true,
            }
        }
        Tool::Histogram => {
            let cmd = "Histogram[{1,2,3,4,5,6,4,3,2,5,3,4,3}, 5]".to_string();
            let mut c = cmd;
            state.last_outcome = Some(grafito_command::commands::process_input(document, &mut c));
            ToolResult {
                objects: vec![],
                message: Some("Histogram created".into()),
                reset_tool: true,
            }
        }
        Tool::ScatterPlot => {
            let cmd = "ScatterPlot[{1,2,3,4,5}, {2,3,5,7,11}]".to_string();
            let mut c = cmd;
            state.last_outcome = Some(grafito_command::commands::process_input(document, &mut c));
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
            let mut idx = document.variables.len();
            let mut name = format!("v{}", idx);
            while document.variables.contains_key(&name) {
                idx += 1;
                name = format!("v{}", idx);
            }
            document.set_variable(name.clone(), 0.0);
            document.variable_meta.insert(
                name.clone(),
                grafito_core::VariableMeta {
                    position: world,
                    min: -5.0,
                    max: 5.0,
                    step: 0.1,
                    visible: true,
                    animating: false,
                    animation_speed: 1.0,
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
        Tool::Image => {
            // Abrimos un file dialog de forma asíncrona (rfd).
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Imagen", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                .pick_file()
            {
                let path_str = path.to_string_lossy().to_string();
                let mut c = format!("Image[{}]", path_str);
                let outcome = grafito_command::commands::process_input(document, &mut c);
                state.last_outcome = Some(outcome);
                ToolResult {
                    objects: vec![],
                    message: Some(format!("Imagen '{}' cargada", path_str)),
                    reset_tool: true,
                }
            } else {
                ToolResult {
                    objects: vec![],
                    message: Some("Selección cancelada".into()),
                    reset_tool: false,
                }
            }
        }
        Tool::DomainColoring | Tool::HeatMap | Tool::ComplexGrid => {
            let cmd = match tool {
                Tool::DomainColoring => "DomainColoring[z^2+1, -2, 2, -2, 2]".to_string(),
                Tool::HeatMap => "HeatMap[sin(x)*cos(y), -3, 3, -3, 3]".to_string(),
                _ => "ComplexGrid[z^3-1, -2, 2, -2, 2]".to_string(),
            };
            let mut c = cmd;
            state.last_outcome = Some(grafito_command::commands::process_input(document, &mut c));
            ToolResult {
                objects: vec![],
                message: Some("Visualization created".into()),
                reset_tool: true,
            }
        }
        Tool::Root
        | Tool::Extremum
        | Tool::Inflection
        | Tool::YIntercept
        | Tool::XIntercept
        | Tool::Analyze => {
            let tolerance = 10.0 / document.view().scale;
            if let Some(id) = document.pick_object(world, tolerance) {
                if let Some(obj) = document.get_object(id) {
                    let label = obj.label().to_string();
                    let cmd = match tool {
                        Tool::Root => format!("Root[{}]", label),
                        Tool::Extremum => format!("Extremum[{}]", label),
                        Tool::Inflection => format!("Inflection[{}]", label),
                        Tool::YIntercept => format!("YIntercept[{}]", label),
                        Tool::XIntercept => format!("XIntercept[{}]", label),
                        Tool::Analyze => format!("Analyze[{}]", label),
                        _ => {
                            return ToolResult {
                                objects: vec![],
                                message: Some("Herramienta no soportada para análisis".into()),
                                reset_tool: true,
                            }
                        }
                    };
                    let mut c = cmd;
                    let outcome = grafito_command::commands::process_input(document, &mut c);
                    state.last_outcome = Some(outcome);
                    return ToolResult {
                        objects: vec![],
                        message: Some(format!("Analizado: {}", label)),
                        reset_tool: true,
                    };
                }
            }
            ToolResult {
                objects: vec![],
                message: Some("Selecciona una función o curva".into()),
                reset_tool: false,
            }
        }
        Tool::Intersect => {
            let tolerance = 10.0 / document.view().scale;
            if let Some(id) = document.pick_object(world, tolerance) {
                if state.driver.is_none() {
                    state.driver = Some(id);
                    return ToolResult {
                        objects: vec![],
                        message: Some("Selecciona el segundo objeto".into()),
                        reset_tool: false,
                    };
                } else if state.driver != Some(id) {
                    if let Some(id1) = state.driver.take() {
                        let id2 = id;
                        let l1 = document
                            .get_object(id1)
                            .map(|o| o.label().to_string())
                            .unwrap_or_default();
                        let l2 = document
                            .get_object(id2)
                            .map(|o| o.label().to_string())
                            .unwrap_or_default();
                        let mut c = format!("Intersect[{}, {}]", l1, l2);
                        let outcome = grafito_command::commands::process_input(document, &mut c);
                        state.last_outcome = Some(outcome);
                        return ToolResult {
                            objects: vec![],
                            message: Some("Intersección calculada".into()),
                            reset_tool: true,
                        };
                    }
                }
            }
            ToolResult {
                objects: vec![],
                message: Some(
                    if state.driver.is_none() {
                        "Selecciona primer objeto"
                    } else {
                        "Selecciona segundo objeto"
                    }
                    .into(),
                ),
                reset_tool: false,
            }
        }
        Tool::ParametricCurve2D => {
            let mut c = "ParametricCurve2D[cos(t), sin(t), 0, 2*pi]".to_string();
            let outcome = grafito_command::commands::process_input(document, &mut c);
            state.last_outcome = Some(outcome);
            ToolResult {
                objects: vec![],
                message: Some("Curva paramétrica creada".into()),
                reset_tool: true,
            }
        }
        Tool::PolarCurve => {
            let mut c = "PolarCurve[1 - cos(t), 0, 2*pi]".to_string();
            let outcome = grafito_command::commands::process_input(document, &mut c);
            state.last_outcome = Some(outcome);
            ToolResult {
                objects: vec![],
                message: Some("Curva polar creada".into()),
                reset_tool: true,
            }
        }
        Tool::ImplicitCurve => {
            let mut c = format!(
                "ImplicitCurve[(x - {:.2})^2 + (y - {:.2})^2 = 4]",
                world.x, world.y
            );
            let outcome = grafito_command::commands::process_input(document, &mut c);
            state.last_outcome = Some(outcome);
            ToolResult {
                objects: vec![],
                message: Some("Curva implícita creada".into()),
                reset_tool: true,
            }
        }
        Tool::VectorField2D => {
            let mut c = "VectorField2D[x, y]".to_string();
            let outcome = grafito_command::commands::process_input(document, &mut c);
            state.last_outcome = Some(outcome);
            ToolResult {
                objects: vec![],
                message: Some("Campo vectorial creado".into()),
                reset_tool: true,
            }
        }
        _ => ToolResult {
            objects: vec![],
            message: None,
            reset_tool: false,
        },
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
    world: Point2,
    measure_type: &str,
) -> ToolResult {
    // Si el clic fue sobre un objeto existente, lo guardamos también para
    // poder hacer medidas polimórficas.
    let tolerance = 10.0 / document.view().scale;
    let picked = document.pick_object(world, tolerance);
    state.pending.push(world);

    let picked_some = picked.is_some();
    match measure_type {
        "Distance" if state.pending.len() == 2 => {
            let a = state.pending[0];
            let b = state.pending[1];
            let d = a.distance(&b);
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let mut line = grafito_core::LineObj::new(a, b);
            line.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.9);
            line.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(line));
            let txt = grafito_core::TextObj::new(format!("{:.3}", d), mid);
            document.add_object(grafito_core::GeoObject::Text(txt));
            state.pending.clear();
            ToolResult {
                objects: vec![],
                message: Some(format!(
                    "Distancia = {:.3} (entre puntos{})",
                    d,
                    if picked_some { " sobre objetos" } else { "" }
                )),
                reset_tool: false,
            }
        }
        "Angle" if state.pending.len() == 3 => {
            // Vértice = clic 1; brazo 1 = clic 2; brazo 2 = clic 3.
            // Si el clic 2/3 fue sobre una Line existente, usamos la dirección
            // de esa línea en lugar del punto crudo, dando ángulos correctos
            // cuando el usuario quiere medir entre rectas.
            let vertex = state.pending[0];
            let arm1 = resolve_arm(document, state.pending[1]);
            let arm2 = resolve_arm(document, state.pending[2]);
            let mut ray1 = grafito_core::LineObj::new_with_kind(
                vertex,
                Point2::new(
                    vertex.x + (arm1.x - vertex.x) * 10.0,
                    vertex.y + (arm1.y - vertex.y) * 10.0,
                ),
                grafito_core::LineKind::Ray,
            );
            ray1.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.7);
            ray1.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(ray1));
            let mut ray2 = grafito_core::LineObj::new_with_kind(
                vertex,
                Point2::new(
                    vertex.x + (arm2.x - vertex.x) * 10.0,
                    vertex.y + (arm2.y - vertex.y) * 10.0,
                ),
                grafito_core::LineKind::Ray,
            );
            ray2.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.7);
            ray2.width = 1.5;
            document.add_object(grafito_core::GeoObject::Line(ray2));
            let v1x = arm1.x - vertex.x;
            let v1y = arm1.y - vertex.y;
            let v2x = arm2.x - vertex.x;
            let v2y = arm2.y - vertex.y;
            let dot = v1x * v2x + v1y * v2y;
            let m1 = (v1x * v1x + v1y * v1y).sqrt();
            let m2 = (v2x * v2x + v2y * v2y).sqrt();
            let angle = if m1 < 1e-12 || m2 < 1e-12 {
                0.0
            } else {
                (dot / (m1 * m2)).clamp(-1.0, 1.0).acos().to_degrees()
            };
            // Arco visual: polígono en forma de sector entre los dos rayos
            let theta1 = v1y.atan2(v1x);
            let theta2 = v2y.atan2(v2x);
            let arc_r = ((m1 + m2) * 0.1).clamp(0.5, 2.0);
            let n = 32;
            let mut verts = Vec::with_capacity(n + 2);
            verts.push(vertex);
            // Ir de theta1 a theta2 en el sentido corto
            let mut dt = theta2 - theta1;
            while dt > std::f64::consts::PI {
                dt -= 2.0 * std::f64::consts::PI;
            }
            while dt < -std::f64::consts::PI {
                dt += 2.0 * std::f64::consts::PI;
            }
            for k in 0..=n {
                let t = theta1 + dt * (k as f64) / (n as f64);
                verts.push(Point2::new(
                    vertex.x + arc_r * t.cos(),
                    vertex.y + arc_r * t.sin(),
                ));
            }
            let mut arc_poly = grafito_core::PolygonObj::new(verts);
            arc_poly.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 1.0);
            arc_poly.width = 1.0;
            arc_poly.fill_color = Some(grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.25));
            arc_poly.label = String::new();
            document.add_object(grafito_core::GeoObject::Polygon(arc_poly));
            let lbl_pos = Point2::new(
                vertex.x + arc_r * 0.7 * (theta1 + dt * 0.5).cos(),
                vertex.y + arc_r * 0.7 * (theta1 + dt * 0.5).sin(),
            );
            let txt = grafito_core::TextObj::new(format!("{:.1}°", angle), lbl_pos);
            document.add_object(grafito_core::GeoObject::Text(txt));
            state.pending.clear();
            ToolResult {
                objects: vec![],
                message: Some(format!("Ángulo = {:.1}°", angle)),
                reset_tool: false,
            }
        }
        "Area" => {
            if state.pending.len() == 1 {
                let p1 = state.pending[0];
                if let Some(id) = document.pick_object(p1, tolerance) {
                    if let Some(obj) = document.get_object(id).cloned() {
                        let (area, label, fill_polygon) = match &obj {
                            grafito_core::GeoObject::Circle(c) => {
                                let n = 64;
                                let mut verts = Vec::with_capacity(n);
                                for k in 0..n {
                                    let theta =
                                        2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                                    verts.push(Point2::new(
                                        c.center.x + c.radius * theta.cos(),
                                        c.center.y + c.radius * theta.sin(),
                                    ));
                                }
                                let a = std::f64::consts::PI * c.radius * c.radius;
                                (a, format!("A = {:.3}", a), Some(verts))
                            }
                            grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                                let a = polygon_area(&poly.vertices);
                                (a, format!("A = {:.3}", a), Some(poly.vertices.clone()))
                            }
                            _ => (0.0, String::new(), None),
                        };

                        if area > 0.0 {
                            if let Some(verts) = fill_polygon {
                                let n = verts.len() as f64;
                                let cx = verts.iter().map(|v| v.x).sum::<f64>() / n;
                                let cy = verts.iter().map(|v| v.y).sum::<f64>() / n;
                                let mut fill_poly = grafito_core::PolygonObj::new(verts);
                                fill_poly.color = grafito_geometry::Color::new(0.2, 0.5, 0.9, 1.0);
                                fill_poly.width = 1.5;
                                fill_poly.fill_color =
                                    Some(grafito_geometry::Color::new(0.2, 0.5, 0.9, 0.3));
                                fill_poly.label = String::new();
                                document.add_object(grafito_core::GeoObject::Polygon(fill_poly));
                                let txt =
                                    grafito_core::TextObj::new(label.clone(), Point2::new(cx, cy));
                                document.add_object(grafito_core::GeoObject::Text(txt));
                            }
                            state.pending.clear();
                            return ToolResult {
                                objects: vec![],
                                message: Some(label),
                                reset_tool: false,
                            };
                        }
                    }
                }
                return ToolResult {
                    objects: vec![],
                    message: Some(
                        "Selecciona un círculo o polígono, o dos puntos sobre una función".into(),
                    ),
                    reset_tool: false,
                };
            } else if state.pending.len() == 2 {
                let p1 = state.pending[0];
                let p2 = state.pending[1];
                if let Some(id) = document.pick_object(p1, tolerance) {
                    if let Some(grafito_core::GeoObject::Function(f)) =
                        document.get_object(id).cloned()
                    {
                        let lo = p1.x.min(p2.x);
                        let hi = p1.x.max(p2.x);
                        let integral = grafito_geometry::integral::eval_integral_hybrid(
                            |x| {
                                grafito_geometry::expr::eval_function_with_vars(
                                    &f.expr,
                                    x,
                                    &document.variables,
                                )
                                .unwrap_or(0.0)
                            },
                            lo,
                            hi,
                            200,
                        );
                        let a = integral.abs();
                        let n = 80;
                        let mut verts = Vec::with_capacity(n + 2);
                        for k in 0..=n {
                            let x = lo + (hi - lo) * (k as f64) / (n as f64);
                            let y = grafito_geometry::expr::eval_function_with_vars(
                                &f.expr,
                                x,
                                &document.variables,
                            )
                            .unwrap_or(0.0);
                            verts.push(Point2::new(x, y));
                        }
                        verts.push(Point2::new(hi, 0.0));
                        verts.push(Point2::new(lo, 0.0));
                        let mut fill_poly = grafito_core::PolygonObj::new(verts);
                        fill_poly.color = grafito_geometry::Color::new(0.2, 0.5, 0.9, 1.0);
                        fill_poly.width = 1.5;
                        fill_poly.fill_color =
                            Some(grafito_geometry::Color::new(0.2, 0.5, 0.9, 0.3));
                        fill_poly.label = String::new();
                        document.add_object(grafito_core::GeoObject::Polygon(fill_poly));
                        let label = format!("A = {:.3}", a);
                        let txt = grafito_core::TextObj::new(
                            label.clone(),
                            Point2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5),
                        );
                        document.add_object(grafito_core::GeoObject::Text(txt));
                        state.pending.clear();
                        return ToolResult {
                            objects: vec![],
                            message: Some(label),
                            reset_tool: false,
                        };
                    }
                }
                state.pending.clear();
                return ToolResult {
                    objects: vec![],
                    message: Some("Se requiere una función para integral".into()),
                    reset_tool: false,
                };
            }
            // state.pending solo puede tener 1 o 2 puntos después del push;
            // los dos casos anteriores ya retornan, así que este punto es
            // inalcanzable, pero lo dejamos para que el compilador no se queje.
            ToolResult {
                objects: vec![],
                message: Some("Selecciona un objeto o dos puntos para área".into()),
                reset_tool: false,
            }
        }
        "Slope" if state.pending.len() == 1 => {
            // Pendiente en el punto: si clic fue sobre Line, m; si fue sobre
            // Function, derivada numérica.
            if let Some(id) = document.pick_object(world, tolerance) {
                let obj = document.get_object(id).cloned();
                if let Some(obj) = obj {
                    match &obj {
                        grafito_core::GeoObject::Line(l) => {
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
                                message: Some(format!("Pendiente = {}", s)),
                                reset_tool: true,
                            };
                        }
                        grafito_core::GeoObject::Function(f) => {
                            // Derivada numérica con paso adaptativo.
                            let h = (world.x.abs().max(1.0) * 1e-5).max(1e-12);
                            let f1 = grafito_geometry::expr::eval_function_with_vars(
                                &f.expr,
                                world.x - h,
                                &document.variables,
                            )
                            .unwrap_or(f64::NAN);
                            let f2 = grafito_geometry::expr::eval_function_with_vars(
                                &f.expr,
                                world.x + h,
                                &document.variables,
                            )
                            .unwrap_or(f64::NAN);
                            let slope = if f1.is_finite() && f2.is_finite() {
                                (f2 - f1) / (2.0 * h)
                            } else {
                                f64::NAN
                            };
                            let s = if slope.is_finite() {
                                format!("{:.3}", slope)
                            } else {
                                "∞".to_string()
                            };
                            let txt = grafito_core::TextObj::new(
                                format!("f'({:.2}) = {}", world.x, s),
                                Point2::new(world.x + 0.3, world.y + 0.3),
                            );
                            document.add_object(grafito_core::GeoObject::Text(txt));
                            state.pending.clear();
                            return ToolResult {
                                objects: vec![],
                                message: Some(format!("f'({:.2}) = {}", world.x, s)),
                                reset_tool: true,
                            };
                        }
                        _ => {}
                    }
                }
            }
            state.pending.clear();
            ToolResult {
                objects: vec![],
                message: Some("Clic sobre Line o Function".into()),
                reset_tool: false,
            }
        }
        _ => ToolResult {
            objects: vec![],
            reset_tool: false,
            message: Some(match measure_type {
                "Distance" => "Clic 2do punto".into(),
                "Angle" => "Clic sobre el segundo brazo".into(),
                "Slope" => "Clic sobre Line o Function".into(),
                _ => "Clic siguiente".into(),
            }),
        },
    }
}

/// Resuelve un clic a un "brazo" del ángulo: si el clic está sobre una
/// `Line` existente, devuelve el extremo más cercano de la línea, dando así
/// un ángulo correcto entre dos rectas. Si no, devuelve el punto crudo.
fn resolve_arm(document: &mut Document, click: Point2) -> Point2 {
    let tolerance = 10.0 / document.view().scale;
    if let Some(id) = document.pick_object(click, tolerance) {
        if let Some(grafito_core::GeoObject::Line(l)) = document.get_object(id) {
            let d_start = l.start.distance(&click);
            let d_end = l.end.distance(&click);
            return if d_start < d_end { l.start } else { l.end };
        }
    }
    click
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
