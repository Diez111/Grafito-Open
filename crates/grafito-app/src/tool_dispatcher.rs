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
    /// Lados del polígono regular (memoria de sesión). `resolve_polygon_sides`
    /// lo combina con la variable `n` del documento; 0 = sin preferencia.
    pub polygon_sides: u32,
}

/// Lados mínimos/máximos para el polígono regular.
pub const MIN_POLYGON_SIDES: u32 = 3;
/// Lados máximos para el polígono regular.
pub const MAX_POLYGON_SIDES: u32 = 64;
/// Default documentado cuando no hay variable `n` ni memoria de sesión.
pub const DEFAULT_POLYGON_SIDES: u32 = 5;
/// Variable del documento que parametriza `n` cuando existe como slider.
pub const POLYGON_SIDES_VARIABLE: &str = "n";

/// Resuelve el `n` del polígono regular sin diálogos (Piel pura):
/// 1. variable `n` del documento (slider) si es entera y está en rango;
/// 2. `state.polygon_sides` (último uso de la sesión) si está en rango;
/// 3. [`DEFAULT_POLYGON_SIDES`] como primer uso documentado.
#[must_use]
pub fn resolve_polygon_sides(state: &ToolState, document: &Document) -> u32 {
    if let Some(value) = document.get_variable(POLYGON_SIDES_VARIABLE) {
        if value.is_finite() {
            let rounded = value.round();
            if (value - rounded).abs() <= 1e-9
                && rounded >= f64::from(MIN_POLYGON_SIDES)
                && rounded <= f64::from(MAX_POLYGON_SIDES)
            {
                return rounded as u32;
            }
        }
    }
    if (MIN_POLYGON_SIDES..=MAX_POLYGON_SIDES).contains(&state.polygon_sides) {
        state.polygon_sides
    } else {
        DEFAULT_POLYGON_SIDES
    }
}

/// Valida y memoriza `n` en la sesión; fuera de 3..=64 devuelve error honesto sin mutar.
pub fn set_polygon_sides(state: &mut ToolState, n: u32) -> Result<(), String> {
    if (MIN_POLYGON_SIDES..=MAX_POLYGON_SIDES).contains(&n) {
        state.polygon_sides = n;
        Ok(())
    } else {
        Err(format!(
            "lados {n} fuera de rango {MIN_POLYGON_SIDES}..={MAX_POLYGON_SIDES}"
        ))
    }
}

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
        Tool::Line => handle_two_click_command(state, document, world, "Line", "Recta creada"),
        Tool::Circle => {
            handle_two_click_command(state, document, world, "Circle", "Círculo creado")
        }
        Tool::Segment => handle_two_click_line(
            state,
            document,
            world,
            "Segment",
            "Select start point",
            "Segment created",
        ),
        Tool::Ray => handle_two_click_line(
            state,
            document,
            world,
            "Ray",
            "Select start point",
            "Ray created",
        ),
        Tool::Vector => handle_two_click_line(
            state,
            document,
            world,
            "Vector",
            "Select start point",
            "Vector created",
        ),
        Tool::RegularPolygon => handle_regular_polygon(state, document, world),
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
        Tool::Parallel => handle_parallel(state, document, world),
        Tool::Reflect => handle_reflect(state, document, world),
        Tool::Rotate => handle_rotate(state, document, world),
        Tool::Translate => handle_translate(state, document, world),
        Tool::Dilate => handle_dilate(state, document, world),
        Tool::Compass => handle_compass(state, document, world),
        Tool::Semicircle => handle_semicircle(state, document, world),
        Tool::Spline => handle_spline(state, document, world),
        Tool::Prism3D => handle_prism(state, document, world),
        Tool::Tetrahedron3D => handle_tetrahedron(state, document, world),
        Tool::Checkbox => handle_action_box(state, document, "Checkbox", "Casilla", "casilla"),
        Tool::InputBox => handle_action_box(state, document, "InputBox", "Entrada", "entrada"),
        Tool::Arc => handle_arc(state, document, world),
        Tool::Sector => handle_sector(state, document, world),
        Tool::Locus => handle_locus(state, document, world),
        Tool::Distance => handle_measure(state, document, world, "Distance"),
        Tool::Angle => handle_measure(state, document, world, "Angle"),
        Tool::Area => handle_measure(state, document, world, "Area"),
        Tool::Slope => handle_measure(state, document, world, "Slope"),
        Tool::Midpoint => {
            handle_two_click_command(state, document, world, "Midpoint", "Punto medio creado")
        }
        Tool::Slider => {
            // Crear slider: usa el sistema de variables + VariableMeta
            let mut idx = document.variables.len();
            let mut name = format!("v{}", idx);
            while document.variables.contains_key(&name) {
                idx += 1;
                name = format!("v{}", idx);
            }
            let metadata = grafito_core::VariableMeta {
                position: world,
                min: -5.0,
                max: 5.0,
                step: 0.1,
                visible: true,
                animating: false,
                animation_speed: 1.0,
                animation_mode: grafito_core::AnimationMode::PingPong,
            };
            if !metadata.position.x.is_finite() || !metadata.position.y.is_finite() {
                state.last_outcome = Some(CommandOutcome::Error(
                    "Slider position must be finite".to_string(),
                ));
                return ToolResult {
                    objects: vec![],
                    message: None,
                    reset_tool: true,
                };
            }
            if let Err(error) = document.try_set_variable(name.clone(), 0.0) {
                state.last_outcome = Some(CommandOutcome::Error(error));
                return ToolResult {
                    objects: vec![],
                    message: None,
                    reset_tool: true,
                };
            }
            if let Err(error) =
                document.try_replace_variable_meta_with_previous(&name, metadata)
            {
                state.last_outcome = Some(CommandOutcome::Error(error));
                return ToolResult {
                    objects: vec![],
                    message: None,
                    reset_tool: true,
                };
            }
            ToolResult {
                objects: vec![],
                message: Some(format!("Slider '{}' created", name)),
                reset_tool: true,
            }
        }
        Tool::Button => unavailable_tool(
            state,
            "Button no está disponible: Grafito aún no tiene un modelo persistente de botón interactivo.",
        ),
        Tool::Image => unavailable_tool(
            state,
            "Image no está disponible: Grafito aún no tiene un modelo persistente de imagen en el documento.",
        ),
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
        _ => unavailable_tool(
            state,
            &format!(
                "La herramienta {tool:?} aún no está disponible desde el lienzo; usá la paleta (Ctrl+K) o el comando CAS."
            ),
        ),
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

