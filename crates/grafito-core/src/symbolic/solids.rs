//! Volumen y área de sólidos 3D + vistas ortográficas (frente F10-C).
//!
//! Fórmulas exactas para los sólidos paramétricos de GeoGebra
//! (esfera, cubo, cilindro, cono, toro, tetraedro regular, pirámide
//! cuadrada regular y prisma de base plana). Lo que no tiene forma
//! cerrada con los parámetros del objeto (`Quadric`, superficies,
//! curvas) devuelve `None` con estado honesto vía [`solid_measure_status`].

use thiserror::Error;

use crate::GeoObject;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SolidError {
    #[error("medida 3D no finita o no positiva: {what} = {value}")]
    NonPositive { what: &'static str, value: f64 },
}

fn positive(what: &'static str, value: f64) -> Result<f64, SolidError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(SolidError::NonPositive { what, value })
    }
}

fn height_between(a: [f64; 3], b: [f64; 3]) -> Result<f64, SolidError> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    positive("altura", (dx * dx + dy * dy + dz * dz).sqrt())
}

/// Volumen de una esfera de radio `r`: 4/3·π·r³.
pub fn sphere_volume(radius: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    Ok(4.0 / 3.0 * std::f64::consts::PI * r * r * r)
}

/// Área de una esfera de radio `r`: 4·π·r².
pub fn sphere_area(radius: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    Ok(4.0 * std::f64::consts::PI * r * r)
}

/// Volumen de un cubo de arista `s`.
pub fn cube_volume(size: f64) -> Result<f64, SolidError> {
    let s = positive("arista", size)?;
    Ok(s * s * s)
}

/// Área de un cubo de arista `s`: 6·s².
pub fn cube_area(size: f64) -> Result<f64, SolidError> {
    let s = positive("arista", size)?;
    Ok(6.0 * s * s)
}

/// Volumen de un cilindro de radio `r` y altura `h`.
pub fn cylinder_volume(radius: f64, height: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    let h = positive("altura", height)?;
    Ok(std::f64::consts::PI * r * r * h)
}

/// Área total de un cilindro: 2·π·r·(r+h).
pub fn cylinder_area(radius: f64, height: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    let h = positive("altura", height)?;
    Ok(2.0 * std::f64::consts::PI * r * (r + h))
}

/// Volumen de un cono de radio `r` y altura `h`: π·r²·h/3.
pub fn cone_volume(radius: f64, height: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    let h = positive("altura", height)?;
    Ok(std::f64::consts::PI * r * r * h / 3.0)
}

/// Área total de un cono: π·r·(r+g) con generatriz g=√(r²+h²).
pub fn cone_area(radius: f64, height: f64) -> Result<f64, SolidError> {
    let r = positive("radio", radius)?;
    let h = positive("altura", height)?;
    let slant = (r * r + h * h).sqrt();
    Ok(std::f64::consts::PI * r * (r + slant))
}

/// Volumen de un toro: 2·π²·R·r².
pub fn torus_volume(major: f64, minor: f64) -> Result<f64, SolidError> {
    let r_major = positive("radio mayor", major)?;
    let r_minor = positive("radio menor", minor)?;
    Ok(2.0 * std::f64::consts::PI * std::f64::consts::PI * r_major * r_minor * r_minor)
}

/// Área de un toro: 4·π²·R·r.
pub fn torus_area(major: f64, minor: f64) -> Result<f64, SolidError> {
    let r_major = positive("radio mayor", major)?;
    let r_minor = positive("radio menor", minor)?;
    Ok(4.0 * std::f64::consts::PI * std::f64::consts::PI * r_major * r_minor)
}

/// Volumen de un tetraedro regular de arista `a`: a³/(6·√2).
pub fn tetrahedron_volume(edge: f64) -> Result<f64, SolidError> {
    let a = positive("arista", edge)?;
    Ok(a * a * a / (6.0 * std::f64::consts::SQRT_2))
}

/// Área de un tetraedro regular: √3·a².
pub fn tetrahedron_area(edge: f64) -> Result<f64, SolidError> {
    let a = positive("arista", edge)?;
    Ok(3.0_f64.sqrt() * a * a)
}

