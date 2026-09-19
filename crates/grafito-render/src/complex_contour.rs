//! Camino de un contorno complejo: convierte un objeto del documento en la
//! polilínea (plano complejo) que integran `ComplexIntegral`/`Gauss`.
//!
//! Puro y determinista: sin GPU, sin estado. `None` cuando el objeto no es un
//! contorno soportado o no produce al menos dos puntos finitos (el caller no
//! dibuja valor, en vez de mentir con un 0).

use std::collections::BTreeMap;

use grafito_core::{Document, GeoObject};
use grafito_geometry::expr::eval_batch_1d;
use num_complex::Complex64;

/// Muestras de una curva paramétrica (t ∈ [t_min, t_max]).
pub const PARAMETRIC_CONTOUR_SAMPLES: usize = 256;
/// Muestras por segmento de una spline (Catmull-Rom).
pub const SPLINE_CONTOUR_STEPS_PER_SEGMENT: usize = 16;
/// Muestras de un arco.
pub const ARC_CONTOUR_STEPS: usize = 256;
/// Muestras de una curva Bézier (De Casteljau).
pub const BEZIER_CONTOUR_STEPS: usize = 256;

/// Polilínea del contorno del objeto tal como la ve el integrador.
///
/// - `Polygon`: cierra repitiendo el primer vértice (lazo), como el render.
/// - `Circle`: NO se muestrea acá; el caller usa la integral analítica
///   (`circle_contour_integral`), exacta y más barata.
/// - `Polyline`/`Pencil`: puntos crudos (el integrador remuestrea si exceden
///   `MAX_QUADRATURE_SEGMENTS`).
/// - `Spline`/`BezierCurve`/`Arc`: muestreo del modelo.
/// - `ParametricCurve2D`: evalúa `(x(t), y(t))` con las variables del
///   documento; descarta muestras no finitas.
#[allow(clippy::too_many_lines)]
pub fn contour_path(target: &GeoObject, document: &Document) -> Option<Vec<Complex64>> {
    let path: Vec<Complex64> = match target {
        GeoObject::Polygon(polygon) => {
            let mut points: Vec<Complex64> = polygon
                .vertices
                .iter()
                .map(|pt| Complex64::new(pt.x, pt.y))
                .collect();
            if let Some(first) = polygon.vertices.first() {
                points.push(Complex64::new(first.x, first.y));
            }
            points
        }
        GeoObject::Line(line) => vec![
            Complex64::new(line.start.x, line.start.y),
            Complex64::new(line.end.x, line.end.y),
        ],
        GeoObject::Polyline(polyline) => polyline
            .points
            .iter()
            .map(|pt| Complex64::new(pt.x, pt.y))
            .collect(),
        GeoObject::Pencil(pencil) => pencil
            .points
            .iter()
            .map(|pt| Complex64::new(pt.x, pt.y))
            .collect(),
        GeoObject::Spline(spline) => spline
            .sample_points(SPLINE_CONTOUR_STEPS_PER_SEGMENT)
            .iter()
            .map(|pt| Complex64::new(pt.x, pt.y))
            .collect(),
        GeoObject::Arc(arc) => arc
            .sample_points(ARC_CONTOUR_STEPS)
            .iter()
            .map(|pt| Complex64::new(pt.x, pt.y))
            .collect(),
        GeoObject::BezierCurve(bezier) => bezier
            .sample_points(BEZIER_CONTOUR_STEPS)
            .iter()
            .map(|pt| Complex64::new(pt.x, pt.y))
            .collect(),
        GeoObject::ParametricCurve2D(curve) => {
            let samples = parametric_samples(curve, document)?;
            samples
                .iter()
                .map(|pt| Complex64::new(pt.x, pt.y))
                .collect()
        }
        _ => return None,
    };
    (path.len() >= 2 && path.iter().all(|z| z.re.is_finite() && z.im.is_finite())).then_some(path)
}

/// Evalúa una paramétrica en `t_min..=t_max` con las variables del documento.
fn parametric_samples(
    curve: &grafito_core::ParametricCurve2DObj,
    document: &Document,
) -> Option<Vec<grafito_geometry::Point2>> {
    if !(curve.t_min.is_finite() && curve.t_max.is_finite()) || curve.t_max <= curve.t_min {
        return None;
    }
    let vars: BTreeMap<String, f64> = document.variables.clone();
    let xs_iter = (0..=PARAMETRIC_CONTOUR_SAMPLES).map(|i| {
        curve.t_min + (curve.t_max - curve.t_min) * i as f64 / PARAMETRIC_CONTOUR_SAMPLES as f64
    });
    let xs = eval_batch_1d(&curve.expr_x, "t", xs_iter.clone(), &vars).ok()?;
    let ys = eval_batch_1d(&curve.expr_y, "t", xs_iter, &vars).ok()?;
    let mut points = Vec::with_capacity(xs.len());
    for (x, y) in xs.into_iter().zip(ys) {
        if let (Some(x), Some(y)) = (x, y) {
            if x.is_finite() && y.is_finite() {
                points.push(grafito_geometry::Point2::new(x, y));
            }
        }
    }
    (points.len() >= 2).then_some(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{
        ArcObj, CircleObj, Document, ParametricCurve2DObj, PencilObj, PointObj, PolygonObj,
    };
    use grafito_geometry::Point2;

    fn document() -> Document {
        Document::new()
    }

    #[test]
    fn poligono_cierra_y_curvas_muestrean() {
        let polygon = GeoObject::Polygon(PolygonObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
        ]));
        let path = contour_path(&polygon, &document()).expect("polígono");
        assert_eq!(path.len(), 4, "cierra repitiendo el primero");
        assert_eq!(path.first(), path.last());

        let arc = GeoObject::Arc(ArcObj::new(
            Point2::new(0.0, 0.0),
            1.0,
            0.0,
            std::f64::consts::PI,
        ));
        let path = contour_path(&arc, &document()).expect("arco");
        assert_eq!(path.len(), ARC_CONTOUR_STEPS + 1);
        assert!((path[0] - Complex64::new(1.0, 0.0)).norm() < 1e-12);
    }

    #[test]
    fn escalares_no_son_contorno() {
        let circle = GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 1.0));
        // El círculo se integra analíticamente, no como polilínea.
        assert!(contour_path(&circle, &document()).is_none());
        let point = GeoObject::Point(PointObj::new(Point2::new(1.0, 2.0)));
        assert!(contour_path(&point, &document()).is_none());
    }

    #[test]
    fn pencil_y_parametrica_producen_camino() {
        let pencil = GeoObject::Pencil(PencilObj::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(2.0, 0.0),
        ]));
        let path = contour_path(&pencil, &document()).expect("pencil");
        assert_eq!(path.len(), 3);

        let mut doc = document();
        doc.variables.insert("a".to_string(), 2.0);
        let parametric = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
            "a*cos(t)",
            "a*sin(t)",
            0.0,
            std::f64::consts::TAU,
        ));
        let path = contour_path(&parametric, &doc).expect("paramétrica");
        assert_eq!(path.len(), PARAMETRIC_CONTOUR_SAMPLES + 1);
        // Primer y último punto sobre la circunferencia de radio a = 2.
        assert!((path[0].norm() - 2.0).abs() < 1e-9);
        assert!((path[path.len() - 1].norm() - 2.0).abs() < 1e-9);
    }
}