fn unavailable_tool(state: &mut ToolState, message: &str) -> ToolResult {
    state.clear();
    state.last_outcome = Some(CommandOutcome::Error(message.to_string()));
    ToolResult {
        objects: vec![],
        message: None,
        reset_tool: true,
    }
}

fn handle_locus(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let tolerance = 10.0 / document.view().scale;
    let Some(selected) = document.pick_object(world, tolerance) else {
        return ToolResult {
            objects: vec![],
            message: Some(
                if state.driver.is_some() {
                    "Selecciona el punto objetivo del lugar geometrico"
                } else {
                    "Selecciona el punto driver del lugar geometrico"
                }
                .to_string(),
            ),
            reset_tool: false,
        };
    };
    if !matches!(document.get_object(selected), Some(GeoObject::Point(_))) {
        return ToolResult {
            objects: vec![],
            message: Some("Locus requiere seleccionar puntos".to_string()),
            reset_tool: false,
        };
    }
    let Some(driver) = state.driver else {
        state.driver = Some(selected);
        return ToolResult {
            objects: vec![],
            message: Some("Selecciona el punto objetivo del lugar geometrico".to_string()),
            reset_tool: false,
        };
    };
    if driver == selected {
        return ToolResult {
            objects: vec![],
            message: Some("Locus requiere dos puntos distintos".to_string()),
            reset_tool: false,
        };
    }

    state.driver = None;
    match document.try_add_locus(driver, selected) {
        Ok((locus, _)) => {
            let label = document
                .get_object(locus)
                .map(|object| object.label().to_string())
                .unwrap_or_else(|| "Locus".to_string());
            state.last_outcome = Some(CommandOutcome::Message(format!(
                "{label}: lugar geometrico creado"
            )));
            ToolResult {
                objects: vec![],
                message: Some("Lugar geometrico creado".to_string()),
                reset_tool: true,
            }
        }
        Err(error) => {
            state.last_outcome = Some(CommandOutcome::Error(error));
            ToolResult {
                objects: vec![],
                message: None,
                reset_tool: true,
            }
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
            // W-B: sobre puntos etiquetados → medida viva (`MeasureDistance`,
            // sigue al arrastre); si no, línea+texto congelados honestos.
            if let (Some(first), Some(second)) =
                (point_label_at(document, a), point_label_at(document, b))
            {
                state.pending.clear();
                return finish_with_command(
                    state,
                    document,
                    format!("MeasureDistance[{first}, {second}]"),
                    format!("Distancia viva entre {first} y {second}"),
                    false,
                );
            }
            let d = a.distance(&b);
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let mut line = grafito_core::LineObj::new(a, b);
            line.color = grafito_geometry::Color::new(0.8, 0.4, 0.0, 0.9);
            line.width = 1.5;
            let txt = grafito_core::TextObj::new(format!("{:.3}", d), mid);
            state.pending.clear();
            ToolResult {
                objects: vec![
                    grafito_core::GeoObject::Line(line),
                    grafito_core::GeoObject::Text(txt),
                ],
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
            let lbl_pos = Point2::new(
                vertex.x + arc_r * 0.7 * (theta1 + dt * 0.5).cos(),
                vertex.y + arc_r * 0.7 * (theta1 + dt * 0.5).sin(),
            );
            let txt = grafito_core::TextObj::new(format!("{:.1}°", angle), lbl_pos);
            state.pending.clear();
            ToolResult {
                objects: vec![
                    grafito_core::GeoObject::Line(ray1),
                    grafito_core::GeoObject::Line(ray2),
                    grafito_core::GeoObject::Polygon(arc_poly),
                    grafito_core::GeoObject::Text(txt),
                ],
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
                                let txt =
                                    grafito_core::TextObj::new(label.clone(), Point2::new(cx, cy));
                                state.pending.clear();
                                return ToolResult {
                                    objects: vec![
                                        grafito_core::GeoObject::Polygon(fill_poly),
                                        grafito_core::GeoObject::Text(txt),
                                    ],
                                    message: Some(label),
                                    reset_tool: false,
                                };
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
                        let label = format!("A = {:.3}", a);
                        let txt = grafito_core::TextObj::new(
                            label.clone(),
                            Point2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5),
                        );
                        state.pending.clear();
                        return ToolResult {
                            objects: vec![
                                grafito_core::GeoObject::Polygon(fill_poly),
                                grafito_core::GeoObject::Text(txt),
                            ],
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
                                "inf".to_string()
                            } else {
                                format!("{:.3}", slope)
                            };
                            let txt = grafito_core::TextObj::new(format!("m = {}", s), mid);
                            state.pending.clear();
                            return ToolResult {
                                objects: vec![grafito_core::GeoObject::Text(txt)],
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
                                "inf".to_string()
                            };
                            let txt = grafito_core::TextObj::new(
                                format!("f'({:.2}) = {}", world.x, s),
                                Point2::new(world.x + 0.3, world.y + 0.3),
                            );
                            state.pending.clear();
                            return ToolResult {
                                objects: vec![grafito_core::GeoObject::Text(txt)],
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
        // GeoGebra: si los clics caen sobre punto+recta existentes, usa etiquetas.
        let cmd =
            perpendicular_parallel_command(document, &pts, "Perpendicular").unwrap_or_else(|| {
                format!(
                    "PerpendicularBisector[({:.2},{:.2}), ({:.2},{:.2})]",
                    pts[0].x, pts[0].y, pts[1].x, pts[1].y
                )
            });
        let mut c = cmd;
        grafito_command::commands::process_input(document, &mut c);
        state.pending.clear();
        ToolResult {
            objects: vec![],
            message: Some("Perpendicular creada".into()),
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

/// Resuelve `Comando[punto, recta]` por etiquetas si los clics caen sobre un
/// punto y una recta existentes (cualquier orden). `None` si no hay ese par:
/// el llamador usa su fallback honesto (mediatriz o guía).
fn perpendicular_parallel_command(
    document: &mut Document,
    pts: &[Point2],
    cmd_name: &str,
) -> Option<String> {
    if pts.len() < 2 {
        return None;
    }
    let tol = 10.0 / document.view().scale;
    let classify = |obj: &GeoObject| {
        (
            obj.label().to_owned(),
            matches!(obj, GeoObject::Point(_)),
            matches!(obj, GeoObject::Line(_)),
        )
    };
    let mut point_label: Option<String> = None;
    let mut line_label: Option<String> = None;
    for p in pts.iter().take(2) {
        let hit = document
            .pick_object(*p, tol)
            .and_then(|id| document.get_object(id).map(classify));
        if let Some((label, is_point, is_line)) = hit {
            if is_point && point_label.is_none() {
                point_label = Some(label.clone());
            }
            if is_line && line_label.is_none() {
                line_label = Some(label);
            }
        }
    }
    match (point_label, line_label) {
        (Some(p), Some(l)) => Some(format!("{cmd_name}[{p}, {l}]")),
        _ => None,
    }
}

fn handle_parallel(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let pts = state.pending[..2].to_vec();
        match perpendicular_parallel_command(document, &pts, "Parallel") {
            Some(cmd) => {
                let mut c = cmd;
                grafito_command::commands::process_input(document, &mut c);
                state.pending.clear();
                ToolResult {
                    objects: vec![],
                    message: Some("Paralela creada".into()),
                    reset_tool: true,
                }
            }
            None => {
                state.pending.clear();
                ToolResult {
                    objects: vec![],
                    message: Some(
                        "Paralela necesita un punto y una recta existentes: clic en cada uno"
                            .into(),
                    ),
                    reset_tool: false,
                }
            }
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some("Select 2nd point".into()),
            reset_tool: false,
        }
    }
}

fn handle_arc(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 3 {
        let pts = state.pending[..3].to_vec();
        // Tres puntos libres: el motor resuelve centro/radio/ángulos o erra honesto.
        let cmd = format!(
            "Arc[({:.2},{:.2}),({:.2},{:.2}),({:.2},{:.2})]",
            pts[0].x, pts[0].y, pts[1].x, pts[1].y, pts[2].x, pts[2].y
        );
        let mut c = cmd;
        grafito_command::commands::process_input(document, &mut c);
        state.pending.clear();
        ToolResult {
            objects: vec![],
            message: Some("Arco creado (o error honesto si colineales)".into()),
            reset_tool: true,
        }
    } else {
        ToolResult {
            objects: vec![],
            message: Some(format!("Select point {}/3", state.pending.len() + 1)),
            reset_tool: false,
        }
    }
}

fn handle_sector(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 3 {
        let pts = state.pending[..3].to_vec();
        let radius = pts[0].distance(&pts[1]);
        let start_deg = (pts[1].y - pts[0].y)
            .atan2(pts[1].x - pts[0].x)
            .to_degrees();
        let mut end_deg = (pts[2].y - pts[0].y)
            .atan2(pts[2].x - pts[0].x)
            .to_degrees();
        if end_deg <= start_deg {
            end_deg += 360.0;
        }
        let cmd = format!(
            "Sector[({:.2},{:.2}),{:.2},{:.2},{:.2}]",
            pts[0].x, pts[0].y, radius, start_deg, end_deg
        );
        let mut c = cmd;
        grafito_command::commands::process_input(document, &mut c);
        state.pending.clear();
        ToolResult {
            objects: vec![],
            message: Some("Sector creado".into()),
            reset_tool: true,
        }
    } else {
        let hints = ["Select center", "Select radius point", "Select end angle"];
        ToolResult {
            objects: vec![],
            message: Some(hints[state.pending.len().min(2)].into()),
            reset_tool: false,
        }
    }
}

/// W-B: dos clics encaminados a `Comando[arg0, arg1]` con `point_arg`
/// (etiqueta si el clic cae sobre un punto existente, literal si no). El
/// comando crea construcción paramétrica viva o libre congelado; el
/// dispatcher no decide, sólo encamina. Nunca muta a mano.
fn handle_two_click_command(
    state: &mut ToolState,
    document: &mut Document,
    world: Point2,
    command: &str,
    done_msg: &str,
) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let pts = state.pending[..2].to_vec();
        let cmd = format!(
            "{command}[{}, {}]",
            point_arg(document, pts[0]),
            point_arg(document, pts[1])
        );
        return finish_with_command(state, document, cmd, done_msg.into(), false);
    }
    ToolResult {
        objects: vec![],
        message: Some("Select 2nd point".into()),
        reset_tool: false,
    }
}

fn handle_two_click_line(
    state: &mut ToolState,
    document: &mut Document,
    world: Point2,
    command: &str,
    first_msg: &str,
    done_msg: &str,
) -> ToolResult {
    // W-B: mismo encaminamiento que `handle_two_click_command`, pero conserva
    // el hint del primer clic propio de cada herramienta.
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let pts = state.pending[..2].to_vec();
        let cmd = format!(
            "{command}[{}, {}]",
            point_arg(document, pts[0]),
            point_arg(document, pts[1])
        );
        return finish_with_command(state, document, cmd, done_msg.into(), false);
    }
    ToolResult {
        objects: vec![],
        message: Some(first_msg.into()),
        reset_tool: false,
    }
}

fn handle_regular_polygon(state: &mut ToolState, document: &Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() >= 2 {
        let center = state.pending[0];
        let vertex = state.pending[1];
        let r = center.distance(&vertex);
        // n paramétrico: variable `n` (slider) > memoria de sesión > 5.
        // Se memoriza lo resuelto para que la preferencia sobreviva aunque
        // la variable `n` se borre después.
        let n = resolve_polygon_sides(state, document);
        let _ = set_polygon_sides(state, n);
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

/// Guía sin mutar: la herramienta sigue activa esperando el próximo clic.
fn tool_hint(message: &str) -> ToolResult {
    ToolResult {
        objects: vec![],
        message: Some(message.to_string()),
        reset_tool: false,
    }
}

/// Error honesto con limpieza: avisa, guarda el outcome y deja la herramienta
/// activa para reintentar desde cero (sin `pending`/`driver` colgados).
fn tool_honest_reset(state: &mut ToolState, message: &str) -> ToolResult {
    state.driver = None;
    state.pending.clear();
    state.last_outcome = Some(CommandOutcome::Error(message.to_string()));
    ToolResult {
        objects: vec![],
        message: Some(message.to_string()),
        reset_tool: false,
    }
}

/// Ejecuta un comando CAS desde una herramienta y limpia el flujo.
/// El mensaje prioriza el del motor (éxito o error honesto); `success` es el
/// fallback cuando el motor devuelve `Ok` pelado.
fn finish_with_command(
    state: &mut ToolState,
    document: &mut Document,
    cmd: String,
    success: String,
    reset_tool: bool,
) -> ToolResult {
    let mut input = cmd;
    let outcome = grafito_command::commands::process_input(document, &mut input);
    let message = match &outcome {
        CommandOutcome::Ok => success,
        CommandOutcome::Message(text) => text.clone(),
        CommandOutcome::Error(error) => error.clone(),
    };
    state.driver = None;
    state.pending.clear();
    state.last_outcome = Some(outcome);
    ToolResult {
        objects: vec![],
        message: Some(message),
        reset_tool,
    }
}

/// Etiqueta no vacía del punto existente bajo el clic, si lo hay.
/// `pub(crate)` para que `input.rs` encamine clics a comandos paramétricos.
pub(crate) fn point_label_at(document: &mut Document, world: Point2) -> Option<String> {
    let tolerance = 10.0 / document.view().scale;
    let id = document.pick_object(world, tolerance)?;
    match document.get_object(id) {
        Some(GeoObject::Point(point)) if !point.label.trim().is_empty() => {
            Some(point.label.clone())
        }
        _ => None,
    }
}

/// Argumento punto para comandos: etiqueta si el clic cae sobre un punto
/// existente (preserva construcción paramétrica), literal `(x, y)` si no.
/// `pub(crate)` para que `input.rs` encamine clics a comandos paramétricos.
pub(crate) fn point_arg(document: &mut Document, world: Point2) -> String {
    point_label_at(document, world).unwrap_or_else(|| format!("({:.2},{:.2})", world.x, world.y))
}

/// Etiqueta no vacía del objeto bajo el clic si es de un tipo admitido.
fn labeled_source_at(
    document: &mut Document,
    world: Point2,
    admitted: fn(&GeoObject) -> bool,
) -> Option<String> {
    let tolerance = 10.0 / document.view().scale;
    let id = document.pick_object(world, tolerance)?;
    let obj = document.get_object(id)?;
    if admitted(obj) && !obj.label().trim().is_empty() {
        Some(obj.label().to_string())
    } else {
        None
    }
}

fn is_point_object(obj: &GeoObject) -> bool {
    matches!(obj, GeoObject::Point(_))
}

fn is_reflectable(obj: &GeoObject) -> bool {
    matches!(
        obj,
        GeoObject::Point(_) | GeoObject::Line(_) | GeoObject::Circle(_) | GeoObject::Polygon(_)
    )
}

/// Clic-1 de las transformaciones puntuales: si no hay fuente, intenta tomar
/// el punto bajo el clic y devuelve `Ok(None)` (clic consumido: pedir el
/// próximo paso). Si ya hay fuente, devuelve `Ok(Some(etiqueta))` y el
/// llamador trata el clic actual como primer parámetro.
fn take_point_driver(
    state: &mut ToolState,
    document: &mut Document,
    world: Point2,
    tool_name: &str,
) -> Result<Option<String>, ToolResult> {
    if state.driver.is_none() {
        match labeled_source_at(document, world, is_point_object) {
            Some(_) => {
                let tolerance = 10.0 / document.view().scale;
                state.driver = document.pick_object(world, tolerance);
                return Ok(None);
            }
            None => {
                return Err(tool_hint(&format!(
                    "{tool_name}: clic sobre el punto a transformar (el motor solo admite puntos)"
                )));
            }
        }
    }
    let Some(id) = state.driver else {
        return Err(tool_honest_reset(
            state,
            &format!("{tool_name}: se perdió la selección; reintentá"),
        ));
    };
    match document.get_object(id) {
        Some(GeoObject::Point(point)) if !point.label.trim().is_empty() => {
            Ok(Some(point.label.clone()))
        }
        _ => Err(tool_honest_reset(
            state,
            &format!("{tool_name}: se perdió la selección; reintentá"),
        )),
    }
}

fn handle_reflect(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let tolerance = 10.0 / document.view().scale;
    if state.driver.is_none() {
        match labeled_source_at(document, world, is_reflectable) {
            Some(_) => {
                state.driver = document.pick_object(world, tolerance);
            }
            None => {
                return tool_hint("Refleja: clic sobre un punto, recta, círculo o polígono");
            }
        }
        return tool_hint(
            "Refleja: clic en el primer punto del eje (repetilo para simetría central)",
        );
    }
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Refleja: clic en el segundo punto del eje");
    }
    let axis = [state.pending[0], state.pending[1]];
    let Some(src_id) = state.driver else {
        return tool_honest_reset(state, "Refleja: se perdió la selección; reintentá");
    };
    let src_label = document
        .get_object(src_id)
        .map(|obj| obj.label().to_string())
        .unwrap_or_default();
    if src_label.trim().is_empty() {
        return tool_honest_reset(state, "Refleja: el objeto perdió su etiqueta; reintentá");
    }
    // Eje degenerado = simetría central (equivale a rotar 180°). El motor
    // Rotate solo admite puntos: otro objeto da error honesto, no botón mudo.
    if axis[0].distance(&axis[1]) < tolerance {
        if !matches!(document.get_object(src_id), Some(GeoObject::Point(_))) {
            return tool_honest_reset(
                state,
                "Refleja: la simetría central solo admite puntos (la axial admite punto, recta, círculo o polígono)",
            );
        }
        let center = point_arg(document, axis[0]);
        return finish_with_command(
            state,
            document,
            format!("Rotate[{src_label}, {center}, 180]"),
            format!("{src_label} reflejado (simetría central)"),
            false,
        );
    }
    let axis_a = point_arg(document, axis[0]);
    let axis_b = point_arg(document, axis[1]);
    finish_with_command(
        state,
        document,
        format!("Reflect[{src_label}, {axis_a}, {axis_b}]"),
        format!("{src_label} reflejado"),
        false,
    )
}

fn handle_rotate(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let src_label = match take_point_driver(state, document, world, "Rota") {
        Ok(None) => return tool_hint("Rota: clic en el centro de rotación"),
        Ok(Some(label)) => label,
        Err(guide) => return guide,
    };
    // El clic actual es el centro; el próximo define el ángulo.
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Rota: clic para definir el ángulo (polar respecto al centro)");
    }
    let center = state.pending[0];
    let angle_at = state.pending[1];
    let angle = (angle_at.y - center.y)
        .atan2(angle_at.x - center.x)
        .to_degrees();
    if !angle.is_finite() {
        return tool_honest_reset(state, "Rota: el ángulo no es finito; reintentá");
    }
    let center_arg = point_arg(document, center);
    finish_with_command(
        state,
        document,
        format!("Rotate[{src_label}, {center_arg}, {angle:.2}]"),
        format!("{src_label} rotado {angle:.1}°"),
        false,
    )
}

fn handle_translate(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let src_label = match take_point_driver(state, document, world, "Traslada") {
        Ok(None) => return tool_hint("Traslada: clic en el inicio del vector"),
        Ok(Some(label)) => label,
        Err(guide) => return guide,
    };
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Traslada: clic en el extremo del vector");
    }
    let start = state.pending[0];
    let end = state.pending[1];
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if dx.hypot(dy) <= 1e-9 {
        return tool_honest_reset(state, "Traslada: vector nulo; elegí dos puntos distintos");
    }
    finish_with_command(
        state,
        document,
        format!("Translate[{src_label}, ({dx:.4}, {dy:.4})]"),
        format!("{src_label} trasladado ({dx:.2}, {dy:.2})"),
        false,
    )
}

fn handle_dilate(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let src_label = match take_point_driver(state, document, world, "Homotecia") {
        Ok(None) => return tool_hint("Homotecia: clic en el centro"),
        Ok(Some(label)) => label,
        Err(guide) => return guide,
    };
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Homotecia: clic donde debe caer la imagen (define el factor)");
    }
    let center = state.pending[0];
    let image_at = state.pending[1];
    let Some(src_id) = state.driver else {
        return tool_honest_reset(state, "Homotecia: se perdió la selección; reintentá");
    };
    let Some(GeoObject::Point(src)) = document.get_object(src_id) else {
        return tool_honest_reset(state, "Homotecia: se perdió la selección; reintentá");
    };
    let base = center.distance(&src.position);
    if base <= 1e-9 {
        return tool_honest_reset(
            state,
            "Homotecia: el punto coincide con el centro (factor indefinido)",
        );
    }
    let factor = center.distance(&image_at) / base;
    if !factor.is_finite() {
        return tool_honest_reset(state, "Homotecia: el factor no es finito; reintentá");
    }
    let center_arg = point_arg(document, center);
    finish_with_command(
        state,
        document,
        format!("Dilate[{src_label}, {factor:.6}, {center_arg}]"),
        format!("{src_label} homotecia k={factor:.3}"),
        false,
    )
}