/// Área de un polígono 3D plano por fórmula de Newell + su normal unitaria.
/// Devuelve `(área, normal)`. Falla si hay menos de 3 vértices o es degenerado.
fn newell_area_normal(vertices: &[[f64; 3]]) -> Option<(f64, [f64; 3])> {
    if vertices.len() < 3 {
        return None;
    }
    let mut normal = [0.0_f64; 3];
    for i in 0..vertices.len() {
        let current = vertices[i];
        let next = vertices[(i + 1) % vertices.len()];
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !length.is_finite() || length <= 1e-12 {
        return None;
    }
    Some((
        0.5 * length,
        [normal[0] / length, normal[1] / length, normal[2] / length],
    ))
}

/// Volumen de un prisma: |área_base × altura_perpendicular|.
/// La altura es la proyección del vector de extrusión sobre la normal.
fn prism_volume_from(base: &[[f64; 3]], direction: [f64; 3]) -> Option<f64> {
    let (area, normal) = newell_area_normal(base)?;
    let height = direction[0] * normal[0] + direction[1] * normal[1] + direction[2] * normal[2];
    if !height.is_finite() {
        return None;
    }
    Some(area * height.abs())
}

/// Volumen exacto del sólido si sus parámetros lo admiten.
pub fn solid_volume(object: &GeoObject) -> Option<f64> {
    match object {
        GeoObject::Sphere3D(o) => sphere_volume(o.radius).ok(),
        GeoObject::Cube3D(o) => cube_volume(o.size).ok(),
        GeoObject::Cylinder3D(o) => {
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.top_center.x, o.top_center.y, o.top_center.z],
            )
            .ok()?;
            cylinder_volume(o.radius, h).ok()
        }
        GeoObject::Cone3D(o) => {
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.apex.x, o.apex.y, o.apex.z],
            )
            .ok()?;
            cone_volume(o.radius, h).ok()
        }
        GeoObject::Torus3D(o) => torus_volume(o.r_major, o.r_minor).ok(),
        GeoObject::Tetrahedron3D(o) => tetrahedron_volume(o.edge_length).ok(),
        GeoObject::Pyramid3D(o) => {
            let base = positive("base", o.base_size).ok()?;
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.apex.x, o.apex.y, o.apex.z],
            )
            .ok()?;
            Some(base * base * h / 3.0)
        }
        GeoObject::Prism3D(o) => {
            let base: Vec<[f64; 3]> = o.base_vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
            prism_volume_from(&base, [o.direction.x, o.direction.y, o.direction.z])
        }
        _ => None,
    }
}

/// Área total exacta del sólido si sus parámetros la admiten.
pub fn solid_area(object: &GeoObject) -> Option<f64> {
    match object {
        GeoObject::Sphere3D(o) => sphere_area(o.radius).ok(),
        GeoObject::Cube3D(o) => cube_area(o.size).ok(),
        GeoObject::Cylinder3D(o) => {
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.top_center.x, o.top_center.y, o.top_center.z],
            )
            .ok()?;
            cylinder_area(o.radius, h).ok()
        }
        GeoObject::Cone3D(o) => {
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.apex.x, o.apex.y, o.apex.z],
            )
            .ok()?;
            cone_area(o.radius, h).ok()
        }
        GeoObject::Torus3D(o) => torus_area(o.r_major, o.r_minor).ok(),
        GeoObject::Tetrahedron3D(o) => tetrahedron_area(o.edge_length).ok(),
        GeoObject::Pyramid3D(o) => {
            let base = positive("base", o.base_size).ok()?;
            let h = height_between(
                [o.base_center.x, o.base_center.y, o.base_center.z],
                [o.apex.x, o.apex.y, o.apex.z],
            )
            .ok()?;
            let slant = (h * h + (base / 2.0) * (base / 2.0)).sqrt();
            Some(base * base + 2.0 * base * slant)
        }
        GeoObject::Prism3D(o) => {
            let base: Vec<[f64; 3]> = o.base_vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
            let (base_area, _) = newell_area_normal(&base)?;
            let extrusion = (o.direction.x * o.direction.x
                + o.direction.y * o.direction.y
                + o.direction.z * o.direction.z)
                .sqrt();
            if !extrusion.is_finite() {
                return None;
            }
            let mut perimeter = 0.0;
            for i in 0..base.len() {
                let a = base[i];
                let b = base[(i + 1) % base.len()];
                let edge =
                    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
                if !edge.is_finite() {
                    return None;
                }
                perimeter += edge;
            }
            Some(2.0 * base_area + perimeter * extrusion)
        }
        _ => None,
    }
}

