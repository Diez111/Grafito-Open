//! Snapping jerárquico y configurable para el lienzo de Grafito.
//!
//! Cuando el usuario mueve el cursor (o hace clic) en el canvas, queremos
//! "clavar" el punto del mundo en la característica o referencia más cercana
//! dentro de cierta tolerancia. La jerarquía implementada es:
//!
//! 1. **Característica exacta** (raíz, extremo, inflexión, intersección con
//!    eje, equilibrio, intersección entre curvas).
//! 2. **Proyección a la curva** del objeto bajo el cursor.
//! 3. **Snap a objeto** (punto existente dentro de tolerancia).
//! 4. **Snap a eje** (eje X o Y si el cursor está muy cerca).
//! 5. **Snap a cuadrícula** según `grid_step`.
//! 6. **Libre** — el cursorそのまま.
//!
//! La función [`snap_point`] es pura y determinista; el llamador decide
//! cuándo invocarla (en hover, en clic) y cómo pintar el resultado. Los
//! atajos de teclado (Shift, Alt) y la tecla G se traducen a flags
//! [`SnapOverrides`] que se pasan junto con [`SnapConfig`].

use grafito_core::analyzable::{analyze_object, default_analysis_features};
use grafito_core::Document;
use grafito_geometry::analysis::AnalysisFeature;
use grafito_geometry::Point2;
use std::collections::HashMap;

/// Configuración persistente del snap, guardada en `AppConfig`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapConfig {
    /// Radio de tolerancia en píxeles (se divide por la escala del mundo para
    /// convertirse a unidades de mundo).
    pub pixel_tolerance: f64,
    /// Activa snap a características exactas (raíces, extremos, …).
    pub snap_to_features: bool,
    /// Activa snap a la curva del objeto bajo el cursor.
    pub snap_to_curve: bool,
    /// Activa snap a la cuadrícula visible.
    pub snap_to_grid: bool,
    /// Activa snap a los ejes cartesianos.
    pub snap_to_axis: bool,
    /// Activa snap a puntos y otros objetos existentes.
    pub snap_to_objects: bool,
    /// Paso explícito de cuadrícula (en unidades de mundo). Si es `None` se
    /// calcula desde el viewport.
    pub grid_step: Option<f64>,
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self {
            pixel_tolerance: 8.0,
            snap_to_features: true,
            snap_to_curve: true,
            snap_to_grid: true,
            snap_to_axis: true,
            snap_to_objects: true,
            grid_step: None,
        }
    }
}

/// Flags que se derivan del estado de teclado en el momento de la consulta.
#[derive(Debug, Clone, Copy, Default)]
pub struct SnapOverrides {
    /// Si es `true`, el snap se desactiva y se devuelve la posición libre.
    pub shift_pressed: bool,
    /// Si es `true`, el snap se fuerza a la característica exacta más
    /// cercana, ignorando el resto de la jerarquía.
    pub alt_pressed: bool,
}

/// Categoría del punto snapped, útil para colorear el cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// El cursorそのまま, sin ajuste.
    Free,
    /// El cursor se ajustó a una característica exacta (raíz, extremo, etc.).
    Feature,
    /// El cursor se proyectó sobre la curva.
    Curve,
    /// El cursor se ajustó a un objeto existente.
    Object,
    /// El cursor se ajustó a un eje cartesiano.
    Axis,
    /// El cursor se ajustó a una intersección de la cuadrícula.
    Grid,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Free => "libre",
            SnapKind::Feature => "característica",
            SnapKind::Curve => "curva",
            SnapKind::Object => "objeto",
            SnapKind::Axis => "eje",
            SnapKind::Grid => "cuadrícula",
        }
    }
}

/// Resultado de un intento de snap.
#[derive(Debug, Clone)]
pub struct SnapResult {
    /// Coordenada del mundo después del snap.
    pub point: Point2,
    /// Tipo de snap aplicado.
    pub kind: SnapKind,
    /// Si `kind == Feature`, la característica concreta que se seleccionó.
    pub feature: Option<AnalysisFeature>,
    /// Etiqueta legible para mostrar en el cursor (p. ej. "Raíz: (1.0, 0.0)").
    pub label: String,
}