fn handle_compass(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Compás: clic en el punto del radio");
    }
    let center = state.pending[0];
    let edge = state.pending[1];
    let radius = center.distance(&edge);
    if !radius.is_finite() || radius <= 1e-9 {
        state.pending.clear();
        return tool_hint("Compás: elegí dos puntos distintos");
    }
    finish_with_command(
        state,
        document,
        format!(
            "Compasses[({:.2},{:.2}), ({:.2},{:.2})]",
            center.x, center.y, edge.x, edge.y
        ),
        format!("Compás: círculo r={radius:.3}"),
        false,
    )
}

fn handle_semicircle(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    state.pending.push(world);
    if state.pending.len() < 2 {
        return tool_hint("Semicírculo: clic en el punto del radio");
    }
    let center = state.pending[0];
    let edge = state.pending[1];
    let radius = center.distance(&edge);
    if !radius.is_finite() || radius <= 1e-9 {
        state.pending.clear();
        return tool_hint("Semicírculo: elegí dos puntos distintos");
    }
    finish_with_command(
        state,
        document,
        format!("Semicircle[({:.2},{:.2}), {radius:.3}]", center.x, center.y),
        format!("Semicírculo r={radius:.3}"),
        false,
    )
}

/// Puntos máximos por Spline de lienzo: el parser acota a 64 argumentos.
const SPLINE_TOOL_MAX_POINTS: usize = 60;

