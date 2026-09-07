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
pub fn solid_measure_status(object: &GeoObject) -> &'static str {
    if solid_volume(object).is_some() && solid_area(object).is_some() {
        "exacto"
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
        let quadric = GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
            1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0,
        ]));
        assert_eq!(solid_volume(&quadric), None);
        assert!(solid_measure_status(&quadric).contains("no soportado"));
    }

    #[test]
    fn ortho_views_drop_one_axis() {
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Front), (1.0, 2.0));
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Top), (1.0, 3.0));
        assert_eq!(project_ortho([1.0, 2.0, 3.0], OrthoView::Side), (2.0, 3.0));
        assert_eq!(OrthoView::Top.name(), "planta");
    }
}