impl SnapResult {
    fn free(point: Point2) -> Self {
        Self {
            point,
            kind: SnapKind::Free,
            feature: None,
            label: format!("({:.3}, {:.3})", point.x, point.y),
        }
    }
}

/// Distancia convertida a unidades de mundo: `pixel_tolerance / view.scale`.
fn world_tolerance(cfg: &SnapConfig, scale: f64) -> f64 {
    (cfg.pixel_tolerance / scale.max(1e-6)).max(1e-6)
}

/// Calcula la característica más cercana dentro de la tolerancia, iterando
/// sobre los objetos visibles. El resultado es `None` si no hay coincidencias
/// o si `snap_to_features` está desactivado y no se ha forzado con Alt.
pub fn snap_point(
    world: Point2,
    document: &Document,
    view_scale: f64,
    cfg: &SnapConfig,
    overrides: SnapOverrides,
    tool_filter: Option<Vec<AnalysisFeature>>,
) -> SnapResult {
    if overrides.shift_pressed {
        return SnapResult::free(world);
    }
    let tol = world_tolerance(cfg, view_scale);

    if overrides.alt_pressed || cfg.snap_to_features {
        if let Some(r) = snap_to_feature(world, document, view_scale, tol, tool_filter) {
            return r;
        }
    }

    if overrides.alt_pressed {
        // Alt = forzar feature; si no hay feature cercana, devolvemos libre.
        return SnapResult::free(world);
    }

    if cfg.snap_to_curve {
        if let Some(r) = snap_to_curve(world, document, view_scale, tol) {
            return r;
        }
    }

    if cfg.snap_to_objects {
        if let Some(r) = snap_to_object(world, document, view_scale, tol) {
            return r;
        }
    }

    if cfg.snap_to_axis {
        if let Some(r) = snap_to_axis(world, view_scale, tol) {
            return r;
        }
    }

    if cfg.snap_to_grid {
        if let Some(r) = snap_to_grid(world, view_scale, tol, cfg.grid_step) {
            return r;
        }
    }

    SnapResult::free(world)
}

fn snap_to_feature(
    world: Point2,
    document: &Document,
    view_scale: f64,
    tol: f64,
    tool_filter: Option<Vec<AnalysisFeature>>,
) -> Option<SnapResult> {
    // La tolerancia en mundo ya está calculada por el caller; view_scale se
    // mantiene en la firma para simetría con `snap_to_curve` y por si en el
    // futuro queremos ajustar la tolerancia en función del zoom.
    let _ = view_scale;
    // Cache simple: mantenemos los resultados por (view_bounds_hash, vars_hash)
    // en un Arc<Mutex<...>> del Document; aquí solo consultamos en línea.
    let view = *document.view();
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(glam::Vec2::new(view.screen_size.x, view.screen_size.y));
    let view_bounds = (
        world_tl.x.min(world_br.x),
        world_tl.x.max(world_br.x),
        world_tl.y.min(world_br.y),
        world_tl.y.max(world_br.y),
    );

    // OPTIMIZACIÓN CRÍTICA PARA EL LAG: No analizar toda la pantalla (cientos de evaluaciones).
    // Solo analizamos una ventana estrecha alrededor del cursor.
    let local_bounds = (
        world.x - tol * 10.0,
        world.x + tol * 10.0,
        view_bounds.2,
        view_bounds.3,
    );

    let vars: HashMap<String, f64> = document.variables.clone();
    let features = tool_filter.unwrap_or_else(default_analysis_features);
    let mut best: Option<(f64, Point2, AnalysisFeature, String)> = None;
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        let results = analyze_object(obj, local_bounds, &vars, &features);
        for r in results {
            let d = r.point.distance(&world);
            if d <= tol
                && (best.is_none() || d < best.as_ref().map(|b| b.0).unwrap_or(f64::INFINITY))
            {
                best = Some((d, r.point, r.feature, r.label));
            }
        }
    }
    best.map(|(_, p, f, label)| SnapResult {
        point: p,
        kind: SnapKind::Feature,
        feature: Some(f),
        label,
    })
    .or_else(|| snap_to_intersections(world, document, tol, view_bounds))
}