fn handle_spline(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    // Cierre por proximidad al primer punto (igual que Polígono: 20px).
    if state.pending.len() >= 2 && world.distance(&state.pending[0]) < 20.0 / document.view().scale
    {
        let pts = std::mem::take(&mut state.pending);
        return emit_spline(state, document, &pts);
    }
    state.pending.push(world);
    if state.pending.len() >= SPLINE_TOOL_MAX_POINTS {
        let pts = std::mem::take(&mut state.pending);
        return emit_spline(state, document, &pts);
    }
    tool_hint(&format!(
        "Spline: punto {} (clic cerca del inicio para cerrar)",
        state.pending.len()
    ))
}

fn emit_spline(state: &mut ToolState, document: &mut Document, pts: &[Point2]) -> ToolResult {
    if pts.len() < 2 {
        return tool_honest_reset(state, "Spline: se necesitan al menos 2 puntos");
    }
    let args = pts
        .iter()
        .map(|p| format!("({:.2},{:.2})", p.x, p.y))
        .collect::<Vec<_>>()
        .join(",");
    finish_with_command(
        state,
        document,
        format!("Spline[{args}]"),
        "Spline creada".to_string(),
        false,
    )
}

fn handle_prism(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    let is_polygon = |obj: &GeoObject| matches!(obj, GeoObject::Polygon(_));
    match labeled_source_at(document, world, is_polygon) {
        Some(label) => finish_with_command(
            state,
            document,
            format!("Prism[{label}, 2]"),
            format!("Prisma sobre {label} (altura 2; ajustala con Prism[etiqueta, altura])"),
            true,
        ),
        None => tool_hint("Prisma: clic sobre un polígono base"),
    }
}

