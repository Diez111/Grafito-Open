//! Integración del análisis matemático con el modelo de objetos de Grafito.
//!
//! Provee una función única `analyze_object` que, dado un `GeoObject`, delega
//! en el motor de análisis de `grafito_geometry` según el tipo de objeto. El
//! despacho es polimórfico: cada variante de `GeoObject` 2D tiene su propia
//! rutina; las variantes 3D, estadísticas y de texto devuelven lista vacía.

use grafito_geometry::analysis::{
    analyze_circle, analyze_ellipse, analyze_function, analyze_hyperbola, analyze_implicit_curve,
    analyze_line, analyze_parabola, analyze_parametric_curve2d, analyze_polar_curve,
    analyze_polygon, analyze_vector_field2d, AnalysisFeature, AnalysisOptions, AnalysisResult,
};
use grafito_geometry::Point2;
use std::collections::HashMap;

use crate::GeoObject;

/// Analiza un objeto geométrico dentro de los límites visibles.
///
/// Devuelve la lista de características que coincidan con `features`. Cada
/// variante de `GeoObject` 2D tiene una rutina dedicada en `grafito-geometry`:
/// - `Function` → muestreo denso + Newton/bisección
/// - `Line`/`Circle`/`Ellipse`/`Parabola`/`Hyperbola` → discriminantes analíticos
/// - `Polygon` → centroide, área y cortes con ejes
/// - `ParametricCurve2D`/`PolarCurve`/`ImplicitCurve` → barridos sobre t o grilla
/// - `VectorField2D` → búsqueda de equilibrios con refinamiento
/// - Resto (3D, estadística, texto) → `Vec::new()`
pub fn analyze_object(
    obj: &GeoObject,
    view_bounds: (f64, f64, f64, f64), // (xmin, xmax, ymin, ymax)
    vars: &HashMap<String, f64>,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let (xmin, xmax, _, _) = view_bounds;
    let opts = AnalysisOptions {
        domain_min: xmin,
        domain_max: xmax,
        samples: 800,
        find_roots: features.contains(&AnalysisFeature::Root)
            || features.contains(&AnalysisFeature::XIntercept),
        find_extrema: features.contains(&AnalysisFeature::LocalMaximum)
            || features.contains(&AnalysisFeature::LocalMinimum),
        find_inflections: features.contains(&AnalysisFeature::Inflection),
        find_y_intercept: features.contains(&AnalysisFeature::YIntercept),
        find_asymptotes: features.iter().any(|f| {
            matches!(
                f,
                AnalysisFeature::VerticalAsymptote
                    | AnalysisFeature::HorizontalAsymptote
                    | AnalysisFeature::ObliqueAsymptote
            )
        }),
    };

    match obj {
        GeoObject::Function(f) => analyze_function(&f.expr, vars, &opts),
        GeoObject::Line(l) => analyze_line(l.start, l.end, features),
        GeoObject::Circle(c) => analyze_circle(c.center, c.radius, features),
        GeoObject::Ellipse(e) => analyze_ellipse(e.center, e.rx, e.ry, e.angle, features),
        GeoObject::Parabola(p) => analyze_parabola(p.vertex, p.p, features),
        GeoObject::Hyperbola(h) => analyze_hyperbola(h.center, h.a, h.b, h.horizontal, features),
        GeoObject::Polygon(p) => analyze_polygon(&p.vertices, features),
        GeoObject::ParametricCurve2D(c) => {
            analyze_parametric_curve2d(&c.expr_x, &c.expr_y, c.t_min, c.t_max, vars, features)
        }
        GeoObject::PolarCurve(c) => {
            analyze_polar_curve(&c.expr_r, c.t_min, c.t_max, vars, features)
        }
        GeoObject::ImplicitCurve(c) => {
            analyze_implicit_curve(&c.expr_lhs, &c.expr_rhs, view_bounds, vars, features)
        }
        GeoObject::VectorField2D(v) => {
            analyze_vector_field2d(&v.expr_u, &v.expr_v, view_bounds, vars, features)
        }
        _ => Vec::new(),
    }
}

