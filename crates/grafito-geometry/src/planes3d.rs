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
    pub fn from_three_points(p1: Point3D, p2: Point3D, p3: Point3D) -> Option<Self> {
        let v1 = (p2.x - p1.x, p2.y - p1.y, p2.z - p1.z);
        let v2 = (p3.x - p1.x, p3.y - p1.y, p3.z - p1.z);
        let n = cross3(v1, v2);
        if n.0.hypot(n.1).hypot(n.2) <= 1e-12 {
            return None;
        }
        Some(Self::from_point_and_normal_f64(p1, n))
    }

    /// Crea un plano a partir de un punto y un vector normal.
    pub fn from_point_and_normal(point: Point3D, normal: Vec3) -> Self {
        Self::from_point_and_normal_f64(point, (normal.x as f64, normal.y as f64, normal.z as f64))
    }

    fn from_point_and_normal_f64(point: Point3D, normal: (f64, f64, f64)) -> Self {
        let (a, b, c) = normal;
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
                PlaneThroughLines::Plane(Plane3D::from_point_and_normal_f64(p, n))
            }
        }
        LineLineRelation::ParallelDistinct => {
            let between = sub3(point_to_tuple(l2.point), point_to_tuple(l1.point));
            let n = cross3(d1, between);
            if dot3(n, n) <= eps * eps {
                PlaneThroughLines::CoincidentLines
            } else {
                PlaneThroughLines::Plane(Plane3D::from_point_and_normal_f64(l1.point, n))
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

// ── G-B: sección plano-poliedro (IntersectPath/Plane honesto) ──
//
// Corta un poliedro (vértices + caras poligonales) con un plano y devuelve
// la sección como polígono ordenado. Todo con tolerancia `eps` explícita y
// `Result`/`Option`: sin plano degenerado, sin caras degeneradas y sin
// reserva sin cota (aristas únicas dedupadas antes de cortar).

/// Sección de un plano con un poliedro.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanePolyhedronSection {
    /// El plano no toca al poliedro (o solo lo roza en un punto/vértice).
    Empty,
    /// Polígono de corte ordenado angularmente en el plano (3+ puntos).
    Polygon { points: Vec<Point3D> },
    /// Una cara yace contenida en el plano: se devuelve tal cual (orden de entrada).
    CoplanarFace {
        face_index: usize,
        points: Vec<Point3D>,
    },
}

/// Error honesto de la sección plano-poliedro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaneSectionError {
    /// El plano tiene normal nula o no finita.
    DegeneratePlane,
    /// La tolerancia no es finita y positiva.
    InvalidTolerance { eps: f64 },
    /// Algún vértice no es finito.
    NonFiniteVertex { index: usize },
    /// Alguna cara referencia un vértice inexistente o tiene menos de 3 vértices.
    InvalidFace { face_index: usize },
}

impl std::fmt::Display for PlaneSectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DegeneratePlane => formatter.write_str("el plano de corte es degenerado"),
            Self::InvalidTolerance { eps } => {
                write!(
                    formatter,
                    "la tolerancia debe ser finita y positiva, era {eps}"
                )
            }
            Self::NonFiniteVertex { index } => {
                write!(formatter, "el vértice {index} no es finito")
            }
            Self::InvalidFace { face_index } => {
                write!(formatter, "la cara {face_index} es inválida")
            }
        }
    }
}

impl std::error::Error for PlaneSectionError {}

fn gb_section_inputs_are_valid(
    vertices: &[Point3D],
    faces: &[Vec<usize>],
    plane: Plane3D,
    eps: f64,
) -> Result<Plane3D, PlaneSectionError> {
    if !eps.is_finite() || eps <= 0.0 {
        return Err(PlaneSectionError::InvalidTolerance { eps });
    }
    let normal_norm = (plane.a * plane.a + plane.b * plane.b + plane.c * plane.c).sqrt();
    if !normal_norm.is_finite() || normal_norm <= eps {
        return Err(PlaneSectionError::DegeneratePlane);
    }
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex.is_finite() {
            return Err(PlaneSectionError::NonFiniteVertex { index });
        }
    }
    for (face_index, face) in faces.iter().enumerate() {
        if face.len() < 3 || face.iter().any(|&index| index >= vertices.len()) {
            return Err(PlaneSectionError::InvalidFace { face_index });
        }
    }
    Ok(plane.normalized())
}