fn handle_tetrahedron(state: &mut ToolState, document: &mut Document, world: Point2) -> ToolResult {
    if !world.x.is_finite() || !world.y.is_finite() {
        return tool_honest_reset(state, "Tetraedro: punto no finito");
    }
    finish_with_command(
        state,
        document,
        format!("Tetrahedron[{:.2}, {:.2}, 0, 2]", world.x, world.y),
        "Tetraedro creado (arista 2; ajustala con Tetrahedron[x, y, z, arista])".to_string(),
        true,
    )
}

fn handle_action_box(
    state: &mut ToolState,
    document: &mut Document,
    command: &str,
    caption_base: &str,
    var_base: &str,
) -> ToolResult {
    // Variable única estilo Slider (`v{N}`): motor crea la variable en 0/1.
    let mut index = document.variables.len() + 1;
    let mut var = format!("{var_base}{index}");
    while document.variables.contains_key(&var) {
        index += 1;
        if index > 1_000_000 {
            return tool_honest_reset(state, "No hay nombres de variable libres");
        }
        var = format!("{var_base}{index}");
    }
    let caption = format!("{caption_base} {index}");
    finish_with_command(
        state,
        document,
        format!("{command}[{caption}, {var}]"),
        format!("{caption} ligado a {var}"),
        true,
    )
}

