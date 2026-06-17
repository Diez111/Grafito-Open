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
    .or_else(|| snap_to_intersections(world, document, tol))
}

/// Computa intersecciones entre pares de objetos visibles cercanos al cursor.
fn snap_to_intersections(world: Point2, document: &Document, tol: f64) -> Option<SnapResult> {
    use grafito_core::GeoObject;
    use grafito_geometry::intersections::{
        circle_circle, function_function, function_line, line_circle, line_line, IntersectionResult,
    };

    let mut nearby: Vec<&GeoObject> = Vec::new();
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        if matches!(
            obj,
            GeoObject::Line(_) | GeoObject::Circle(_) | GeoObject::Function(_)
        ) {
            nearby.push(obj);
        }
    }

    let extract_points = |result: IntersectionResult| -> Vec<Point2> {
        match result {
            IntersectionResult::One(p) => vec![p],
            IntersectionResult::Two(p1, p2) => vec![p1, p2],
            _ => vec![],
        }
    };

    let mut best: Option<(f64, Point2, String)> = None;
    for i in 0..nearby.len() {
        for j in (i + 1)..nearby.len() {
            let a = nearby[i];
            let b = nearby[j];
            let points = match (a, b) {
                (GeoObject::Line(l1), GeoObject::Line(l2)) => {
                    let s1 = Point2::new(l1.start.x, l1.start.y);
                    let e1 = Point2::new(l1.end.x, l1.end.y);
                    let s2 = Point2::new(l2.start.x, l2.start.y);
                    let e2 = Point2::new(l2.end.x, l2.end.y);
                    extract_points(line_line(s1, e1, s2, e2))
                }
                (GeoObject::Line(l), GeoObject::Circle(c))
                | (GeoObject::Circle(c), GeoObject::Line(l)) => {
                    let s = Point2::new(l.start.x, l.start.y);
                    let e = Point2::new(l.end.x, l.end.y);
                    extract_points(line_circle(s, e, c.center, c.radius))
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
                    if dx.abs() < 1e-12 {
                        continue;
                    }
                    let slope = dy / dx;
                    let intercept = s.y - slope * s.x;
                    let x_min = s.x.min(e.x) - 5.0;
                    let x_max = s.x.max(e.x) + 5.0;
                    function_line(&f.expr, slope, intercept, x_min, x_max)
                }
                (GeoObject::Function(f1), GeoObject::Function(f2)) => {
                    function_function(&f1.expr, &f2.expr, -20.0, 20.0)
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
        // representación curva.
        match obj {
            grafito_core::GeoObject::Function(_)
            | grafito_core::GeoObject::Circle(_)
            | grafito_core::GeoObject::Line(_) => {}
            _ => continue,
        }
        // Aproximación rápida: si el cursor está "razonablemente" cerca en
        // coordenadas de mundo, proyectamos. La heurística de selección fina
        // se delega a `evaluate_curve_at`.
        let vars: HashMap<String, f64> = document.variables.clone();
        if let Some(proj) = grafito_core::analyzable::evaluate_curve_at(obj, world, &vars) {
            if let grafito_core::GeoObject::Function(f) = obj {
                let y = proj;
                if y.is_finite() && (y - world.y).abs() * view_scale <= tol_screen {
                    return Some(SnapResult {
                        point: Point2::new(world.x, y),
                        kind: SnapKind::Curve,
                        feature: None,
                        label: format!("f({:.3}) = {:.3}", world.x, y),
                    });
                }
                let _ = f;
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
                        let px = start.x + t * dx;
                        let py = start.y + t * dy;
                        return Some(SnapResult {
                            point: Point2::new(px, py),
                            kind: SnapKind::Curve,
                            feature: None,
                            label: format!("({:.3}, {:.3})", px, py),
                        });
                    }
                }
            }
        }
    }
    None
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
    use grafito_core::{Document, FunctionObj, GeoObject, PointObj};
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
}