/// Intersecta el plano con el poliedro (`vertices` + `faces` como índices).
///
/// Por cada arista única: extremos sobre el plano (`|d| ≤ eps`) se conservan;
/// cruces estrictos se interpolan. Los puntos se deduplican (`eps`) y ordenan
/// angularmente en el plano. Si una cara yace en el plano → `CoplanarFace`.
pub fn intersect_plane_polyhedron(
    vertices: &[Point3D],
    faces: &[Vec<usize>],
    plane: Plane3D,
    eps: f64,
) -> Result<PlanePolyhedronSection, PlaneSectionError> {
    let plane = gb_section_inputs_are_valid(vertices, faces, plane, eps)?;
    let distances: Vec<f64> = vertices
        .iter()
        .map(|vertex| plane.a * vertex.x + plane.b * vertex.y + plane.c * vertex.z + plane.d)
        .collect();
    // Cara coplanar: todos sus vértices sobre el plano → sección honesta directa.
    for (face_index, face) in faces.iter().enumerate() {
        let on_plane = face.iter().all(|&index| {
            distances
                .get(index)
                .is_some_and(|distance| distance.abs() <= eps)
        });
        if on_plane {
            let points: Vec<Point3D> = face.iter().map(|&index| vertices[index]).collect();
            return Ok(PlanePolyhedronSection::CoplanarFace { face_index, points });
        }
    }
    // Aristas únicas del poliedro (dedupadas antes de cortar: cota implícita).
    let mut edges = std::collections::BTreeSet::new();
    for face in faces {
        for side in 0..face.len() {
            let first = face[side];
            let second = face[(side + 1) % face.len()];
            edges.insert((first.min(second), first.max(second)));
        }
    }
    let mut raw: Vec<Point3D> = Vec::new();
    for (first, second) in edges {
        let Some((&distance_a, &distance_b)) = distances.get(first).zip(distances.get(second))
        else {
            continue;
        };
        let on_a = distance_a.abs() <= eps;
        let on_b = distance_b.abs() <= eps;
        if on_a && on_b {
            continue;
        }
        if on_a {
            raw.push(vertices[first]);
        } else if on_b {
            raw.push(vertices[second]);
        } else if distance_a.signum() != distance_b.signum() {
            let ratio = distance_a / (distance_a - distance_b);
            if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
                continue;
            }
            let a = vertices[first];
            let b = vertices[second];
            let point = Point3D::new(
                a.x + (b.x - a.x) * ratio,
                a.y + (b.y - a.y) * ratio,
                a.z + (b.z - a.z) * ratio,
            );
            if point.is_finite() {
                raw.push(point);
            }
        }
    }
    // Deduplicación por tolerancia.
    let mut unique: Vec<Point3D> = Vec::with_capacity(raw.len());
    for point in raw {
        if unique
            .iter()
            .all(|kept: &Point3D| kept.distance(&point) > eps)
        {
            unique.push(point);
        }
    }
    if unique.len() < 3 {
        return Ok(PlanePolyhedronSection::Empty);
    }
    // Orden angular en el plano alrededor del centroide.
    let mut centroid = Point3D::new(0.0, 0.0, 0.0);
    for point in &unique {
        centroid.x += point.x;
        centroid.y += point.y;
        centroid.z += point.z;
    }
    let count = unique.len() as f64;
    centroid.x /= count;
    centroid.y /= count;
    centroid.z /= count;
    let normal = (plane.a, plane.b, plane.c);
    let helper: (f64, f64, f64) = if normal.0.abs() < 0.9 {
        (1.0, 0.0, 0.0)
    } else {
        (0.0, 1.0, 0.0)
    };
    let tangent = cross3(normal, helper);
    if dot3(tangent, tangent) <= eps * eps {
        return Ok(PlanePolyhedronSection::Empty);
    }
    let bitangent = cross3(normal, tangent);
    let mut by_angle: Vec<(f64, Point3D)> = Vec::with_capacity(unique.len());
    for point in unique {
        let radial = sub3(point_to_tuple(point), point_to_tuple(centroid));
        let angle = dot3(radial, bitangent).atan2(dot3(radial, tangent));
        if !angle.is_finite() {
            return Ok(PlanePolyhedronSection::Empty);
        }
        by_angle.push((angle, point));
    }
    by_angle.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(PlanePolyhedronSection::Polygon {
        points: by_angle.into_iter().map(|(_, point)| point).collect(),
    })
}