/// Evalúa una función o curva en un punto del plano y devuelve el valor
/// vertical (función explícita), la coordenada x de la curva paramétrica en
/// el t más cercano, o la distancia al círculo/línea más cercana.
pub fn evaluate_curve_at(
    obj: &GeoObject,
    world: Point2,
    vars: &HashMap<String, f64>,
) -> Option<f64> {
    match obj {
        GeoObject::Function(f) => {
            grafito_geometry::expr::eval_function_with_vars(&f.expr, world.x, vars).ok()
        }
        GeoObject::Circle(c) => Some(c.center.distance(&world) - c.radius),
        GeoObject::Line(l) => {
            // distancia con signo usando el producto cruzado normalizado.
            let dx = l.end.x - l.start.x;
            let dy = l.end.y - l.start.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-12 {
                return None;
            }
            let nx = -dy / len;
            let ny = dx / len;
            Some((world.x - l.start.x) * nx + (world.y - l.start.y) * ny)
        }
        GeoObject::ParametricCurve2D(c) => {
            // Muestreo + bisección rápida para encontrar el t más cercano.
            let n = 200;
            let (mut best_t, mut best_d2) = (c.t_min, f64::INFINITY);
            for i in 0..=n {
                let t = c.t_min + (i as f64 / n as f64) * (c.t_max - c.t_min);
                if let (Some(x), Some(y)) = (
                    grafito_geometry::expr::eval_batch_1d(&c.expr_x, "t", std::iter::once(t), vars)
                        .ok()
                        .and_then(|mut v| v.pop().flatten()),
                    grafito_geometry::expr::eval_batch_1d(&c.expr_y, "t", std::iter::once(t), vars)
                        .ok()
                        .and_then(|mut v| v.pop().flatten()),
                ) {
                    let d2 = (x - world.x).powi(2) + (y - world.y).powi(2);
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best_t = t;
                    }
                }
            }
            Some(best_t)
        }
        _ => None,
    }
}

/// Características por defecto para análisis general.
pub fn default_analysis_features() -> Vec<AnalysisFeature> {
    vec![
        AnalysisFeature::Root,
        AnalysisFeature::XIntercept,
        AnalysisFeature::YIntercept,
        AnalysisFeature::LocalMaximum,
        AnalysisFeature::LocalMinimum,
        AnalysisFeature::Inflection,
        AnalysisFeature::Centroid,
        AnalysisFeature::VerticalAsymptote,
        AnalysisFeature::HorizontalAsymptote,
    ]
}

/// Características de "clic rápido": solo lo que el usuario suele querer al
/// señalar sobre un objeto.
pub fn quick_analysis_features() -> Vec<AnalysisFeature> {
    vec![
        AnalysisFeature::XIntercept,
        AnalysisFeature::YIntercept,
        AnalysisFeature::Centroid,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CircleObj, LineObj, PointObj, PolygonObj};

    fn empty_vars() -> HashMap<String, f64> {
        HashMap::new()
    }

    fn bounds() -> (f64, f64, f64, f64) {
        (-10.0, 10.0, -10.0, 10.0)
    }

    #[test]
    fn analyze_object_dispatches_circle() {
        // Círculo unidad en (0,0) corta X en (±1, 0) y Y en (0, ±1).
        let c = GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0));
        let r = analyze_object(
            &c,
            bounds(),
            &empty_vars(),
            &[AnalysisFeature::XIntercept, AnalysisFeature::YIntercept],
        );
        assert!(r.iter().any(|x| x.feature == AnalysisFeature::XIntercept));
        assert!(r.iter().any(|x| x.feature == AnalysisFeature::YIntercept));
    }

    #[test]
    fn analyze_object_dispatches_polygon() {
        let tri = GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(2.0, 3.0),
        ]));
        let r = analyze_object(&tri, bounds(), &empty_vars(), &[AnalysisFeature::Centroid]);
        let c = r
            .iter()
            .find(|x| x.feature == AnalysisFeature::Centroid)
            .expect("centroide");
        assert!((c.point.x - 2.0).abs() < 1e-9);
        assert!((c.point.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn analyze_object_dispatches_line() {
        // Línea y = x (segmento de (0,0) a (2,2)): corta X en (0,0) y Y en (0,0).
        let l = GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(2.0, 2.0)));
        let r = analyze_object(
            &l,
            bounds(),
            &empty_vars(),
            &[AnalysisFeature::XIntercept, AnalysisFeature::YIntercept],
        );
        assert!(r.iter().any(|x| x.feature == AnalysisFeature::XIntercept));
        assert!(r.iter().any(|x| x.feature == AnalysisFeature::YIntercept));
    }

    #[test]
    fn evaluate_curve_at_function() {
        let p = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let f = GeoObject::Function(crate::FunctionObj::new("2*x + 1".to_string()));
        let y = evaluate_curve_at(&f, Point2::new(3.0, 0.0), &empty_vars());
        assert!(y.is_some());
        assert!((y.unwrap() - 7.0).abs() < 1e-9);
        let _ = p;
    }
}