#[cfg(test)]
mod dispatcher_new_arms_tests {
    use super::*;

    fn empty_doc() -> Document {
        Document::new()
    }

    #[test]
    fn parallel_without_point_line_pair_guides_honestly() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        // Primer clic: pide segundo punto.
        let r1 = dispatch_tool(Tool::Parallel, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert!(!r1.reset_tool);
        // Segundo clic sin objetos: guía honesta, sin mutación.
        let before = doc.object_count();
        let r2 = dispatch_tool(Tool::Parallel, &mut state, &mut doc, Point2::new(1.0, 1.0));
        assert_eq!(doc.object_count(), before);
        let msg = r2.message.expect("guía");
        assert!(msg.contains("punto") && msg.contains("recta"), "{msg}");
    }

    #[test]
    fn parallel_with_point_and_line_creates_parallel() {
        use grafito_core::{LineObj, PointObj};
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        doc.try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(0.0, 1.0)).with_label("A"),
        ))
        .expect("punto A");
        doc.try_add_object(GeoObject::Line(
            LineObj::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)).with_label("r"),
        ))
        .expect("recta r");
        let before = doc.object_count();
        dispatch_tool(Tool::Parallel, &mut state, &mut doc, Point2::new(0.0, 1.0));
        let r = dispatch_tool(Tool::Parallel, &mut state, &mut doc, Point2::new(2.0, 0.0));
        assert!(r.reset_tool, "debe resetear tras crear");
        assert!(
            doc.object_count() > before,
            "Parallel[A,r] debe crear la paralela"
        );
    }

    #[test]
    fn arc_three_clicks_delegates_to_motor() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        for (i, p) in [(0.0, 0.0), (2.0, 0.0), (1.0, 1.0)].iter().enumerate() {
            let r = dispatch_tool(Tool::Arc, &mut state, &mut doc, Point2::new(p.0, p.1));
            if i < 2 {
                assert!(!r.reset_tool, "pide punto {}/3", i + 2);
            }
        }
        // El motor crea el arco o erra honesto sin panic; el flujo resetea.
        assert!(state.pending.is_empty());
    }

    #[test]
    fn sector_three_clicks_emit_degrees() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        for p in [(0.0, 0.0), (2.0, 0.0), (0.0, 2.0)] {
            dispatch_tool(Tool::Sector, &mut state, &mut doc, Point2::new(p.0, p.1));
        }
        assert!(state.pending.is_empty(), "flujo completo limpia pending");
    }

    #[test]
    fn polygon_sides_resolve_variable_session_default() {
        let state = ToolState::default();
        let doc = empty_doc();
        assert_eq!(resolve_polygon_sides(&state, &doc), DEFAULT_POLYGON_SIDES);
        assert_eq!(DEFAULT_POLYGON_SIDES, 5);
        let mut state = ToolState::default();
        set_polygon_sides(&mut state, 8).expect("n=8 válido");
        assert_eq!(resolve_polygon_sides(&state, &doc), 8);
        assert!(set_polygon_sides(&mut state, 2).is_err());
        assert!(set_polygon_sides(&mut state, 65).is_err());
    }
}

