//! Geometría analítica 3D: planos y rectas infinitas.
//!
//! Proporciona [`Plane3D`] (plano `ax + by + cz + d = 0`) y [`Line3D`]
//! (recta punto + dirección), con distancia de punto a plano/recta y
//! utilidades para geometría analítica universitaria.

use crate::types3d::Point3D;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Plano en 3D representado como `ax + by + cz + d = 0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plane3D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

impl Plane3D {
    /// Crea un plano a partir de los coeficientes `ax + by + cz + d = 0`.
    pub fn from_equation(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self { a, b, c, d }
    }

    /// Crea un plano a partir de tres puntos no colineales.
    pub fn from_three_points(p1: Point3D, p2: Point3D, p3: Point3D) -> Self {
        let v1 = Vec3::new(
            (p2.x - p1.x) as f32,
            (p2.y - p1.y) as f32,
            (p2.z - p1.z) as f32,
        );
        let v2 = Vec3::new(
            (p3.x - p1.x) as f32,
            (p3.y - p1.y) as f32,
            (p3.z - p1.z) as f32,
        );
        let n = v1.cross(v2);
        let a = n.x as f64;
        let b = n.y as f64;
        let c = n.z as f64;
        let d = -(a * p1.x + b * p1.y + c * p1.z);
        Self { a, b, c, d }
    }

    /// Crea un plano a partir de un punto y un vector normal.
    pub fn from_point_and_normal(point: Point3D, normal: Vec3) -> Self {
        let a = normal.x as f64;
        let b = normal.y as f64;
        let c = normal.z as f64;
        let d = -(a * point.x + b * point.y + c * point.z);
        Self { a, b, c, d }
    }

    /// Devuelve el vector normal (no normalizado) como `Vec3`.
    pub fn normal(&self) -> Vec3 {
        Vec3::new(self.a as f32, self.b as f32, self.c as f32)
    }

    /// Normaliza la ecuación del plano para que |normal| = 1.
    pub fn normalized(&self) -> Self {
        let norm = (self.a * self.a + self.b * self.b + self.c * self.c).sqrt();
        if norm < 1e-15 {
            return *self;
        }
        Self {
            a: self.a / norm,
            b: self.b / norm,
            c: self.c / norm,
            d: self.d / norm,
        }
    }

    /// Distancia con signo del punto al plano (positiva del lado del normal).
    pub fn signed_distance_to_point(&self, p: Point3D) -> f64 {
        (self.a * p.x + self.b * p.y + self.c * p.z + self.d)
            / (self.a * self.a + self.b * self.b + self.c * self.c).sqrt()
    }

    /// Distancia (absoluta) del punto al plano.
    pub fn distance_to_point(&self, p: Point3D) -> f64 {
        self.signed_distance_to_point(p).abs()
    }

    /// Proyecta un punto sobre el plano.
    pub fn project_point(&self, p: Point3D) -> Point3D {
        let n = self.normalized();
        let dist = n.signed_distance_to_point(p);
        Point3D::new(p.x - n.a * dist, p.y - n.b * dist, p.z - n.c * dist)
    }
}

/// Recta infinita en 3D: punto de paso + dirección.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line3D {
    pub point: Point3D,
    pub direction: Point3D,
}

impl Line3D {
    /// Crea una recta a partir de un punto y un vector dirección.
    pub fn from_point_and_direction(point: Point3D, direction: Point3D) -> Self {
        Self { point, direction }
    }

    /// Crea una recta a partir de dos puntos distintos.
    pub fn from_two_points(a: Point3D, b: Point3D) -> Self {
        Self {
            point: a,
            direction: Point3D::new(b.x - a.x, b.y - a.y, b.z - a.z),
        }
    }

    /// Vector dirección normalizado como `Vec3`.
    pub fn direction_vec3(&self) -> Vec3 {
        let d = self.direction;
        let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
        if len < 1e-15 {
            return Vec3::ZERO;
        }
        Vec3::new((d.x / len) as f32, (d.y / len) as f32, (d.z / len) as f32)
    }

    /// Distancia de un punto a la recta infinita.
    ///
    /// Fórmula: `|PQ × d| / |d|` donde Q es el punto de paso y d la dirección.
    pub fn distance_to_point(&self, p: Point3D) -> f64 {
        let q = self.point;
        let d = self.direction;

        // PQ = P - Q
        let pqx = p.x - q.x;
        let pqy = p.y - q.y;
        let pqz = p.z - q.z;

        // cross = PQ × d
        let cx = pqy * d.z - pqz * d.y;
        let cy = pqz * d.x - pqx * d.z;
        let cz = pqx * d.y - pqy * d.x;

        let cross_len = (cx * cx + cy * cy + cz * cz).sqrt();
        let d_len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();

        if d_len < 1e-15 {
            return p.distance(&q);
        }
        cross_len / d_len
    }

    /// Punto más cercano de la recta al punto `p`.
    pub fn closest_point_to(&self, p: Point3D) -> Point3D {
        let q = self.point;
        let d = self.direction;
        let d_len_sq = d.x * d.x + d.y * d.y + d.z * d.z;
        if d_len_sq < 1e-15 {
            return q;
        }
        // t = dot(PQ, d) / |d|^2  con PQ = P - Q
        let t = ((p.x - q.x) * d.x + (p.y - q.y) * d.y + (p.z - q.z) * d.z) / d_len_sq;
        Point3D::new(q.x + t * d.x, q.y + t * d.y, q.z + t * d.z)
    }
}

/// Resultado de intersectar dos planos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanePlaneIntersection {
    Line(Line3D),
    ParallelDistinct,
    Coincident,
    Degenerate,
}

/// Resultado de proyectar una recta sobre un plano.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineProjectionOnPlane {
    Line(Line3D),
    Point(Point3D),
    DegenerateLine,
    DegeneratePlane,
}

/// Relación geométrica entre dos rectas 3D.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineLineRelation {
    Intersecting(Point3D),
    ParallelDistinct,
    Coincident,
    Skew {
        closest_on_first: Point3D,
        closest_on_second: Point3D,
        distance: f64,
    },
    Degenerate,
}

/// Resultado de construir un plano que contenga dos rectas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaneThroughLines {
    Plane(Plane3D),
    Skew,
    CoincidentLines,
    DegenerateLine,
}

/// Intersecta dos planos `ax + by + cz + d = 0`.
pub fn intersect_planes(p1: Plane3D, p2: Plane3D, eps: f64) -> PlanePlaneIntersection {
    let n1 = (p1.a, p1.b, p1.c);
    let n2 = (p2.a, p2.b, p2.c);
    let n1_sq = dot3(n1, n1);
    let n2_sq = dot3(n2, n2);
    if n1_sq <= eps * eps || n2_sq <= eps * eps {
        return PlanePlaneIntersection::Degenerate;
    }

    let dir = cross3(n1, n2);
    let dir_sq = dot3(dir, dir);
    if dir_sq <= eps * eps * n1_sq.max(n2_sq).max(1.0) {
        if planes_coincident(p1, p2, eps) {
            PlanePlaneIntersection::Coincident
        } else {
            PlanePlaneIntersection::ParallelDistinct
        }
    } else {
        let v = (
            p2.d * n1.0 - p1.d * n2.0,
            p2.d * n1.1 - p1.d * n2.1,
            p2.d * n1.2 - p1.d * n2.2,
        );
        let point = div3(cross3(v, dir), dir_sq);
        PlanePlaneIntersection::Line(Line3D::from_point_and_direction(
            point_from_tuple(point),
            point_from_tuple(dir),
        ))
    }
}

/// Proyecta ortogonalmente una recta sobre un plano.
pub fn project_line_onto_plane(line: Line3D, plane: Plane3D, eps: f64) -> LineProjectionOnPlane {
    let n = (plane.a, plane.b, plane.c);
    let n_sq = dot3(n, n);
    if n_sq <= eps * eps {
        return LineProjectionOnPlane::DegeneratePlane;
    }
    let d = point_to_tuple(line.direction);
    let d_sq = dot3(d, d);
    if d_sq <= eps * eps {
        return LineProjectionOnPlane::DegenerateLine;
    }

    let projected_point = plane.project_point(line.point);
    let factor = dot3(d, n) / n_sq;
    let projected_dir = sub3(d, mul3(n, factor));
    if dot3(projected_dir, projected_dir) <= eps * eps * d_sq.max(1.0) {
        LineProjectionOnPlane::Point(projected_point)
    } else {
        LineProjectionOnPlane::Line(Line3D::from_point_and_direction(
            projected_point,
            point_from_tuple(projected_dir),
        ))
    }
}

/// Clasifica dos rectas 3D e identifica intersección o puntos más cercanos.
pub fn line_line_relation(l1: Line3D, l2: Line3D, eps: f64) -> LineLineRelation {
    let p1 = point_to_tuple(l1.point);
    let p2 = point_to_tuple(l2.point);
    let d1 = point_to_tuple(l1.direction);
    let d2 = point_to_tuple(l2.direction);
    let a = dot3(d1, d1);
    let c = dot3(d2, d2);
    if a <= eps * eps || c <= eps * eps {
        return LineLineRelation::Degenerate;
    }

    let between = sub3(p2, p1);
    let cross_dir = cross3(d1, d2);
    let cross_sq = dot3(cross_dir, cross_dir);
    let scale = a.max(c).max(1.0);

    if cross_sq <= eps * eps * scale {
        let offset_cross = cross3(between, d1);
        if dot3(offset_cross, offset_cross) <= eps * eps * a.max(1.0) {
            LineLineRelation::Coincident
        } else {
            LineLineRelation::ParallelDistinct
        }
    } else {
        let w0 = sub3(p1, p2);
        let b = dot3(d1, d2);
        let d = dot3(d1, w0);
        let e = dot3(d2, w0);
        let denom = a * c - b * b;
        if denom.abs() <= eps * eps * scale {
            return LineLineRelation::Degenerate;
        }
        let s = (b * e - c * d) / denom;
        let t = (a * e - b * d) / denom;
        let c1 = add3(p1, mul3(d1, s));
        let c2 = add3(p2, mul3(d2, t));
        let delta = sub3(c1, c2);
        let distance = dot3(delta, delta).sqrt();
        if distance <= eps * (c1_norm(c1).max(c1_norm(c2)).max(1.0)) {
            LineLineRelation::Intersecting(point_from_tuple(mul3(add3(c1, c2), 0.5)))
        } else {
            LineLineRelation::Skew {
                closest_on_first: point_from_tuple(c1),
                closest_on_second: point_from_tuple(c2),
                distance,
            }
        }
    }
}

/// Indica si dos rectas son perpendiculares. Si `require_intersection` es true,
/// también exige que se corten en un punto.
pub fn lines_are_perpendicular(
    l1: Line3D,
    l2: Line3D,
    require_intersection: bool,
    eps: f64,
) -> bool {
    let d1 = point_to_tuple(l1.direction);
    let d2 = point_to_tuple(l2.direction);
    let len = (dot3(d1, d1) * dot3(d2, d2)).sqrt();
    if len <= eps {
        return false;
    }
    let orthogonal = dot3(d1, d2).abs() <= eps * len.max(1.0);
    if !orthogonal {
        return false;
    }
    !require_intersection
        || matches!(
            line_line_relation(l1, l2, eps),
            LineLineRelation::Intersecting(_)
        )
}

/// Construye el plano que contiene dos rectas, si existe y es único.
pub fn plane_through_lines(l1: Line3D, l2: Line3D, eps: f64) -> PlaneThroughLines {
    let d1 = point_to_tuple(l1.direction);
    let d2 = point_to_tuple(l2.direction);
    if dot3(d1, d1) <= eps * eps || dot3(d2, d2) <= eps * eps {
        return PlaneThroughLines::DegenerateLine;
    }

    match line_line_relation(l1, l2, eps) {
        LineLineRelation::Intersecting(p) => {
            let n = cross3(d1, d2);
            if dot3(n, n) <= eps * eps {
                PlaneThroughLines::CoincidentLines
            } else {
                PlaneThroughLines::Plane(Plane3D::from_point_and_normal(
                    p,
                    Vec3::new(n.0 as f32, n.1 as f32, n.2 as f32),
                ))
            }
        }
        LineLineRelation::ParallelDistinct => {
            let between = sub3(point_to_tuple(l2.point), point_to_tuple(l1.point));
            let n = cross3(d1, between);
            if dot3(n, n) <= eps * eps {
                PlaneThroughLines::CoincidentLines
            } else {
                PlaneThroughLines::Plane(Plane3D::from_point_and_normal(
                    l1.point,
                    Vec3::new(n.0 as f32, n.1 as f32, n.2 as f32),
                ))
            }
        }
        LineLineRelation::Coincident => PlaneThroughLines::CoincidentLines,
        LineLineRelation::Skew { .. } => PlaneThroughLines::Skew,
        LineLineRelation::Degenerate => PlaneThroughLines::DegenerateLine,
    }
}

fn planes_coincident(p1: Plane3D, p2: Plane3D, eps: f64) -> bool {
    let n1 = (p1.a, p1.b, p1.c);
    let n2 = (p2.a, p2.b, p2.c);
    let scale = c1_norm((p1.a, p1.b, p1.c))
        .max(c1_norm((p2.a, p2.b, p2.c)))
        .max(1.0);
    if dot3(cross3(n1, n2), cross3(n1, n2)) > eps * eps * scale * scale {
        return false;
    }
    let coeffs = [(p1.a, p2.a), (p1.b, p2.b), (p1.c, p2.c), (p1.d, p2.d)];
    let pivot = coeffs
        .iter()
        .find(|(a, b)| a.abs().max(b.abs()) > eps)
        .copied();
    let Some((a0, b0)) = pivot else {
        return true;
    };
    coeffs
        .iter()
        .all(|(a, b)| (a * b0 - b * a0).abs() <= eps * scale.max(a.abs()).max(b.abs()).max(1.0))
}

fn point_to_tuple(p: Point3D) -> (f64, f64, f64) {
    (p.x, p.y, p.z)
}

fn point_from_tuple(v: (f64, f64, f64)) -> Point3D {
    Point3D::new(v.0, v.1, v.2)
}

fn dot3(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn cross3(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

fn add3(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

fn sub3(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn mul3(a: (f64, f64, f64), s: f64) -> (f64, f64, f64) {
    (a.0 * s, a.1 * s, a.2 * s)
}

fn div3(a: (f64, f64, f64), s: f64) -> (f64, f64, f64) {
    (a.0 / s, a.1 / s, a.2 / s)
}

fn c1_norm(a: (f64, f64, f64)) -> f64 {
    (a.0 * a.0 + a.1 * a.1 + a.2 * a.2).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plane_from_equation() {
        let plane = Plane3D::from_equation(1.0, 0.0, 1.0, 4.0); // x + z + 4 = 0
        assert!((plane.a - 1.0).abs() < 1e-12);
        assert!((plane.b - 0.0).abs() < 1e-12);
        assert!((plane.c - 1.0).abs() < 1e-12);
        assert!((plane.d - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_plane_distance_point() {
        // Plano x + z + 4 = 0, punto (0, 0, 0)
        let plane = Plane3D::from_equation(1.0, 0.0, 1.0, 4.0);
        let p = Point3D::new(0.0, 0.0, 0.0);
        let dist = plane.distance_to_point(p);
        // |0 + 0 + 4| / sqrt(2) = 4/sqrt(2) = 2*sqrt(2)
        let expected = 4.0 / 2.0_f64.sqrt();
        assert!(
            (dist - expected).abs() < 1e-10,
            "dist={} expected={}",
            dist,
            expected
        );
    }

    #[test]
    fn test_plane_from_three_points() {
        // Tres puntos en z=0: (0,0,0), (1,0,0), (0,1,0) → plano z=0
        let p1 = Point3D::new(0.0, 0.0, 0.0);
        let p2 = Point3D::new(1.0, 0.0, 0.0);
        let p3 = Point3D::new(0.0, 1.0, 0.0);
        let plane = Plane3D::from_three_points(p1, p2, p3);
        // Normal debe ser (0, 0, ±1) y d = 0
        assert!((plane.a).abs() < 1e-10);
        assert!((plane.b).abs() < 1e-10);
        assert!((plane.c.abs() - 1.0).abs() < 1e-10);
        assert!((plane.d).abs() < 1e-10);
    }

    #[test]
    fn test_line_from_point_and_direction() {
        let pt = Point3D::new(1.0, 1.0, 2.0);
        let dir = Point3D::new(1.0, 1.0, 0.0);
        let line = Line3D::from_point_and_direction(pt, dir);
        assert!((line.point.x - 1.0).abs() < 1e-12);
        assert!((line.direction.x - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_line_distance_point_on_line() {
        let line = Line3D::from_point_and_direction(
            Point3D::new(1.0, 1.0, 2.0),
            Point3D::new(1.0, 1.0, 0.0),
        );
        // Punto sobre la recta: (2, 2, 2) = (1,1,2) + 1*(1,1,0)
        let p = Point3D::new(2.0, 2.0, 2.0);
        let dist = line.distance_to_point(p);
        assert!(dist < 1e-10, "dist={}", dist);
    }

    #[test]
    fn test_line_distance_point_off_line() {
        // Recta: (1,1,2) + t(1,1,0). Punto (0,0,0).
        let line = Line3D::from_point_and_direction(
            Point3D::new(1.0, 1.0, 2.0),
            Point3D::new(1.0, 1.0, 0.0),
        );
        let p = Point3D::new(0.0, 0.0, 0.0);
        let dist = line.distance_to_point(p);
        // PQ = (-1,-1,-2), d=(1,1,0)
        // cross = PQ × d = (-1*0-(-2)*1, -2*1-(-1)*0, -1*1-(-1)*1) = (2, -2, 0)
        // |cross| = sqrt(8) = 2*sqrt(2)
        // |d| = sqrt(2)
        // dist = 2*sqrt(2)/sqrt(2) = 2
        assert!((dist - 2.0).abs() < 1e-10, "dist={} expected=2.0", dist);
    }

    #[test]
    fn test_line_closest_point() {
        let line = Line3D::from_point_and_direction(
            Point3D::new(1.0, 1.0, 2.0),
            Point3D::new(1.0, 1.0, 0.0),
        );
        let p = Point3D::new(0.0, 0.0, 0.0);
        let closest = line.closest_point_to(p);
        // t = dot(PQ,d)/|d|^2 = dot((-1,-1,-2),(1,1,0))/2 = (-1-1)/2 = -1
        // closest = (1,1,2) + (-1)*(1,1,0) = (0,0,2)
        assert!((closest.x - 0.0).abs() < 1e-10);
        assert!((closest.y - 0.0).abs() < 1e-10);
        assert!((closest.z - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_university_problem_distances() {
        // Problema: P=(0,y,0) equidistante de π: x+z+4=0 y r: (1,1,2)+β(1,1,0)
        let plane = Plane3D::from_equation(1.0, 0.0, 1.0, 4.0);
        let line = Line3D::from_point_and_direction(
            Point3D::new(1.0, 1.0, 2.0),
            Point3D::new(1.0, 1.0, 0.0),
        );

        // P = (0, 2*sqrt(2), 0) → solución esperada
        let y = 2.0 * 2.0_f64.sqrt();
        let p = Point3D::new(0.0, y, 0.0);

        let d_plane = plane.distance_to_point(p);
        let d_line = line.distance_to_point(p);

        assert!(
            (d_plane - d_line).abs() < 1e-9,
            "d_plane={} d_line={}",
            d_plane,
            d_line
        );
        // Ambas deben ser 2*sqrt(2)
        let expected = 2.0 * 2.0_f64.sqrt();
        assert!((d_plane - expected).abs() < 1e-9, "d_plane={}", d_plane);
    }

    #[test]
    fn test_intersect_planes_returns_line() {
        let px = Plane3D::from_equation(1.0, 0.0, 0.0, -1.0); // x = 1
        let py = Plane3D::from_equation(0.0, 1.0, 0.0, -2.0); // y = 2
        match intersect_planes(px, py, 1e-10) {
            PlanePlaneIntersection::Line(line) => {
                assert!((line.point.x - 1.0).abs() < 1e-10, "{:?}", line.point);
                assert!((line.point.y - 2.0).abs() < 1e-10, "{:?}", line.point);
                assert!(line.direction.z.abs() > 0.9, "{:?}", line.direction);
            }
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn test_project_line_onto_plane_collapses_perpendicular_line() {
        let plane = Plane3D::from_equation(0.0, 0.0, 1.0, 0.0); // z = 0
        let line = Line3D::from_point_and_direction(
            Point3D::new(1.0, 2.0, 3.0),
            Point3D::new(0.0, 0.0, 1.0),
        );
        match project_line_onto_plane(line, plane, 1e-10) {
            LineProjectionOnPlane::Point(p) => {
                assert!((p.x - 1.0).abs() < 1e-10);
                assert!((p.y - 2.0).abs() < 1e-10);
                assert!(p.z.abs() < 1e-10);
            }
            other => panic!("expected point projection, got {other:?}"),
        }
    }

    #[test]
    fn test_line_line_relation_intersecting_and_skew() {
        let l1 = Line3D::from_point_and_direction(
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(1.0, 0.0, 0.0),
        );
        let l2 = Line3D::from_point_and_direction(
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(0.0, 1.0, 0.0),
        );
        assert!(matches!(
            line_line_relation(l1, l2, 1e-10),
            LineLineRelation::Intersecting(_)
        ));

        let skew = Line3D::from_point_and_direction(
            Point3D::new(0.0, 1.0, 1.0),
            Point3D::new(0.0, 1.0, 0.0),
        );
        assert!(matches!(
            line_line_relation(l1, skew, 1e-10),
            LineLineRelation::Skew { .. }
        ));
    }

    #[test]
    fn test_plane_through_intersecting_lines() {
        let l1 = Line3D::from_point_and_direction(
            Point3D::new(1.0, 0.0, 0.0),
            Point3D::new(1.0, 1.0, 1.0),
        );
        let l2 = Line3D::from_point_and_direction(
            Point3D::new(1.0, 0.0, 0.0),
            Point3D::new(0.0, 1.0, -1.0),
        );
        match plane_through_lines(l1, l2, 1e-10) {
            PlaneThroughLines::Plane(p) => {
                // Equivalent to 2x - y - z - 2 = 0.
                assert!(p.distance_to_point(Point3D::new(1.0, 0.0, 0.0)) < 1e-9);
                assert!(p.distance_to_point(Point3D::new(2.0, 1.0, 1.0)) < 1e-9);
                assert!(p.distance_to_point(Point3D::new(1.0, 1.0, -1.0)) < 1e-9);
            }
            other => panic!("expected plane, got {other:?}"),
        }
    }
}