/// Cap de pares evaluados por frame: acota el O(n²) en documentos densos.
const MAX_INTERSECTION_PAIRS_PER_FRAME: usize = 32;

/// Multiplicador del pre-filtro respecto a la tolerancia de snap.
const INTERSECTION_PREFILTER_MULT: f64 = 4.0;

/// Computa intersecciones entre pares de objetos visibles cercanos al cursor.
///
/// Pre-filtro en dos niveles: (1) `SpatialIndex::candidates(world, tol*4)` si
/// el índice está poblado; (2) AABB rápida expandida por `tol*4`, descartando
/// lo que no interseca el disco `(world, tol*4)`. Con índice vacío o stale se
/// usa solo el AABB manual. Los pares supervivientes se ordenan por distancia
/// media al cursor y se capan a 32 pares/frame.
///
/// `view_bounds = (xmin, xmax, ymin, ymax)` es el viewport ya calculado en
/// `snap_to_feature`: delimita los barridos `function_line`/`function_function`
/// (antes hardcodeados a `±5.0` y `-20..20`).
fn snap_to_intersections(
    world: Point2,
    document: &Document,
    tol: f64,
    view_bounds: (f64, f64, f64, f64),
) -> Option<SnapResult> {
    use grafito_core::{GeoObject, LineKind, ObjectId};
    use grafito_geometry::intersections::{
        circle_circle, function_function, function_line, line_circle, line_line, IntersectionResult,
    };
    use std::cmp::Ordering;

    let pre_tol = (tol * INTERSECTION_PREFILTER_MULT).max(1e-9);

    /// AABB rápida de objetos finitos; `None` = no acotado (Function y recta
    /// infinita) → siempre candidato al pre-filtro.
    fn quick_aabb(obj: &GeoObject) -> Option<(f64, f64, f64, f64)> {
        match obj {
            GeoObject::Line(l) => {
                if l.kind == LineKind::Line {
                    None
                } else {
                    Some((
                        l.start.x.min(l.end.x),
                        l.start.y.min(l.end.y),
                        l.start.x.max(l.end.x),
                        l.start.y.max(l.end.y),
                    ))
                }
            }
            GeoObject::Circle(c) => {
                if !c.radius.is_finite() {
                    return None;
                }
                Some((
                    c.center.x - c.radius,
                    c.center.y - c.radius,
                    c.center.x + c.radius,
                    c.center.y + c.radius,
                ))
            }
            // Function: no acotada en x → siempre candidata.
            _ => None,
        }
    }

    /// Distancia del cursor al AABB (0 si el cursor cae dentro).
    fn aabb_distance(world: Point2, bb: (f64, f64, f64, f64)) -> f64 {
        let cx = world.x.clamp(bb.0, bb.2);
        let cy = world.y.clamp(bb.1, bb.3);
        ((world.x - cx).powi(2) + (world.y - cy).powi(2)).sqrt()
    }

    // Nivel 1: candidatos del índice espacial (R-tree) si está poblado.
    let spatial_ids = if document.spatial.is_empty() {
        Vec::new()
    } else {
        document.spatial.candidates(world.x, world.y, pre_tol)
    };
    let use_spatial = !spatial_ids.is_empty();

    // Nivel 2: AABB manual contra el disco (world, tol*4). `anchor` estima la
    // distancia objeto→cursor y sirve para ordenar los pares después.
    let mut manual: Vec<(ObjectId, &GeoObject, f64)> = Vec::new();
    for (id, obj) in document.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        if !matches!(
            obj,
            GeoObject::Line(_) | GeoObject::Circle(_) | GeoObject::Function(_)
        ) {
            continue;
        }
        let anchor = match obj {
            // Recta infinita: el AABB del segmento muestra es arbitrario; se
            // usa distancia punto-recta.
            GeoObject::Line(l) if l.kind == LineKind::Line => l.distance_to_point(world),
            _ => match quick_aabb(obj) {
                Some(bb) => aabb_distance(world, bb),
                None => {
                    // Function: estima con la distancia vertical si es evaluable.
                    if let GeoObject::Function(f) = obj {
                        match grafito_geometry::expr::eval_function_with_vars(
                            &f.expr,
                            world.x,
                            &document.variables,
                        ) {
                            Ok(y) if y.is_finite() => (y - world.y).abs(),
                            _ => 0.0,
                        }
                    } else {
                        0.0
                    }
                }
            },
        };
        if anchor <= pre_tol {
            manual.push((*id, obj, anchor));
        }
    }

    // Intersección con el índice espacial; si el índice está stale y filtra
    // todo (< 2 supervivientes), se conserva la lista solo-AABB.
    let nearby: Vec<(ObjectId, &GeoObject, f64)> = if use_spatial {
        let filtered: Vec<(ObjectId, &GeoObject, f64)> = manual
            .iter()
            .filter(|(id, _, _)| spatial_ids.contains(id))
            .cloned()
            .collect();
        if filtered.len() >= 2 {
            filtered
        } else {
            manual
        }
    } else {
        manual
    };

    // Ordena los pares por distancia media al cursor y capa a 32/frame.
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for i in 0..nearby.len() {
        for j in (i + 1)..nearby.len() {
            let score = (nearby[i].2 + nearby[j].2) * 0.5;
            pairs.push((i, j, score));
        }
    }
    pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));
    pairs.truncate(MAX_INTERSECTION_PAIRS_PER_FRAME);

    let extract_points = |result: IntersectionResult| -> Vec<Point2> {
        match result {
            IntersectionResult::One(p) => vec![p],
            IntersectionResult::Two(p1, p2) => vec![p1, p2],
            _ => vec![],
        }
    };

    let (vx0, vx1, _, _) = view_bounds;

    let mut best: Option<(f64, Point2, String)> = None;
    for (i, j, _) in pairs {
        let a = nearby[i].1;
        let b = nearby[j].1;
        let points = match (a, b) {
            (GeoObject::Line(l1), GeoObject::Line(l2)) => {
                let s1 = Point2::new(l1.start.x, l1.start.y);
                let e1 = Point2::new(l1.end.x, l1.end.y);
                let s2 = Point2::new(l2.start.x, l2.start.y);
                let e2 = Point2::new(l2.end.x, l2.end.y);
                extract_points(line_line(s1, e1, s2, e2))
                    .into_iter()
                    .filter(|point| {
                        l1.kind_contains_t(l1.param_at_point(*point))
                            && l2.kind_contains_t(l2.param_at_point(*point))
                    })
                    .collect()
            }
            (GeoObject::Line(l), GeoObject::Circle(c))
            | (GeoObject::Circle(c), GeoObject::Line(l)) => {
                let s = Point2::new(l.start.x, l.start.y);
                let e = Point2::new(l.end.x, l.end.y);
                extract_points(line_circle(s, e, c.center, c.radius))
                    .into_iter()
                    .filter(|point| l.kind_contains_t(l.param_at_point(*point)))
                    .collect()
            }
            (GeoObject::Circle(c1), GeoObject::Circle(c2)) => {
                extract_points(circle_circle(c1.center, c1.radius, c2.center, c2.radius))
            }
            (GeoObject::Function(f), GeoObject::Line(l))
            | (GeoObject::Line(l), GeoObject::Function(f)) => {
                let s = Point2::new(l.start.x, l.start.y);
                let e = Point2::new(l.end.x, l.end.y);
                let dx = e.x - s.x;
                let dy = e.y - s.y;
                let points = if dx.abs() < 1e-12 {
                    // Recta vertical: solo si su x cae dentro del viewport.
                    if s.x < vx0 || s.x > vx1 {
                        vec![]
                    } else {
                        grafito_geometry::expr::eval_function_with_vars(
                            &f.expr,
                            s.x,
                            &document.variables,
                        )
                        .ok()
                        .filter(|y| y.is_finite())
                        .map(|y| vec![Point2::new(s.x, y)])
                        .unwrap_or_default()
                    }
                } else {
                    // Barrido acotado al viewport: la recta infinita usa
                    // todo el rango visible; el segmento, su span ∩ vista.
                    let (x_min, x_max) = match l.kind {
                        LineKind::Segment => {
                            ((s.x.min(e.x) - tol).max(vx0), (s.x.max(e.x) + tol).min(vx1))
                        }
                        LineKind::Ray | LineKind::Line => (vx0, vx1),
                    };
                    if x_max < x_min || !x_min.is_finite() || !x_max.is_finite() {
                        vec![]
                    } else {
                        let slope = dy / dx;
                        let intercept = s.y - slope * s.x;
                        function_line(&f.expr, slope, intercept, x_min, x_max)
                    }
                };
                points
                    .into_iter()
                    .filter(|point| l.kind_contains_t(l.param_at_point(*point)))
                    .collect()
            }
            (GeoObject::Function(f1), GeoObject::Function(f2)) => {
                // Barrido acotado al viewport visible (antes `-20..20` fijo).
                if !vx0.is_finite() || !vx1.is_finite() || vx1 <= vx0 {
                    vec![]
                } else {
                    function_function(&f1.expr, &f2.expr, vx0, vx1)
                }
            }
            _ => vec![],
        };
        for p in points {
            let d = p.distance(&world);
            if d <= tol
                && (best.is_none() || d < best.as_ref().map(|b| b.0).unwrap_or(f64::INFINITY))
            {
                best = Some((d, p, format!("Intersección: ({:.3}, {:.3})", p.x, p.y)));
            }
        }
    }
    best.map(|(_, p, label)| SnapResult {
        point: p,
        kind: SnapKind::Feature,
        feature: Some(AnalysisFeature::Intersection),
        label,
    })
}