/// Estado honesto de la medida: exacto o motivo de indisponibilidad.
///
/// La cuádrica con clasificación real de esfera/elipsoide informa su volumen
/// analítico `4/3·π·rx·ry·rz` (el área sigue por integración numérica: el
/// resumen global continúa `None` honesto). El resto de cuádricas mantiene el
/// mensaje de no soportado.
pub fn solid_measure_status(object: &GeoObject) -> &'static str {
    if solid_volume(object).is_some() && solid_area(object).is_some() {
        "exacto"
    } else if let GeoObject::Quadric3D(quadric) = object {
        let coeffs = [
            quadric.a, quadric.b, quadric.c, quadric.d, quadric.e, quadric.f, quadric.g, quadric.h,
            quadric.i, quadric.j,
        ];
        match grafito_geometry::quadrics::classify_quadric(coeffs) {
            Ok(shape)
                if matches!(
                    shape.kind,
                    grafito_geometry::quadrics::QuadricKind::Sphere
                        | grafito_geometry::quadrics::QuadricKind::Ellipsoid
                ) =>
            {
                "elipsoide real: volumen 4/3·π·rx·ry·rz (área por integración numérica)"
            }
            _ => "no soportado: el objeto no es un sólido paramétrico con forma cerrada (usa cuádrica/superficie con integración numérica)",
        }
    } else {
        "no soportado: el objeto no es un sólido paramétrico con forma cerrada (usa cuádrica/superficie con integración numérica)"
    }
}

/// Vista ortográfica al estilo GeoGebra 3D (sin perspectiva).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrthoView {
    /// Plano XY (alzado).
    Front,
    /// Plano XZ (planta).
    Top,
    /// Plano YZ (perfil).
    Side,
}

impl OrthoView {
    /// Nombre estable para UI y comandos.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Front => "alzado",
            Self::Top => "planta",
            Self::Side => "perfil",
        }
    }
}

/// Proyecta un punto 3D a 2D según la vista ortográfica (descarta un eje).
pub fn project_ortho(point: [f64; 3], view: OrthoView) -> (f64, f64) {
    match view {
        OrthoView::Front => (point[0], point[1]),
        OrthoView::Top => (point[0], point[2]),
        OrthoView::Side => (point[1], point[2]),
    }
}

/// Vértices de un cubo eje-alineado de centro dado y arista `size`.
/// Orden binario `(-,-,-)…(+,+,+)`. `None` si no finitos o `size <= 0`.
pub fn cube_vertices(center: [f64; 3], size: f64) -> Option<[[f64; 3]; 8]> {
    if !center.iter().all(|v| v.is_finite()) || !size.is_finite() || size <= 0.0 {
        return None;
    }
    let h = 0.5 * size;
    if !h.is_finite() {
        return None;
    }
    let mut out = [[0.0_f64; 3]; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        let sx = if i & 1 == 0 { -h } else { h };
        let sy = if i & 2 == 0 { -h } else { h };
        let sz = if i & 4 == 0 { -h } else { h };
        let p = [center[0] + sx, center[1] + sy, center[2] + sz];
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        *slot = p;
    }
    Some(out)
}

/// Aristas del cubo por índices en [`cube_vertices`] (12, sin duplicados).
pub fn cube_edges() -> [[usize; 2]; 12] {
    [
        [0, 1],
        [2, 3],
        [4, 5],
        [6, 7],
        [0, 2],
        [1, 3],
        [4, 6],
        [5, 7],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ]
}

/// Vista ortográfica que mejor muestra un plano de normal `normal`:
/// dominante X→perfil, Y→planta, Z→alzado. Espeja
/// `render_3d::OrthoProjection` (piel) para que el polígono 2D no colapse.
pub fn best_ortho_view_for_normal(normal: [f64; 3]) -> OrthoView {
    let ax = normal[0].abs();
    let ay = normal[1].abs();
    let az = normal[2].abs();
    if ax >= ay && ax >= az {
        OrthoView::Side
    } else if ay >= ax && ay >= az {
        OrthoView::Top
    } else {
        OrthoView::Front
    }
}