#[cfg(test)]
mod dispatcher_f3a_tests {
    use super::*;
    use grafito_core::{LineObj, PointObj, PolygonObj};

    fn empty_doc() -> Document {
        Document::new()
    }

    fn add_point(doc: &mut Document, label: &str, x: f64, y: f64) {
        doc.try_add_object(GeoObject::Point(
            PointObj::new(Point2::new(x, y)).with_label(label),
        ))
        .expect("punto de prueba");
    }

    fn point_positions(doc: &Document, label: &str) -> Vec<Point2> {
        doc.objects_iter()
            .filter_map(|(_, obj)| match obj {
                GeoObject::Point(p) if p.label == label => Some(p.position),
                _ => None,
            })
            .collect()
    }

    fn count_kind(doc: &Document, kind: &str) -> usize {
        doc.objects_iter()
            .filter(|(_, obj)| match obj {
                GeoObject::Circle(_) => kind == "circle",
                GeoObject::Sector(_) => kind == "sector",
                GeoObject::Spline(_) => kind == "spline",
                GeoObject::Prism3D(_) => kind == "prism",
                GeoObject::Tetrahedron3D(_) => kind == "tetra",
                GeoObject::Text(_) => kind == "text",
                _ => false,
            })
            .count()
    }

    #[test]
    fn reflect_across_axis_mirrors_point() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 1.0, 0.0);
        let before = doc.object_count();
        // Clic-1: fuente. Eje x=0 con dos clics.
        let r1 = dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(1.0, 0.0));
        assert!(!r1.reset_tool);
        assert!(state.driver.is_some());
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(0.0, -1.0));
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(0.0, 1.0));
        assert_eq!(doc.object_count(), before + 1, "Reflect crea el espejado");
        assert!(state.pending.is_empty() && state.driver.is_none());
        let mirrored = point_positions(&doc, "A'");
        assert_eq!(mirrored.len(), 1, "etiqueta A' generada");
        assert!(
            (mirrored[0].x + 1.0).abs() < 1e-6 && mirrored[0].y.abs() < 1e-6,
            "A(1,0) sobre eje x=0 -> (-1,0), dio {:?}",
            mirrored[0]
        );
    }

    #[test]
    fn reflect_coincident_axis_clicks_do_central_symmetry() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 2.0, 0.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(2.0, 0.0));
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(0.0, 0.0));
        let done = dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert!(done.message.is_some());
        assert_eq!(doc.object_count(), before + 1);
        let mirrored = point_positions(&doc, "A'");
        assert_eq!(mirrored.len(), 1);
        assert!(
            (mirrored[0].x + 2.0).abs() < 1e-6 && mirrored[0].y.abs() < 1e-6,
            "simetría central de (2,0) en origen -> (-2,0), dio {:?}",
            mirrored[0]
        );
    }

    #[test]
    fn reflect_central_symmetry_rejects_non_point_honestly() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        doc.try_add_object(GeoObject::Line(
            LineObj::new(Point2::new(0.0, 0.0), Point2::new(4.0, 0.0)).with_label("r"),
        ))
        .expect("recta r");
        let before = doc.object_count();
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(1.0, 0.0));
        dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(5.0, 5.0));
        let done = dispatch_tool(Tool::Reflect, &mut state, &mut doc, Point2::new(5.0, 5.0));
        assert_eq!(
            doc.object_count(),
            before,
            "sin mutación ante límite del motor"
        );
        let msg = done.message.expect("mensaje honesto");
        assert!(msg.contains("simetría central"), "{msg}");
    }

    #[test]
    fn rotate_angle_comes_from_third_click() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 1.0, 0.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Rotate, &mut state, &mut doc, Point2::new(1.0, 0.0));
        let r2 = dispatch_tool(Tool::Rotate, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert!(!r2.reset_tool, "tras el centro pide el ángulo");
        dispatch_tool(Tool::Rotate, &mut state, &mut doc, Point2::new(0.0, 1.0));
        assert_eq!(doc.object_count(), before + 1);
        let rotated = point_positions(&doc, "A'");
        assert_eq!(rotated.len(), 1);
        assert!(
            rotated[0].x.abs() < 1e-6 && (rotated[0].y - 1.0).abs() < 1e-6,
            "rotar (1,0) 90° sobre origen -> (0,1), dio {:?}",
            rotated[0]
        );
    }

    #[test]
    fn rotate_without_point_guides_without_mutating() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        let r = dispatch_tool(Tool::Rotate, &mut state, &mut doc, Point2::new(5.0, 5.0));
        assert_eq!(doc.object_count(), before);
        assert!(state.driver.is_none());
        let msg = r.message.expect("guía");
        assert!(msg.contains("clic sobre el punto"), "{msg}");
    }

    #[test]
    fn translate_vector_from_two_clicks() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 1.0, 1.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(1.0, 1.0));
        dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(0.0, 0.0));
        dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(2.0, 3.0));
        assert_eq!(doc.object_count(), before + 1);
        let moved = point_positions(&doc, "A'");
        assert_eq!(moved.len(), 1);
        assert!(
            (moved[0].x - 3.0).abs() < 1e-6 && (moved[0].y - 4.0).abs() < 1e-6,
            "(1,1)+(2,3) -> (3,4), dio {:?}",
            moved[0]
        );
    }

    #[test]
    fn translate_rejects_null_vector_honestly() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 1.0, 1.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(1.0, 1.0));
        dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(0.0, 0.0));
        let done = dispatch_tool(Tool::Translate, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert_eq!(doc.object_count(), before);
        let msg = done.message.expect("mensaje honesto");
        assert!(msg.contains("vector nulo"), "{msg}");
    }

    #[test]
    fn dilate_factor_from_image_click() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 2.0, 0.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(2.0, 0.0));
        dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(0.0, 0.0));
        dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(3.0, 0.0));
        assert_eq!(doc.object_count(), before + 1);
        let scaled = point_positions(&doc, "A'");
        assert_eq!(scaled.len(), 1);
        assert!(
            (scaled[0].x - 3.0).abs() < 1e-6 && scaled[0].y.abs() < 1e-6,
            "k=3/2 sobre (2,0) -> (3,0), dio {:?}",
            scaled[0]
        );
    }

    #[test]
    fn dilate_rejects_center_on_source_honestly() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        add_point(&mut doc, "A", 1.0, 1.0);
        let before = doc.object_count();
        dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(1.0, 1.0));
        dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(1.0, 1.0));
        let done = dispatch_tool(Tool::Dilate, &mut state, &mut doc, Point2::new(2.0, 2.0));
        assert_eq!(doc.object_count(), before);
        let msg = done.message.expect("mensaje honesto");
        assert!(msg.contains("coincide con el centro"), "{msg}");
    }

    #[test]
    fn compass_two_clicks_create_circle_with_radius() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        dispatch_tool(Tool::Compass, &mut state, &mut doc, Point2::new(0.0, 0.0));
        dispatch_tool(Tool::Compass, &mut state, &mut doc, Point2::new(3.0, 0.0));
        assert_eq!(doc.object_count(), before + 1);
        assert_eq!(count_kind(&doc, "circle"), 1);
        let radius = doc
            .objects_iter()
            .find_map(|(_, obj)| match obj {
                GeoObject::Circle(c) => Some(c.radius),
                _ => None,
            })
            .expect("círculo del compás");
        assert!((radius - 3.0).abs() < 1e-6, "r=3, dio {radius}");
    }

    #[test]
    fn semicircle_two_clicks_create_sector() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        dispatch_tool(
            Tool::Semicircle,
            &mut state,
            &mut doc,
            Point2::new(1.0, 1.0),
        );
        dispatch_tool(
            Tool::Semicircle,
            &mut state,
            &mut doc,
            Point2::new(3.0, 1.0),
        );
        assert_eq!(doc.object_count(), before + 1);
        assert_eq!(count_kind(&doc, "sector"), 1);
    }

    #[test]
    fn spline_closes_near_first_point() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        for p in [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)] {
            dispatch_tool(Tool::Spline, &mut state, &mut doc, Point2::new(p.0, p.1));
        }
        assert_eq!(state.pending.len(), 3);
        dispatch_tool(Tool::Spline, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert_eq!(doc.object_count(), before + 1);
        assert_eq!(count_kind(&doc, "spline"), 1);
        assert!(state.pending.is_empty());
    }

    #[test]
    fn prism_needs_polygon_then_extrudes() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        let guide = dispatch_tool(Tool::Prism3D, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert_eq!(doc.object_count(), before, "sin polígono no muta");
        let msg = guide.message.expect("guía");
        assert!(msg.contains("polígono"), "{msg}");
        let mut poly = PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(0.0, 2.0),
        ]);
        poly.label = "P".to_string();
        doc.try_add_object(GeoObject::Polygon(poly))
            .expect("polígono base");
        let before = doc.object_count();
        dispatch_tool(Tool::Prism3D, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert_eq!(doc.object_count(), before + 1);
        assert_eq!(count_kind(&doc, "prism"), 1);
    }

    #[test]
    fn tetrahedron_single_click_creates_default_solid() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let before = doc.object_count();
        let done = dispatch_tool(
            Tool::Tetrahedron3D,
            &mut state,
            &mut doc,
            Point2::new(1.0, 2.0),
        );
        assert!(done.reset_tool);
        assert_eq!(doc.object_count(), before + 1);
        assert_eq!(count_kind(&doc, "tetra"), 1);
        let edge = doc
            .objects_iter()
            .find_map(|(_, obj)| match obj {
                GeoObject::Tetrahedron3D(t) => Some(t.edge_length),
                _ => None,
            })
            .expect("tetraedro");
        assert!((edge - 2.0).abs() < 1e-9, "arista default 2, dio {edge}");
    }

    #[test]
    fn checkbox_creates_variable_and_text() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let done = dispatch_tool(Tool::Checkbox, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert!(done.reset_tool);
        assert_eq!(doc.variables.get("casilla1"), Some(&0.0));
        assert_eq!(count_kind(&doc, "text"), 1);
        let msg = done.message.expect("mensaje del motor");
        assert!(msg.contains("casilla1"), "{msg}");
    }

    #[test]
    fn inputbox_creates_variable_and_text() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        let done = dispatch_tool(Tool::InputBox, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert!(done.reset_tool);
        assert_eq!(doc.variables.get("entrada1"), Some(&0.0));
        assert_eq!(count_kind(&doc, "text"), 1);
        let msg = done.message.expect("mensaje del motor");
        assert!(msg.contains("entrada1"), "{msg}");
    }

    #[test]
    fn action_boxes_pick_unique_variable_names() {
        let mut state = ToolState::default();
        let mut doc = empty_doc();
        dispatch_tool(Tool::Checkbox, &mut state, &mut doc, Point2::new(0.0, 0.0));
        dispatch_tool(Tool::Checkbox, &mut state, &mut doc, Point2::new(0.0, 0.0));
        assert_eq!(doc.variables.get("casilla1"), Some(&0.0));
        assert_eq!(doc.variables.get("casilla2"), Some(&0.0));
    }
}