fn snap_to_curve(
    world: Point2,
    document: &Document,
    view_scale: f64,
    tol: f64,
) -> Option<SnapResult> {
    let tol_screen = tol * view_scale;
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        // Solo intentamos proyección para objetos que tengan una
        // representación curva. `PolarCurve` se trata aparte con un barrido
        // dedicado porque `evaluate_curve_at` devuelve `None` para ella;
        // `ImplicitCurve` devuelve `None` a propósito (requiere marching
        // squares / Newton 2D, fuera del presupuesto por frame del snap).
        match obj {
            grafito_core::GeoObject::Function(_)
            | grafito_core::GeoObject::Circle(_)
            | grafito_core::GeoObject::Line(_)
            | grafito_core::GeoObject::Pencil(_)
            | grafito_core::GeoObject::ParametricCurve2D(_) => {}
            grafito_core::GeoObject::PolarCurve(c) => {
                let vars: HashMap<String, f64> = document.variables.clone();
                if let Some(pt) = closest_point_on_polar(c, world, &vars) {
                    if pt.distance(&world) * view_scale <= tol_screen {
                        return Some(SnapResult {
                            point: pt,
                            kind: SnapKind::Curve,
                            feature: None,
                            label: format!("polar ≈ ({:.3}, {:.3})", pt.x, pt.y),
                        });
                    }
                }
                continue;
            }
            _ => continue,
        }
        // Aproximación rápida: si el cursor está "razonablemente" cerca en
        // coordenadas de mundo, proyectamos. La heurística de selección fina
        // se delega a `evaluate_curve_at`.
        let vars: HashMap<String, f64> = document.variables.clone();
        if let Some(proj) = grafito_core::analyzable::evaluate_curve_at(obj, world, &vars) {
            if let grafito_core::GeoObject::Function(_) = obj {
                let y = proj;
                if y.is_finite() && (y - world.y).abs() * view_scale <= tol_screen {
                    return Some(SnapResult {
                        point: Point2::new(world.x, y),
                        kind: SnapKind::Curve,
                        feature: None,
                        label: format!("f({:.3}) = {:.3}", world.x, y),
                    });
                }
            } else if let grafito_core::GeoObject::Circle(c) = obj {
                let d = proj;
                if d.abs() * view_scale <= tol_screen {
                    let dx = world.x - c.center.x;
                    let dy = world.y - c.center.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(1e-10);
                    let px = c.center.x + dx / dist * c.radius;
                    let py = c.center.y + dy / dist * c.radius;
                    return Some(SnapResult {
                        point: Point2::new(px, py),
                        kind: SnapKind::Curve,
                        feature: None,
                        label: format!("({:.3}, {:.3})", px, py),
                    });
                }
            } else if let grafito_core::GeoObject::Line(l) = obj {
                let d = proj;
                if d.abs() * view_scale <= tol_screen {
                    let start = Point2::new(l.start.x, l.start.y);
                    let end = Point2::new(l.end.x, l.end.y);
                    let dx = end.x - start.x;
                    let dy = end.y - start.y;
                    let len2 = dx * dx + dy * dy;
                    if len2 > 1e-20 {
                        let t = ((world.x - start.x) * dx + (world.y - start.y) * dy) / len2;
                        let t = l.kind.clamp_t(t);
                        let px = start.x + t * dx;
                        let py = start.y + t * dy;
                        let point = Point2::new(px, py);
                        if point.distance(&world) * view_scale <= tol_screen {
                            return Some(SnapResult {
                                point,
                                kind: SnapKind::Curve,
                                feature: None,
                                label: format!("({:.3}, {:.3})", px, py),
                            });
                        }
                    }
                }
            } else if let grafito_core::GeoObject::Pencil(p) = obj {
                // `proj` es la distancia al segmento contiguo más cercano; el
                // punto más cercano se recalcula con barrido lineal.
                let d = proj;
                if d * view_scale <= tol_screen {
                    if let Some(pt) = closest_point_on_pencil(p, world) {
                        if pt.distance(&world) * view_scale <= tol_screen {
                            return Some(SnapResult {
                                point: pt,
                                kind: SnapKind::Curve,
                                feature: None,
                                label: format!("trazo({:.3}, {:.3})", pt.x, pt.y),
                            });
                        }
                    }
                }
            } else if let grafito_core::GeoObject::ParametricCurve2D(c) = obj {
                // `proj` es el best_t del barrido de 200 muestras en
                // `evaluate_curve_at`; se evalúa (x(t), y(t)) para el punto.
                let t = proj;
                if t.is_finite() && t >= c.t_min && t <= c.t_max {
                    let x = grafito_geometry::expr::eval_batch_1d(
                        &c.expr_x,
                        "t",
                        std::iter::once(t),
                        &vars,
                    )
                    .ok()
                    .and_then(|mut v| v.pop().flatten());
                    let y = grafito_geometry::expr::eval_batch_1d(
                        &c.expr_y,
                        "t",
                        std::iter::once(t),
                        &vars,
                    )
                    .ok()
                    .and_then(|mut v| v.pop().flatten());
                    if let (Some(x), Some(y)) = (x, y) {
                        if x.is_finite() && y.is_finite() {
                            let pt = Point2::new(x, y);
                            if pt.distance(&world) * view_scale <= tol_screen {
                                return Some(SnapResult {
                                    point: pt,
                                    kind: SnapKind::Curve,
                                    feature: None,
                                    label: format!("param(t={:.3}) = ({:.3}, {:.3})", t, x, y),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Punto más cercano sobre un trazo Pencil (barrido de segmentos con t
/// clamped a [0, 1]; O(n) en puntos del trazo).
fn closest_point_on_pencil(p: &grafito_core::PencilObj, world: Point2) -> Option<Point2> {
    if p.points.is_empty() {
        return None;
    }
    if p.points.len() == 1 {
        return Some(p.points[0]);
    }
    let mut best: Option<(f64, Point2)> = None;
    for w in p.points.windows(2) {
        let a = w[0];
        let b = w[1];
        let abx = b.x - a.x;
        let aby = b.y - a.y;
        let len2 = abx * abx + aby * aby;
        let (cx, cy) = if len2 < 1e-15 {
            (a.x, a.y)
        } else {
            let t = ((world.x - a.x) * abx + (world.y - a.y) * aby) / len2;
            let t = t.clamp(0.0, 1.0);
            (a.x + t * abx, a.y + t * aby)
        };
        let d2 = (world.x - cx).powi(2) + (world.y - cy).powi(2);
        let closer = match &best {
            None => true,
            Some((bd2, _)) => d2 < *bd2,
        };
        if closer {
            best = Some((d2, Point2::new(cx, cy)));
        }
    }
    best.map(|(_, pt)| pt)
}

/// Punto más cercano sobre una curva polar r(t): barrido de 200 muestras
/// t → (r·cos t, r·sin t). `evaluate_curve_at` no soporta Polar (→ None),
/// de ahí el barrido dedicado.
fn closest_point_on_polar(
    c: &grafito_core::PolarCurveObj,
    world: Point2,
    vars: &HashMap<String, f64>,
) -> Option<Point2> {
    if !(c.t_min.is_finite() && c.t_max.is_finite()) || c.t_max <= c.t_min {
        return None;
    }
    let n = 200;
    let mut best: Option<(f64, Point2)> = None;
    for i in 0..=n {
        let t = c.t_min + (i as f64 / n as f64) * (c.t_max - c.t_min);
        let r = grafito_geometry::expr::eval_batch_1d(&c.expr_r, "t", std::iter::once(t), vars)
            .ok()
            .and_then(|mut v| v.pop().flatten());
        if let Some(r) = r {
            if !r.is_finite() {
                continue;
            }
            let pt = Point2::new(r * t.cos(), r * t.sin());
            if !pt.x.is_finite() || !pt.y.is_finite() {
                continue;
            }
            let d2 = (pt.x - world.x).powi(2) + (pt.y - world.y).powi(2);
            let closer = match &best {
                None => true,
                Some((bd2, _)) => d2 < *bd2,
            };
            if closer {
                best = Some((d2, pt));
            }
        }
    }
    best.map(|(_, pt)| pt)
}

fn snap_to_object(
    world: Point2,
    document: &Document,
    view_scale: f64,
    tol: f64,
) -> Option<SnapResult> {
    // Reusamos `Document::pick_object` pero solo sobre puntos.
    let tol_world = (8.0_f64 / view_scale.max(1e-6)).max(tol);
    let mut best: Option<(f64, Point2, String)> = None;
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        if let grafito_core::GeoObject::Point(p) = obj {
            let d = p.position.distance(&world);
            if d <= tol_world
                && (best.is_none() || d < best.as_ref().map(|b| b.0).unwrap_or(f64::INFINITY))
            {
                best = Some((
                    d,
                    p.position,
                    format!("Punto: ({:.3}, {:.3})", p.position.x, p.position.y),
                ));
            }
        }
    }
    best.map(|(_, p, label)| SnapResult {
        point: p,
        kind: SnapKind::Object,
        feature: None,
        label,
    })
}

fn snap_to_axis(world: Point2, view_scale: f64, tol: f64) -> Option<SnapResult> {
    let tol_world = tol;
    if world.y.abs() <= tol_world {
        return Some(SnapResult {
            point: Point2::new(world.x, 0.0),
            kind: SnapKind::Axis,
            feature: Some(AnalysisFeature::XIntercept),
            label: format!("Eje X: ({:.3}, 0)", world.x),
        });
    }
    if world.x.abs() <= tol_world {
        return Some(SnapResult {
            point: Point2::new(0.0, world.y),
            kind: SnapKind::Axis,
            feature: Some(AnalysisFeature::YIntercept),
            label: format!("Eje Y: (0, {:.3})", world.y),
        });
    }
    let _ = view_scale;
    None
}

fn snap_to_grid(
    world: Point2,
    view_scale: f64,
    tol: f64,
    explicit_step: Option<f64>,
) -> Option<SnapResult> {
    let step = explicit_step.unwrap_or_else(|| {
        // Paso adaptativo: el siguiente valor 1·10^k ≥ 1/scale.
        let target = 1.0 / view_scale.max(1e-6);
        let exp = target.log10().floor();
        10f64.powi(exp as i32).max(1e-6)
    });
    let closest_x = (world.x / step).round() * step;
    let closest_y = (world.y / step).round() * step;
    if (closest_x - world.x).abs() <= tol && (closest_y - world.y).abs() <= tol {
        Some(SnapResult {
            point: Point2::new(closest_x, closest_y),
            kind: SnapKind::Grid,
            feature: None,
            label: format!("Cuadrícula: ({:.3}, {:.3})", closest_x, closest_y),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{Document, FunctionObj, GeoObject, LineKind, LineObj, PointObj};
    use grafito_geometry::Point2;

    fn empty_doc() -> Document {
        Document::new()
    }

    #[test]
    fn shift_disables_snap() {
        let doc = empty_doc();
        let r = snap_point(
            Point2::new(0.0, 0.0),
            &doc,
            50.0,
            &SnapConfig::default(),
            SnapOverrides {
                shift_pressed: true,
                ..Default::default()
            },
            None,
        );
        assert_eq!(r.kind, SnapKind::Free);
    }

    #[test]
    fn snap_to_axis_when_close_to_x() {
        let doc = empty_doc();
        let cfg = SnapConfig {
            snap_to_features: false,
            snap_to_curve: false,
            snap_to_objects: false,
            snap_to_grid: false,
            ..Default::default()
        };
        let r = snap_point(
            Point2::new(1.5, 0.05),
            &doc,
            50.0,
            &cfg,
            SnapOverrides::default(),
            None,
        );
        assert_eq!(r.kind, SnapKind::Axis);
        assert!((r.point.y).abs() < 1e-9);
    }

    #[test]
    fn snap_grid_disabled_when_flag_false() {
        let doc = empty_doc();
        let cfg = SnapConfig {
            snap_to_features: false,
            snap_to_curve: false,
            snap_to_objects: false,
            snap_to_axis: false,
            snap_to_grid: false,
            ..Default::default()
        };
        let r = snap_point(
            Point2::new(0.49, 0.49),
            &doc,
            50.0,
            &cfg,
            SnapOverrides::default(),
            None,
        );
        assert_eq!(r.kind, SnapKind::Free);
    }

    #[test]
    fn snap_to_feature_finds_root() {
        let mut doc = empty_doc();
        let f = GeoObject::Function(FunctionObj::new("x^2 - 1".to_string()));
        doc.add_object(f);
        let cfg = SnapConfig::default();
        // cursor a 0.01 de la raíz x=1, en unidades de mundo
        let r = snap_point(
            Point2::new(1.001, 0.0),
            &doc,
            50.0,
            &cfg,
            SnapOverrides::default(),
            None,
        );
        assert_eq!(r.kind, SnapKind::Feature);
        assert!((r.point.x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn snap_to_object_finds_nearby_point() {
        let mut doc = empty_doc();
        let p = GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0)));
        doc.add_object(p);
        let cfg = SnapConfig {
            snap_to_features: false,
            snap_to_curve: false,
            snap_to_axis: false,
            snap_to_grid: false,
            ..Default::default()
        };
        let r = snap_point(
            Point2::new(1.05, 1.05),
            &doc,
            50.0,
            &cfg,
            SnapOverrides::default(),
            None,
        );
        assert_eq!(r.kind, SnapKind::Object);
        assert!((r.point.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn line_snaps_do_not_use_segment_extensions() {
        let segment = GeoObject::Line(LineObj::new_with_kind(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            LineKind::Segment,
        ));

        let mut curve_doc = empty_doc();
        curve_doc.add_object(segment.clone());
        assert!(snap_to_curve(Point2::new(2.0, 0.0), &curve_doc, 1.0, 0.1).is_none());

        let mut ray_doc = empty_doc();
        ray_doc.add_object(GeoObject::Line(LineObj::new_with_kind(
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            LineKind::Ray,
        )));
        assert!(snap_to_curve(Point2::new(0.0, 0.0), &ray_doc, 1.0, 0.1).is_none());

        let mut intersection_doc = empty_doc();
        intersection_doc.add_object(segment);
        intersection_doc.add_object(GeoObject::Line(LineObj::new_with_kind(
            Point2::new(2.0, -1.0),
            Point2::new(2.0, 1.0),
            LineKind::Line,
        )));

        assert!(snap_to_intersections(
            Point2::new(2.0, 0.0),
            &intersection_doc,
            0.1,
            (-10.0, 10.0, -10.0, 10.0)
        )
        .is_none());

        let mut vertical_doc = empty_doc();
        vertical_doc.add_object(GeoObject::Function(FunctionObj::new("x".to_string())));
        vertical_doc.add_object(GeoObject::Line(LineObj::new_with_kind(
            Point2::new(2.0, 1.0),
            Point2::new(2.0, 3.0),
            LineKind::Segment,
        )));
        let vertical = snap_to_intersections(
            Point2::new(2.0, 2.0),
            &vertical_doc,
            0.1,
            (-10.0, 10.0, -10.0, 10.0),
        )
        .expect("vertical function-line intersection should snap");
        assert_eq!(vertical.point, Point2::new(2.0, 2.0));
    }
}