/// Perímetro de una sección poligonal; `None` si no es finito.
pub fn section_perimeter(points: &[Point3D]) -> Option<f64> {
    if points.len() < 3 {
        return None;
    }
    let mut total = 0.0_f64;
    for side in 0..points.len() {
        total += points[side].distance(&points[(side + 1) % points.len()]);
    }
    total.is_finite().then_some(total)
}

/// Área de una sección poligonal plana por abanico desde el vértice 0; `None` si no es finita.
pub fn section_area(points: &[Point3D]) -> Option<f64> {
    if points.len() < 3 {
        return None;
    }
    let origin = points[0].to_dvec3();
    let mut total = 0.0_f64;
    for side in 1..points.len() - 1 {
        let edge_a = points[side].to_dvec3() - origin;
        let edge_b = points[side + 1].to_dvec3() - origin;
        total += edge_a.cross(edge_b).length() * 0.5;
    }
    total.is_finite().then_some(total)
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
        let plane = Plane3D::from_three_points(p1, p2, p3).expect("non-colinear");
        // Normal debe ser (0, 0, ±1) y d = 0
        assert!((plane.a).abs() < 1e-10);
        assert!((plane.b).abs() < 1e-10);
        assert!((plane.c.abs() - 1.0).abs() < 1e-10);
        assert!((plane.d).abs() < 1e-10);
    }

    #[test]
    fn plane_from_three_points_keeps_large_f64_cross_products_finite() {
        let plane = Plane3D::from_three_points(
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(1.0e20, 0.0, 0.0),
            Point3D::new(0.0, 1.0e20, 0.0),
        )
        .expect("non-colinear");
        assert!(plane.a.is_finite() && plane.b.is_finite() && plane.c.is_finite());
        assert!(plane.distance_to_point(Point3D::new(1.0e20, 0.0, 0.0)) < 1e-9);
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

    fn unit_cube() -> (Vec<Point3D>, Vec<Vec<usize>>) {
        let vertices = vec![
            Point3D::new(-1.0, -1.0, -1.0),
            Point3D::new(1.0, -1.0, -1.0),
            Point3D::new(1.0, 1.0, -1.0),
            Point3D::new(-1.0, 1.0, -1.0),
            Point3D::new(-1.0, -1.0, 1.0),
            Point3D::new(1.0, -1.0, 1.0),
            Point3D::new(1.0, 1.0, 1.0),
            Point3D::new(-1.0, 1.0, 1.0),
        ];
        let faces = vec![
            vec![0, 1, 2, 3],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![2, 3, 7, 6],
            vec![0, 3, 7, 4],
            vec![1, 2, 6, 5],
        ];
        (vertices, faces)
    }

    #[test]
    fn section_midplane_of_cube_is_unit_square() {
        let (vertices, faces) = unit_cube();
        let plane = Plane3D::from_equation(0.0, 0.0, 1.0, 0.0);
        match intersect_plane_polyhedron(&vertices, &faces, plane, 1e-9).expect("corte") {
            PlanePolyhedronSection::Polygon { points } => {
                assert_eq!(points.len(), 4, "{points:?}");
                let area = section_area(&points).expect("área");
                assert!((area - 4.0).abs() < 1e-9, "{area}");
                let perimeter = section_perimeter(&points).expect("perímetro");
                assert!((perimeter - 8.0).abs() < 1e-9, "{perimeter}");
                assert!(points.iter().all(|point| point.z.abs() < 1e-9));
            }
            other => panic!("expected polygon, got {other:?}"),
        }
    }

    #[test]
    fn section_diagonal_of_cube_is_regular_hexagon() {
        let (vertices, faces) = unit_cube();
        // Plano x + y + z = 0 por el centro: hexágono regular de lado √2.
        let plane = Plane3D::from_equation(1.0, 1.0, 1.0, 0.0);
        match intersect_plane_polyhedron(&vertices, &faces, plane, 1e-9).expect("corte") {
            PlanePolyhedronSection::Polygon { points } => {
                assert_eq!(points.len(), 6, "{points:?}");
                let mut sides: Vec<f64> = (0..6)
                    .map(|side| points[side].distance(&points[(side + 1) % 6]))
                    .collect();
                sides.sort_by(|a, b| a.total_cmp(b));
                assert!((sides[0] - 2.0_f64.sqrt()).abs() < 1e-9, "{sides:?}");
                assert!((sides[5] - sides[0]).abs() < 1e-9, "{sides:?}");
            }
            other => panic!("expected hexagon, got {other:?}"),
        }
    }

    #[test]
    fn section_miss_and_coplanar_face_are_honest() {
        let (vertices, faces) = unit_cube();
        let far = Plane3D::from_equation(0.0, 0.0, 1.0, -5.0);
        assert_eq!(
            intersect_plane_polyhedron(&vertices, &faces, far, 1e-9).expect("corte"),
            PlanePolyhedronSection::Empty
        );
        let face_plane = Plane3D::from_equation(0.0, 0.0, 1.0, -1.0);
        match intersect_plane_polyhedron(&vertices, &faces, face_plane, 1e-9).expect("corte") {
            PlanePolyhedronSection::CoplanarFace { face_index, points } => {
                assert_eq!(face_index, 1);
                assert_eq!(points.len(), 4);
            }
            other => panic!("expected coplanar face, got {other:?}"),
        }
    }

    #[test]
    fn section_through_pyramid_apex_is_triangle() {
        // Pirámide cuadrada: base z=0 de (-1,-1)..(1,1), ápice (0,0,2).
        let vertices = vec![
            Point3D::new(-1.0, -1.0, 0.0),
            Point3D::new(1.0, -1.0, 0.0),
            Point3D::new(1.0, 1.0, 0.0),
            Point3D::new(-1.0, 1.0, 0.0),
            Point3D::new(0.0, 0.0, 2.0),
        ];
        let faces = vec![
            vec![0, 1, 2, 3],
            vec![0, 1, 4],
            vec![1, 2, 4],
            vec![2, 3, 4],
            vec![3, 0, 4],
        ];
        // Plano y=0 por el ápice y el centro de la base: triángulo base 2, altura 2.
        let plane = Plane3D::from_equation(0.0, 1.0, 0.0, 0.0);
        match intersect_plane_polyhedron(&vertices, &faces, plane, 1e-9).expect("corte") {
            PlanePolyhedronSection::Polygon { points } => {
                assert_eq!(points.len(), 3, "{points:?}");
                let area = section_area(&points).expect("área");
                assert!((area - 2.0).abs() < 1e-9, "{area}");
            }
            other => panic!("expected triangle, got {other:?}"),
        }
    }

    #[test]
    fn section_rejects_degenerate_inputs() {
        let (vertices, faces) = unit_cube();
        let plane = Plane3D::from_equation(0.0, 0.0, 0.0, 1.0);
        assert_eq!(
            intersect_plane_polyhedron(&vertices, &faces, plane, 1e-9),
            Err(PlaneSectionError::DegeneratePlane)
        );
        let good = Plane3D::from_equation(0.0, 0.0, 1.0, 0.0);
        assert_eq!(
            intersect_plane_polyhedron(&vertices, &faces, good, -1.0),
            Err(PlaneSectionError::InvalidTolerance { eps: -1.0 })
        );
        assert_eq!(
            intersect_plane_polyhedron(&vertices, &[vec![0, 99, 1]], good, 1e-9),
            Err(PlaneSectionError::InvalidFace { face_index: 0 })
        );
        let mut bad_vertices = vertices.clone();
        bad_vertices[0] = Point3D::new(f64::NAN, 0.0, 0.0);
        assert_eq!(
            intersect_plane_polyhedron(&bad_vertices, &faces, good, 1e-9),
            Err(PlaneSectionError::NonFiniteVertex { index: 0 })
        );
    }
}