/// Intersección plano-cubo como polígono 3D ordenado angularmente en el plano.
///
/// Plano `ax+by+cz+d=0` + cubo eje-alineado. Recorre las 12 aristas,
/// interpola los cruces, deduplica (1e-9) y ordena por ángulo alrededor del
/// centroide en una base ortonormal del plano (misma que el círculo
/// plano-esfera). `None` si el plano es degenerado, el cubo es inválido o hay
/// menos de 3 puntos distintos (tangencia en punto/arista: sin polígono).
pub fn plane_cube_section(
    plane: (f64, f64, f64, f64),
    center: [f64; 3],
    size: f64,
) -> Option<Vec<[f64; 3]>> {
    const EPS: f64 = 1e-9;
    const DEDUP_EPS2: f64 = 1e-18;
    let (a, b, c, d) = plane;
    if !a.is_finite() || !b.is_finite() || !c.is_finite() || !d.is_finite() {
        return None;
    }
    let norm_len = (a * a + b * b + c * c).sqrt();
    if !norm_len.is_finite() || norm_len <= 1e-12 {
        return None;
    }
    let vertices = cube_vertices(center, size)?;
    let mut points: Vec<[f64; 3]> = Vec::new();
    for edge in cube_edges() {
        let p0 = vertices[edge[0]];
        let p1 = vertices[edge[1]];
        let v0 = a * p0[0] + b * p0[1] + c * p0[2] + d;
        let v1 = a * p1[0] + b * p1[1] + c * p1[2] + d;
        if !v0.is_finite() || !v1.is_finite() {
            return None;
        }
        if v0.abs() <= EPS && v1.abs() <= EPS {
            for p in [p0, p1] {
                if !points.iter().any(|q| {
                    let dx = q[0] - p[0];
                    let dy = q[1] - p[1];
                    let dz = q[2] - p[2];
                    dx * dx + dy * dy + dz * dz <= DEDUP_EPS2
                }) {
                    points.push(p);
                }
            }
        } else if v0.abs() <= EPS {
            if !points.iter().any(|q| {
                let dx = q[0] - p0[0];
                let dy = q[1] - p0[1];
                let dz = q[2] - p0[2];
                dx * dx + dy * dy + dz * dz <= DEDUP_EPS2
            }) {
                points.push(p0);
            }
        } else if v1.abs() <= EPS {
            if !points.iter().any(|q| {
                let dx = q[0] - p1[0];
                let dy = q[1] - p1[1];
                let dz = q[2] - p1[2];
                dx * dx + dy * dy + dz * dz <= DEDUP_EPS2
            }) {
                points.push(p1);
            }
        } else if v0 * v1 < 0.0 {
            let denom = v0 - v1;
            if !denom.is_finite() || denom.abs() <= 1e-18 {
                continue;
            }
            let t = v0 / denom;
            if !t.is_finite() {
                continue;
            }
            let p = [
                p0[0] + t * (p1[0] - p0[0]),
                p0[1] + t * (p1[1] - p0[1]),
                p0[2] + t * (p1[2] - p0[2]),
            ];
            if !p.iter().all(|v| v.is_finite()) {
                continue;
            }
            if !points.iter().any(|q| {
                let dx = q[0] - p[0];
                let dy = q[1] - p[1];
                let dz = q[2] - p[2];
                dx * dx + dy * dy + dz * dz <= DEDUP_EPS2
            }) {
                points.push(p);
            }
        }
    }
    if points.len() < 3 {
        return None;
    }
    // Orden angular alrededor del centroide en base (u, v) del plano.
    let n = points.len() as f64;
    let mut centroid = [0.0_f64; 3];
    for p in &points {
        centroid[0] += p[0] / n;
        centroid[1] += p[1] / n;
        centroid[2] += p[2] / n;
    }
    let nx = a / norm_len;
    let ny = b / norm_len;
    let nz = c / norm_len;
    let arbitrary = if nx.abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut u = [
        ny * arbitrary[2] - nz * arbitrary[1],
        nz * arbitrary[0] - nx * arbitrary[2],
        nx * arbitrary[1] - ny * arbitrary[0],
    ];
    let ulen = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if !ulen.is_finite() || ulen <= 1e-12 {
        return None;
    }
    u[0] /= ulen;
    u[1] /= ulen;
    u[2] /= ulen;
    let v = [
        ny * u[2] - nz * u[1],
        nz * u[0] - nx * u[2],
        nx * u[1] - ny * u[0],
    ];
    let mut with_angle: Vec<(f64, [f64; 3])> = Vec::with_capacity(points.len());
    for p in points {
        let dx = [p[0] - centroid[0], p[1] - centroid[1], p[2] - centroid[2]];
        let x = dx[0] * u[0] + dx[1] * u[1] + dx[2] * u[2];
        let y = dx[0] * v[0] + dx[1] * v[1] + dx[2] * v[2];
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        with_angle.push((y.atan2(x), p));
    }
    with_angle.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Some(with_angle.into_iter().map(|(_, p)| p).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cube3DObj, Quadric3DObj, Sphere3DObj};
    use grafito_geometry::Point3D;

    #[test]
    fn sphere_unit_volume_and_area() {
        let volume = sphere_volume(1.0).expect("esfera fixture");
        let area = sphere_area(1.0).expect("esfera fixture");
        assert!((volume - 4.188_790_204_786_390_5).abs() < 1e-9);
        assert!((area - 12.566_370_614_359_172).abs() < 1e-9);
    }

    #[test]
    fn cube_cylinder_cone_torus() {
        assert_eq!(cube_volume(2.0).expect("cubo"), 8.0);
        assert_eq!(cube_area(2.0).expect("cubo"), 24.0);
        let cylinder = cylinder_volume(1.0, 2.0).expect("cilindro");
        assert!((cylinder - 2.0 * std::f64::consts::PI).abs() < 1e-9);
        let cone = cone_volume(1.0, 3.0).expect("cono");
        assert!((cone - std::f64::consts::PI).abs() < 1e-9);
        let torus = torus_volume(3.0, 1.0).expect("toro");
        assert!((torus - 6.0 * std::f64::consts::PI.powi(2)).abs() < 1e-9);
        let torus_area_value = torus_area(3.0, 1.0).expect("toro");
        assert!((torus_area_value - 12.0 * std::f64::consts::PI.powi(2)).abs() < 1e-9);
    }

    #[test]
    fn non_positive_params_are_rejected() {
        assert!(matches!(
            sphere_volume(0.0),
            Err(SolidError::NonPositive { .. })
        ));
        assert!(matches!(
            cube_volume(f64::NAN),
            Err(SolidError::NonPositive { .. })
        ));
        assert!(matches!(
            cylinder_volume(1.0, f64::INFINITY),
            Err(SolidError::NonPositive { .. })
        ));
    }

    #[test]
    fn solid_volume_dispatches_by_object() {
        let sphere = GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0));
        let volume = solid_volume(&sphere).expect("esfera objeto");
        assert!((volume - 4.188_790_204_786_390_5).abs() < 1e-9);
        let cube = GeoObject::Cube3D(Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 2.0));
        assert_eq!(solid_volume(&cube), Some(8.0));
        assert_eq!(solid_measure_status(&cube), "exacto");
    }

    #[test]
    fn quadric_has_honest_status() {
        // Esfera como cuádrica: clasificación real con volumen analítico.
        let quadric = GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
            1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]));
        assert_eq!(solid_volume(&quadric), None);
        assert!(solid_measure_status(&quadric).contains("elipsoide real"));
        // Hiperboloide: superficie real pero sin volumen cerrado.
        let hiperboloide = GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
            1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]));
        assert_eq!(solid_volume(&hiperboloide), None);
        assert!(solid_measure_status(&hiperboloide).contains("no soportado"));
    }

    #[test]
    fn ortho_views_drop_one_axis() {
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Front), (1.0, 2.0));
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Top), (1.0, 3.0));
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Side), (2.0, 3.0));
        assert_eq!(OrthoView::Top.name(), "planta");
    }

    #[test]
    fn plano_z_corta_cubo_en_cuadrado() {
        // R3.2: cubo centro origen arista 2 + plano z=0 → cuadrado (±1,±1,0).
        let section =
            plane_cube_section((0.0, 0.0, 1.0, 0.0), [0.0, 0.0, 0.0], 2.0).expect("sección");
        assert_eq!(section.len(), 4, "cuadrado esperado: {section:?}");
        for p in &section {
            assert!(p[2].abs() < 1e-9, "en el plano z=0: {p:?}");
            assert!((p[0].abs() - 1.0).abs() < 1e-9, "x=±1: {p:?}");
            assert!((p[1].abs() - 1.0).abs() < 1e-9, "y=±1: {p:?}");
        }
        assert_eq!(
            best_ortho_view_for_normal([0.0, 0.0, 1.0]),
            OrthoView::Front
        );
        assert_eq!(best_ortho_view_for_normal([1.0, 0.0, 0.0]), OrthoView::Side);
        assert_eq!(best_ortho_view_for_normal([0.0, 1.0, 0.0]), OrthoView::Top);
    }

    #[test]
    fn plano_lejos_no_corta_y_degenerado_falla() {
        assert!(plane_cube_section((0.0, 0.0, 1.0, -5.0), [0.0, 0.0, 0.0], 2.0).is_none());
        assert!(plane_cube_section((0.0, 0.0, 0.0, 0.0), [0.0, 0.0, 0.0], 2.0).is_none());
        assert!(cube_vertices([0.0, 0.0, 0.0], 0.0).is_none());
        assert_eq!(cube_edges().len(), 12);
    }
}
