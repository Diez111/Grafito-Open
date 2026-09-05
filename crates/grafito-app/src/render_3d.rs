use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use glam::{DVec3, Vec3};
use grafito_core::{
    ChangeSet, Cone3DObj, Cube3DObj, Cylinder3DObj, Document, GeoObject, Line3DObj,
    MoebiusStripObj, ObjectId, ParametricCurve3DObj, Plane3DObj, Point3DObj, Pyramid3DObj,
    RegularPolychoron4DObj, RegularPolytopeNDObj, Segment3DObj, Sphere3DObj, Surface3DObj,
    Torus3DObj, VectorField3DObj,
};
use grafito_geometry::{
    curve_3d_segment_is_continuous, Aabb3D, Camera3D, Point3D, Ray3D, RegularPolychoron,
    RegularPolytopeFamily,
};
use grafito_render::depth_3d::{
    project_regular_polychoron, project_regular_polytope_nd, ProjectedRegularPolytope,
};
use grafito_ui::Tool;

use crate::{to_color32, GrafitoApp};

/// La superficie 4D histórica no entra en el `WorldMesh` y conserva su
/// proyección CPU cuando el callback GPU está activo.
pub(crate) fn requires_cpu_3d_overlay(object: &GeoObject) -> bool {
    matches!(object, GeoObject::HyperSurface4D(_))
}

pub(crate) fn should_draw_cpu_3d_geometry(object: &GeoObject, overlay_only: bool) -> bool {
    !overlay_only || requires_cpu_3d_overlay(object)
}

/// Indica si un objeto tipado debe adquirir una proyección CPU en este frame.
///
/// Cuando el callback GPU ya compone su geometría, una proyección solo se necesita
/// para posicionar una etiqueta CPU; los objetos sin etiqueta no tocan la cache.
pub(crate) fn typed_cpu_projection_is_needed(object: &GeoObject, overlay_only: bool) -> bool {
    match object {
        GeoObject::RegularPolychoron4D(polychoron) => !overlay_only || !polychoron.label.is_empty(),
        GeoObject::RegularPolytopeND(polytope) => !overlay_only || !polytope.label.is_empty(),
        _ => false,
    }
}

/// Limita una fase transitoria a las proyecciones tipadas que realmente viven en R4.
/// La aplicación conserva la fase al pausar para mantener la proyección visible inmóvil.
pub(crate) fn typed_four_d_phase_for_object(
    object: &GeoObject,
    transient_phase: Option<f64>,
) -> Option<f64> {
    let phase = transient_phase.filter(|phase| phase.is_finite())?;
    match object {
        GeoObject::RegularPolychoron4D(_) => Some(phase),
        GeoObject::RegularPolytopeND(polytope) if polytope.dimension == 4 => Some(phase),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Cpu3dRenderOptions {
    pub(crate) overlay_only: bool,
    pub(crate) motion_preview: bool,
    pub(crate) typed_four_d_phase: Option<f64>,
}

fn camera_view_depth(camera: &Camera3D, point: Vec3) -> f32 {
    -(camera.view_matrix() * point.extend(1.0)).z
}

const PICK_RADIUS_PIXELS: f64 = 8.0;
const DEFAULT_CREATION_RADIUS_PIXELS: f64 = 40.0;
const PLANE_RENDER_EXTENT: f64 = 8.0;
const LINE_RENDER_HALF_EXTENT: f64 = 40.0;
const FALLBACK_CURVE_SAMPLES: usize = 500;
const FALLBACK_ATTRACTOR_STEPS: usize = 4_096;
const MOTION_PREVIEW_MAX_3D_SAMPLES: usize = 1_024;
pub(crate) const MAX_MOTION_PREVIEW_POLYTOPE_EDGES: usize = 1_024;

fn motion_preview_sample_stride(sample_count: usize, motion_preview: bool) -> usize {
    if !motion_preview || sample_count <= MOTION_PREVIEW_MAX_3D_SAMPLES {
        return 1;
    }
    sample_count.div_ceil(MOTION_PREVIEW_MAX_3D_SAMPLES)
}

fn motion_preview_surface_resolution(resolution: usize, motion_preview: bool) -> usize {
    if motion_preview {
        resolution.min(16)
    } else {
        resolution
    }
}

pub(crate) fn motion_preview_polytope_edge_stride(
    edge_count: usize,
    motion_preview: bool,
) -> usize {
    if !motion_preview || edge_count <= MAX_MOTION_PREVIEW_POLYTOPE_EDGES {
        return 1;
    }
    edge_count.div_ceil(MAX_MOTION_PREVIEW_POLYTOPE_EDGES)
}

fn effective_four_d_angles(base_angles: &[f64], phase: f64) -> [f64; 3] {
    let base = [
        base_angles.first().copied().unwrap_or(0.3),
        base_angles.get(1).copied().unwrap_or(0.5),
        base_angles.get(2).copied().unwrap_or(0.7),
    ];
    if !phase.is_finite() {
        return base;
    }
    [
        base[0] + phase,
        base[1] + phase * 2.0,
        base[2] + phase * 3.0,
    ]
}

/// Compone una fase transitoria únicamente sobre las seis rotaciones tipadas
/// de R4. Los multiplicadores enteros conservan continuidad al envolver en TAU.
pub(crate) fn effective_typed_four_d_angles(base_angles: [f64; 6], phase: Option<f64>) -> [f64; 6] {
    let Some(phase) = phase.filter(|phase| phase.is_finite()) else {
        return base_angles;
    };

    let mut effective = base_angles;
    for (index, angle) in effective.iter_mut().enumerate() {
        let offset = phase * (index + 1) as f64;
        let next = *angle + offset;
        if !next.is_finite() {
            return base_angles;
        }
        *angle = next;
    }
    effective
}

fn color_is_renderable(color: grafito_geometry::Color) -> bool {
    color
        .to_array()
        .iter()
        .all(|component| component.is_finite())
}

fn regular_polychoron_is_renderable(object: &RegularPolychoron4DObj) -> bool {
    object.scale.is_finite()
        && object.scale > 0.0
        && object.rotation_angles.iter().all(|angle| angle.is_finite())
        && object.width.is_finite()
        && object.width > 0.0
        && color_is_renderable(object.color)
        && object.fill_color.is_none_or(color_is_renderable)
}

fn regular_polytope_is_renderable(object: &RegularPolytopeNDObj) -> bool {
    let Some(expected_rotation_count) =
        RegularPolytopeNDObj::expected_rotation_angle_count(object.dimension)
    else {
        return false;
    };

    object.scale.is_finite()
        && object.scale > 0.0
        && object.rotation_angles.len() == expected_rotation_count
        && object.rotation_angles.iter().all(|angle| angle.is_finite())
        && object.width.is_finite()
        && object.width > 0.0
        && color_is_renderable(object.color)
        && object.fill_color.is_none_or(color_is_renderable)
}

pub(crate) fn project_regular_polychoron_cpu(
    object: &RegularPolychoron4DObj,
    transient_phase: Option<f64>,
) -> Option<ProjectedRegularPolytope> {
    if !regular_polychoron_is_renderable(object) {
        return None;
    }
    let angles = effective_typed_four_d_angles(object.rotation_angles, transient_phase);
    project_regular_polychoron(object, angles)
}

pub(crate) fn project_regular_polytope_nd_cpu(
    object: &RegularPolytopeNDObj,
    transient_phase: Option<f64>,
) -> Option<ProjectedRegularPolytope> {
    if !regular_polytope_is_renderable(object) {
        return None;
    }
    if object.dimension == 4 {
        let base_angles: [f64; 6] = object.rotation_angles.as_slice().try_into().ok()?;
        let angles = effective_typed_four_d_angles(base_angles, transient_phase);
        project_regular_polytope_nd(object, &angles)
    } else {
        project_regular_polytope_nd(object, &object.rotation_angles)
    }
}

pub(crate) fn should_draw_polychoron_faces(has_fill: bool, motion_preview: bool) -> bool {
    has_fill && !motion_preview
}

pub(crate) fn projected_polychoron_faces(
    camera: &Camera3D,
    geometry: &ProjectedRegularPolytope,
    screen_w: f32,
    screen_h: f32,
) -> Vec<(f32, [(f32, f32); 3])> {
    let mut triangles = Vec::new();
    for face in geometry.faces() {
        let Some(&first) = face.first() else {
            continue;
        };
        for index in 1..face.len().saturating_sub(1) {
            let Some((&second, &third)) = face.get(index).zip(face.get(index + 1)) else {
                continue;
            };
            let Some((a, b, c)) = geometry
                .vertices()
                .get(first)
                .zip(geometry.vertices().get(second))
                .zip(geometry.vertices().get(third))
                .map(|((a, b), c)| (*a, *b, *c))
            else {
                continue;
            };
            let (Some(a2), Some(b2), Some(c2)) = (
                camera.project(&a, screen_w, screen_h),
                camera.project(&b, screen_w, screen_h),
                camera.project(&c, screen_w, screen_h),
            ) else {
                continue;
            };
            let depth = (camera_view_depth(camera, a.to_vec3())
                + camera_view_depth(camera, b.to_vec3())
                + camera_view_depth(camera, c.to_vec3()))
                / 3.0;
            if depth.is_finite() {
                triangles.push((depth, [a2, b2, c2]));
            }
        }
    }
    triangles.sort_by(|left, right| right.0.total_cmp(&left.0));
    triangles
}

fn projected_polytope_center(geometry: &ProjectedRegularPolytope) -> Option<Vec3> {
    let count = geometry.vertices().len() as f64;
    if count <= 0.0 || !count.is_finite() {
        return None;
    }
    let sum = geometry
        .vertices()
        .iter()
        .fold(DVec3::ZERO, |sum, point| sum + point.to_dvec3());
    if !sum.is_finite() {
        return None;
    }
    let center = Point3D::from_dvec3(sum / count);
    grafito_render::depth_3d::point_is_renderable(center).then_some(center.to_vec3())
}

/// Intersects a canvas-local pointer ray with the camera-facing plane through
/// `camera.target`. The plane basis remains available from `Camera3D` for
/// future axis-constrained dragging.
pub(crate) fn construction_point_from_canvas(
    camera: &Camera3D,
    local_pointer: Vec2,
    canvas_size: Vec2,
) -> Option<Point3D> {
    if !local_pointer.is_finite() || !canvas_size.is_finite() {
        return None;
    }
    let ray = camera.screen_ray(
        local_pointer.x,
        local_pointer.y,
        canvas_size.x,
        canvas_size.y,
    )?;
    camera.construction_plane()?.intersect_ray(&ray)
}

fn world_pick_radius(
    camera: &Camera3D,
    ray: &Ray3D,
    distance: f64,
    canvas_height: f32,
    pixel_radius: f64,
) -> Option<f64> {
    if !canvas_height.is_finite()
        || canvas_height <= 0.0
        || !pixel_radius.is_finite()
        || pixel_radius <= 0.0
    {
        return None;
    }
    let point = ray.point_at(distance)?.to_vec3();
    if !point.is_finite() {
        return None;
    }
    let depth = camera_view_depth(camera, point) as f64;
    let half_fov = (camera.fov as f64).to_radians() * 0.5;
    let radius = 2.0 * depth * half_fov.tan() * pixel_radius / canvas_height as f64;
    (depth >= camera.near as f64
        && depth <= camera.far as f64
        && radius.is_finite()
        && radius > 0.0)
        .then_some(radius)
}

fn segment_overlaps_visible_depth(camera: &Camera3D, a: Point3D, b: Point3D) -> bool {
    let a = a.to_vec3();
    let b = b.to_vec3();
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let a_depth = camera_view_depth(camera, a);
    let b_depth = camera_view_depth(camera, b);
    a_depth.is_finite()
        && b_depth.is_finite()
        && a_depth.max(b_depth) >= camera.near
        && a_depth.min(b_depth) <= camera.far
}

fn proximity_hit(
    camera: &Camera3D,
    ray: &Ray3D,
    a: Point3D,
    b: Point3D,
    canvas_height: f32,
    pixel_radius: f64,
) -> Option<f64> {
    if !segment_overlaps_visible_depth(camera, a, b) {
        return None;
    }
    let proximity = ray.closest_to_segment(a, b)?;
    let tolerance = world_pick_radius(
        camera,
        ray,
        proximity.distance_along_ray,
        canvas_height,
        pixel_radius,
    )?;
    (proximity.separation <= tolerance).then_some(proximity.distance_along_ray)
}

fn center_extent_bounds(center: Point3D, extent: f64) -> Option<Aabb3D> {
    if !center.is_finite() || !extent.is_finite() || extent < 0.0 {
        return None;
    }
    let center = center.to_dvec3();
    let extent = DVec3::splat(extent);
    Aabb3D::new(
        Point3D::from_dvec3(center - extent),
        Point3D::from_dvec3(center + extent),
    )
}

fn endpoints_radius_bounds(a: Point3D, b: Point3D, radius: f64) -> Option<Aabb3D> {
    if !a.is_finite() || !b.is_finite() || !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let min = a.to_dvec3().min(b.to_dvec3()) - DVec3::splat(radius);
    let max = a.to_dvec3().max(b.to_dvec3()) + DVec3::splat(radius);
    Aabb3D::new(Point3D::from_dvec3(min), Point3D::from_dvec3(max))
}

fn surface_bounds(
    surface: &Surface3DObj,
    variables: &std::collections::HashMap<String, f64>,
) -> Option<Aabb3D> {
    if let Ok(grid) = surface.cached_grid.try_read() {
        if let Some(bounds) = Aabb3D::from_points(grid.iter().flatten().copied()) {
            return Some(bounds);
        }
    }
    Aabb3D::from_points(
        grafito_core::parametric_sampling::evaluate_surface_3d(
            surface,
            surface.mesh_res.clamp(8, 50),
            variables,
        )
        .into_iter()
        .flatten(),
    )
}

fn curve_bounds(
    curve: &ParametricCurve3DObj,
    variables: &std::collections::HashMap<String, f64>,
) -> Option<Aabb3D> {
    if let Ok(samples) = curve.cached_samples.try_read() {
        if let Some(bounds) =
            Aabb3D::from_points(samples.iter().map(|&(x, y, z)| Point3D::new(x, y, z)))
        {
            return Some(bounds);
        }
    }
    Aabb3D::from_points(
        grafito_core::parametric_sampling::evaluate_parametric_curve_3d(
            curve,
            FALLBACK_CURVE_SAMPLES,
            variables,
        )
        .into_iter()
        .map(|(x, y, z)| Point3D::new(x, y, z)),
    )
}

fn fallback_bounds_hit(
    bounds: Aabb3D,
    camera: &Camera3D,
    ray: &Ray3D,
    canvas_height: f32,
) -> Option<f64> {
    let mut min_depth = f32::INFINITY;
    let mut max_depth = f32::NEG_INFINITY;
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let point = Point3D::new(x, y, z).to_vec3();
                if !point.is_finite() {
                    return None;
                }
                let depth = camera_view_depth(camera, point);
                if !depth.is_finite() {
                    return None;
                }
                min_depth = min_depth.min(depth);
                max_depth = max_depth.max(depth);
            }
        }
    }
    if max_depth < camera.near || min_depth > camera.far {
        return None;
    }

    let min = bounds.min.to_dvec3();
    let max = bounds.max.to_dvec3();
    let center = min + (max - min) * 0.5;
    if !center.is_finite() {
        return None;
    }
    let center_distance = (center - ray.origin.to_dvec3())
        .dot(ray.direction.to_dvec3())
        .clamp(ray.min_distance, ray.max_distance);
    let padding = world_pick_radius(
        camera,
        ray,
        center_distance,
        canvas_height,
        PICK_RADIUS_PIXELS,
    )?;
    let padding = DVec3::splat(padding.max(1.0e-9));
    let padded = Aabb3D::new(
        Point3D::from_dvec3(min - padding),
        Point3D::from_dvec3(max + padding),
    )?;
    ray.intersect_aabb(padded)
}

fn fallback_object_bounds(
    object: &GeoObject,
    variables: &std::collections::HashMap<String, f64>,
) -> Option<Aabb3D> {
    fallback_object_bounds_with_typed_four_d_phase(object, variables, None)
}

/// Obtiene los límites CPU de selección con la misma fase tipada que la proyección dibujada.
pub(crate) fn fallback_object_bounds_with_typed_four_d_phase(
    object: &GeoObject,
    variables: &std::collections::HashMap<String, f64>,
    typed_four_d_phase: Option<f64>,
) -> Option<Aabb3D> {
    match object {
        GeoObject::Pyramid3D(pyramid) => {
            let geometry = grafito_geometry::Pyramid3D::new(
                pyramid.base_center,
                pyramid.apex,
                pyramid.base_size,
            );
            Aabb3D::from_points(
                geometry
                    .base_vertices()
                    .into_iter()
                    .chain(std::iter::once(pyramid.apex)),
            )
        }
        GeoObject::Tetrahedron3D(tetrahedron) => Aabb3D::from_points(
            grafito_geometry::Tetrahedron3D::new(tetrahedron.center, tetrahedron.edge_length)
                .vertices(),
        ),
        GeoObject::Cone3D(cone) => {
            endpoints_radius_bounds(cone.base_center, cone.apex, cone.radius)
        }
        GeoObject::Cylinder3D(cylinder) => {
            endpoints_radius_bounds(cylinder.base_center, cylinder.top_center, cylinder.radius)
        }
        GeoObject::Torus3D(torus) => {
            center_extent_bounds(torus.center, torus.r_major.abs() + torus.r_minor.abs())
        }
        GeoObject::MoebiusStrip(strip) => {
            center_extent_bounds(strip.center, strip.radius.abs() + strip.width_r.abs() * 0.5)
        }
        GeoObject::Surface3D(surface) => surface_bounds(surface, variables),
        GeoObject::ParametricCurve3D(curve) => curve_bounds(curve, variables),
        GeoObject::Attractor3D(attractor) => {
            let steps = attractor.steps.min(FALLBACK_ATTRACTOR_STEPS);
            let skip = attractor.skip.min(steps.saturating_sub(1));
            let points = grafito_geometry::attractors::integrate_attractor(
                &attractor.model(),
                attractor.x0,
                attractor.y0,
                attractor.z0,
                attractor.dt,
                steps,
                skip,
            )
            .into_iter()
            .map(|point| Point3D::new(point.x * 0.2, point.y * 0.2, point.z * 0.2));
            Aabb3D::from_points(points)
        }
        GeoObject::RegularPolychoron4D(polychoron) => project_regular_polychoron_cpu(
            polychoron,
            typed_four_d_phase_for_object(object, typed_four_d_phase),
        )
        .and_then(|geometry| Aabb3D::from_points(geometry.vertices().iter().copied())),
        GeoObject::RegularPolytopeND(polytope) => project_regular_polytope_nd_cpu(
            polytope,
            typed_four_d_phase_for_object(object, typed_four_d_phase),
        )
        .and_then(|geometry| Aabb3D::from_points(geometry.vertices().iter().copied())),
        GeoObject::HyperSurface4D(surface) => {
            let extent = surface.params.first().copied().unwrap_or(3.0).abs() * 2.0;
            center_extent_bounds(Point3D::new(0.0, 0.0, 0.0), extent)
        }
        GeoObject::VectorField3D(field) => Aabb3D::new(
            Point3D::new(field.x_min, field.y_min, field.z_min),
            Point3D::new(field.x_max, field.y_max, field.z_max),
        ),
        GeoObject::Prism3D(prism) => {
            let base = grafito_render::prism_base_vertices(prism);
            let top = grafito_render::prism_top_vertices(prism);
            Aabb3D::from_points(base.iter().copied().chain(top))
        }
        GeoObject::Quadric3D(quadric) => {
            // Elipsoide derivado de la cuádrica (paso intermedio honesto).
            let ellipsoid = grafito_render::quadric_ellipsoid_params(quadric)
                .unwrap_or_else(grafito_render::QuadricEllipsoid::placeholder);
            let center = ellipsoid.center.to_dvec3();
            let radii = DVec3::new(
                ellipsoid.radii.x as f64,
                ellipsoid.radii.y as f64,
                ellipsoid.radii.z as f64,
            );
            Aabb3D::new(
                Point3D::from_dvec3(center - radii),
                Point3D::from_dvec3(center + radii),
            )
        }
        _ => None,
    }
}

fn object_ray_hit(
    object: &GeoObject,
    variables: &std::collections::HashMap<String, f64>,
    camera: &Camera3D,
    ray: &Ray3D,
    canvas_height: f32,
    typed_four_d_phase: Option<f64>,
) -> Option<PickHit> {
    match object {
        GeoObject::Point3D(point) => proximity_hit(
            camera,
            ray,
            point.position,
            point.position,
            canvas_height,
            PICK_RADIUS_PIXELS.max(point.size.min(5.0) as f64),
        )
        .map(PickHit::exact),
        GeoObject::Segment3D(segment) => proximity_hit(
            camera,
            ray,
            segment.a,
            segment.b,
            canvas_height,
            PICK_RADIUS_PIXELS.max(segment.width as f64 * 0.5 + 4.0),
        )
        .map(PickHit::exact),
        GeoObject::Line3D(line) => {
            let direction = line.direction.to_dvec3();
            let length = direction.length();
            if !length.is_finite() || length <= 1.0e-12 || !line.point.is_finite() {
                return None;
            }
            let extent = direction / length * LINE_RENDER_HALF_EXTENT;
            proximity_hit(
                camera,
                ray,
                Point3D::from_dvec3(line.point.to_dvec3() - extent),
                Point3D::from_dvec3(line.point.to_dvec3() + extent),
                canvas_height,
                PICK_RADIUS_PIXELS.max(line.width as f64 * 0.5 + 4.0),
            )
            .map(PickHit::exact)
        }
        GeoObject::Sphere3D(sphere) => ray
            .intersect_sphere(sphere.center, sphere.radius)
            .map(PickHit::exact),
        GeoObject::Cube3D(cube) => center_extent_bounds(cube.center, cube.size * 0.5)
            .and_then(|bounds| ray.intersect_aabb(bounds))
            .map(PickHit::exact),
        GeoObject::Plane3D(plane) => {
            let (center, axis_u, axis_v) =
                plane_point_and_basis(plane.a, plane.b, plane.c, plane.d)?;
            let normal = Point3D::from_vec3(axis_u.cross(axis_v).normalize_or_zero());
            let (distance, hit) = ray.intersect_plane(center, normal)?;
            let offset = hit.to_dvec3() - center.to_dvec3();
            let u = offset.dot(axis_u.as_dvec3());
            let v = offset.dot(axis_v.as_dvec3());
            (u.is_finite()
                && v.is_finite()
                && u.abs() <= PLANE_RENDER_EXTENT
                && v.abs() <= PLANE_RENDER_EXTENT)
                .then_some(distance)
                .map(PickHit::exact)
        }
        _ => {
            let bounds = match typed_four_d_phase {
                Some(phase) => {
                    fallback_object_bounds_with_typed_four_d_phase(object, variables, Some(phase))
                }
                None => fallback_object_bounds(object, variables),
            };
            bounds
                .and_then(|bounds| fallback_bounds_hit(bounds, camera, ray, canvas_height))
                .map(PickHit::coarse)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PickConfidence {
    CoarseBounds,
    ExactGeometry,
}

#[derive(Debug, Clone, Copy)]
struct PickHit {
    distance: f64,
    confidence: PickConfidence,
}

impl PickHit {
    fn exact(distance: f64) -> Self {
        Self {
            distance,
            confidence: PickConfidence::ExactGeometry,
        }
    }

    fn coarse(distance: f64) -> Self {
        Self {
            distance,
            confidence: PickConfidence::CoarseBounds,
        }
    }
}

/// Picks the highest-confidence visible 3D hit under a canvas-local pointer.
/// Exact hits outrank coarse bounds; distance and stable `ObjectId` break ties.
pub(crate) fn pick_3d_object(
    document: &Document,
    camera: &Camera3D,
    local_pointer: Vec2,
    canvas_size: Vec2,
) -> Option<ObjectId> {
    pick_3d_object_with_typed_four_d_phase(document, camera, local_pointer, canvas_size, None)
}

/// Picks using the phase snapshot captured for the current 3D frame.
pub(crate) fn pick_3d_object_with_typed_four_d_phase(
    document: &Document,
    camera: &Camera3D,
    local_pointer: Vec2,
    canvas_size: Vec2,
    typed_four_d_phase: Option<f64>,
) -> Option<ObjectId> {
    if !local_pointer.is_finite() || !canvas_size.is_finite() {
        return None;
    }
    let ray = camera.screen_ray(
        local_pointer.x,
        local_pointer.y,
        canvas_size.x,
        canvas_size.y,
    )?;
    let mut best: Option<(PickHit, ObjectId)> = None;
    for (id, object) in document.objects_iter() {
        if !object.is_visible() || !object.is_3d() {
            continue;
        }
        let Some(hit) = object_ray_hit(
            object,
            &document.variables,
            camera,
            &ray,
            canvas_size.y,
            typed_four_d_phase,
        ) else {
            continue;
        };
        if !hit.distance.is_finite() {
            continue;
        }
        let candidate = (hit, *id);
        if best
            .as_ref()
            .map(|current| {
                candidate.0.confidence > current.0.confidence
                    || (candidate.0.confidence == current.0.confidence
                        && (candidate.0.distance.total_cmp(&current.0.distance).is_lt()
                            || (candidate.0.distance.total_cmp(&current.0.distance).is_eq()
                                && candidate.1 < current.1)))
            })
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }
    best.map(|(_, id)| id)
}

/// Applies the same single-selection and empty-click clearing policy as 2D.
pub(crate) fn select_3d_object_at_pointer(
    document: &mut Document,
    selected_object: &mut Option<ObjectId>,
    camera: &Camera3D,
    local_pointer: Vec2,
    canvas_size: Vec2,
) -> Option<ObjectId> {
    select_3d_object_at_pointer_with_typed_four_d_phase(
        document,
        selected_object,
        camera,
        local_pointer,
        canvas_size,
        None,
    )
}

/// Applies 3D selection using the per-frame typed-4D phase snapshot.
pub(crate) fn select_3d_object_at_pointer_with_typed_four_d_phase(
    document: &mut Document,
    selected_object: &mut Option<ObjectId>,
    camera: &Camera3D,
    local_pointer: Vec2,
    canvas_size: Vec2,
    typed_four_d_phase: Option<f64>,
) -> Option<ObjectId> {
    let picked = match typed_four_d_phase {
        Some(phase) => pick_3d_object_with_typed_four_d_phase(
            document,
            camera,
            local_pointer,
            canvas_size,
            Some(phase),
        ),
        None => pick_3d_object(document, camera, local_pointer, canvas_size),
    };
    document.clear_selection();
    if let Some(id) = picked {
        document.select(id);
    }
    *selected_object = picked;
    picked
}

fn project_segment(
    camera: &Camera3D,
    a: &Point3D,
    b: &Point3D,
    screen_w: f32,
    screen_h: f32,
) -> Option<((f32, f32), (f32, f32))> {
    if !a.x.is_finite()
        || !a.y.is_finite()
        || !a.z.is_finite()
        || !b.x.is_finite()
        || !b.y.is_finite()
        || !b.z.is_finite()
        || !screen_w.is_finite()
        || !screen_h.is_finite()
        || screen_w <= 0.0
        || screen_h <= 0.0
        || !camera.near.is_finite()
        || camera.near <= 0.0
    {
        return None;
    }
    let mvp = camera.mvp();
    let a = a.to_vec3();
    let b = b.to_vec3();
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let mut clip_a = mvp * a.extend(1.0);
    let mut clip_b = mvp * b.extend(1.0);
    if !clip_a.is_finite() || !clip_b.is_finite() {
        return None;
    }

    let near = camera.near;

    if clip_a.w < near && clip_b.w < near {
        return None;
    }

    if clip_a.w < near {
        let t = (near - clip_a.w) / (clip_b.w - clip_a.w);
        if !t.is_finite() {
            return None;
        }
        clip_a = clip_a + t * (clip_b - clip_a);
    } else if clip_b.w < near {
        let t = (near - clip_b.w) / (clip_a.w - clip_b.w);
        if !t.is_finite() {
            return None;
        }
        clip_b = clip_b + t * (clip_a - clip_b);
    }
    if !clip_a.is_finite() || !clip_b.is_finite() {
        return None;
    }

    let ndc_ax = clip_a.x / clip_a.w;
    let ndc_ay = clip_a.y / clip_a.w;
    let ndc_bx = clip_b.x / clip_b.w;
    let ndc_by = clip_b.y / clip_b.w;
    if !ndc_ax.is_finite() || !ndc_ay.is_finite() || !ndc_bx.is_finite() || !ndc_by.is_finite() {
        return None;
    }

    if ndc_ax.abs() > 5.0 && ndc_bx.abs() > 5.0 && ndc_ax.signum() == ndc_bx.signum() {
        return None;
    }
    if ndc_ay.abs() > 5.0 && ndc_by.abs() > 5.0 && ndc_ay.signum() == ndc_by.signum() {
        return None;
    }

    let sax = (ndc_ax + 1.0) * 0.5 * screen_w;
    let say = (1.0 - ndc_ay) * 0.5 * screen_h;
    let sbx = (ndc_bx + 1.0) * 0.5 * screen_w;
    let sby = (1.0 - ndc_by) * 0.5 * screen_h;

    (sax.is_finite() && say.is_finite() && sbx.is_finite() && sby.is_finite())
        .then_some(((sax, say), (sbx, sby)))
}

pub(crate) fn projected_tetrahedron_faces(
    camera: &Camera3D,
    tetrahedron: &grafito_geometry::Tetrahedron3D,
    screen_w: f32,
    screen_h: f32,
) -> Vec<(f32, [(f32, f32); 3])> {
    let vertices = tetrahedron.vertices();
    let mut faces = tetrahedron
        .faces()
        .into_iter()
        .filter_map(|[a, b, c]| {
            let (a2, b2, c2) = (
                camera.project(&vertices[a], screen_w, screen_h)?,
                camera.project(&vertices[b], screen_w, screen_h)?,
                camera.project(&vertices[c], screen_w, screen_h)?,
            );
            let depth = (camera_view_depth(camera, vertices[a].to_vec3())
                + camera_view_depth(camera, vertices[b].to_vec3())
                + camera_view_depth(camera, vertices[c].to_vec3()))
                / 3.0;
            depth.is_finite().then_some((depth, [a2, b2, c2]))
        })
        .collect::<Vec<_>>();
    faces.sort_by(|left, right| right.0.total_cmp(&left.0));
    faces
}

/// Normal de una cara triangular (producto cruz), o `None` si es degenerada.
fn face_normal(a: Point3D, b: Point3D, c: Point3D) -> Option<Vec3> {
    let normal = (b.to_vec3() - a.to_vec3())
        .cross(c.to_vec3() - a.to_vec3())
        .normalize_or_zero();
    (normal.is_finite() && normal.length_squared() > 1.0e-12).then_some(normal)
}

/// Proyecta una cara 3D a puntos de pantalla; `None` si algún vértice queda
/// fuera del frustum (la cara no debe dibujarse parcialmente).
fn projected_face_points(
    camera: &Camera3D,
    points: &[Point3D],
    screen_w: f32,
    screen_h: f32,
    origin: Pos2,
) -> Option<Vec<Pos2>> {
    let projected: Vec<Pos2> = points
        .iter()
        .filter_map(|point| {
            camera
                .project(point, screen_w, screen_h)
                .map(|(x, y)| origin + Vec2::new(x, y))
        })
        .collect();
    (projected.len() == points.len()).then_some(projected)
}

pub(crate) fn projected_point_position(
    camera: &Camera3D,
    point: Point3D,
    canvas_size: Vec2,
) -> Option<Vec2> {
    camera
        .project(&point, canvas_size.x, canvas_size.y)
        .map(|(x, y)| Vec2::new(x, y))
}

fn plane_point_and_basis(a: f64, b: f64, c: f64, d: f64) -> Option<(Point3D, Vec3, Vec3)> {
    if !a.is_finite() || !b.is_finite() || !c.is_finite() || !d.is_finite() {
        return None;
    }
    let scale = a.abs().max(b.abs()).max(c.abs());
    if !scale.is_finite() || scale <= 1.0e-15 {
        return None;
    }
    let normal = DVec3::new(a / scale, b / scale, c / scale).normalize_or_zero();
    if !normal.is_finite() || normal.length_squared() < 1.0e-24 {
        return None;
    }
    let point = if a.abs() >= b.abs() && a.abs() >= c.abs() {
        Point3D::new(-d / a, 0.0, 0.0)
    } else if b.abs() >= c.abs() {
        Point3D::new(0.0, -d / b, 0.0)
    } else {
        Point3D::new(0.0, 0.0, -d / c)
    };
    if !point.is_finite() {
        return None;
    }
    let n = normal.as_vec3();
    let seed = if n.x.abs() < 0.8 { Vec3::X } else { Vec3::Y };
    let u = n.cross(seed).normalize_or_zero();
    let v = n.cross(u).normalize_or_zero();
    (u.is_finite() && v.is_finite() && u.length_squared() > 1.0e-12 && v.length_squared() > 1.0e-12)
        .then_some((point, u, v))
}

fn offset_point(base: Point3D, u: Vec3, v: Vec3, du: f32, dv: f32) -> Point3D {
    Point3D::new(
        base.x + (u.x * du + v.x * dv) as f64,
        base.y + (u.y * du + v.y * dv) as f64,
        base.z + (u.z * du + v.z * dv) as f64,
    )
}

fn centered_four_d_tool_default(tool: Tool) -> Option<(GeoObject, &'static str)> {
    match tool {
        Tool::Tesseract4D => Some((
            GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
                RegularPolychoron::Tesseract,
            )),
            "Tesseract4D",
        )),
        Tool::Hypercube5D => Some((
            GeoObject::RegularPolytopeND(RegularPolytopeNDObj::new(
                RegularPolytopeFamily::Hypercube,
                5,
            )),
            "Hypercube5D",
        )),
        _ => None,
    }
}

/// Inserta el selector tipado por defecto para una herramienta 4D y lo deja
/// seleccionado. La construcción no consume el puntero: ambas proyecciones
/// nacen centradas en el origen canónico.
pub(crate) fn create_centered_four_d_tool_object(
    current_tool: &mut Tool,
    document: &mut Document,
    selected_object: &mut Option<ObjectId>,
    undo_stack: &mut std::collections::VecDeque<Document>,
    redo_stack: &mut std::collections::VecDeque<ChangeSet>,
) -> Result<Option<(ObjectId, &'static str)>, String> {
    let Some((object, action)) = centered_four_d_tool_default(*current_tool) else {
        return Ok(None);
    };
    let id = crate::app::commit_object_insertions(document, undo_stack, redo_stack, vec![object])?
        .into_iter()
        .next()
        .ok_or_else(|| "La inserción 4D no devolvió un identificador".to_string())?;

    document.clear_selection();
    document.select(id);
    *selected_object = Some(id);
    *current_tool = Tool::Select;
    Ok(Some((id, action)))
}

impl GrafitoApp {
    pub fn handle_3d_click(&mut self, ui: &egui::Ui, local_pointer: Vec2, canvas_size: Vec2) {
        if matches!(self.current_tool, Tool::Tesseract4D | Tool::Hypercube5D) {
            let tool_name = self.current_tool.name();
            let time = ui.ctx().input(|input| input.time);
            match create_centered_four_d_tool_object(
                &mut self.current_tool,
                &mut self.document,
                &mut self.selected_object,
                &mut self.undo_stack,
                &mut self.redo_stack,
            ) {
                Ok(Some((id, action))) => {
                    let output = self
                        .document
                        .get_object(id)
                        .map(|object| object.label().to_string())
                        .unwrap_or_default();
                    self.record_construction_step(action, Vec::new(), &output);
                    self.tool_ghost = None;
                    self.reset_tool_input();
                }
                Ok(None) => {}
                Err(error) => self.handle_command_outcome(
                    grafito_command::commands::CommandOutcome::Error(error),
                    time,
                    tool_name,
                ),
            }
            return;
        }

        let Some(c) = construction_point_from_canvas(&self.camera, local_pointer, canvas_size)
        else {
            return;
        };
        let h = canvas_size.y;
        let approx_scale = (2.0
            * self.camera.sanitized_distance() as f64
            * ((self.camera.fov as f64).to_radians() * 0.5)
                .tan()
                .abs()
                .max(1e-6)
            / (h as f64).max(1.0)
            * DEFAULT_CREATION_RADIUS_PIXELS)
            .clamp(1.0e-6, 1e6);
        let time = ui.ctx().input(|i| i.time);

        match self.current_tool {
            Tool::Point => {
                self.insert_object_from_tool(
                    GeoObject::Point3D(Point3DObj::new(c)),
                    "Point3D",
                    time,
                );
            }
            Tool::Line => {
                self.pending_points_3d.push(c);
                if self.pending_points_3d.len() == 2 {
                    let a = self.pending_points_3d[0];
                    let b = self.pending_points_3d[1];
                    self.insert_object_from_tool(
                        GeoObject::Segment3D(Segment3DObj::new(a, b)),
                        "Segment3D",
                        time,
                    );
                    self.pending_points_3d.clear();
                }
            }
            Tool::Circle => {
                self.pending_points_3d.push(c);
                if self.pending_points_3d.len() == 2 {
                    let center = self.pending_points_3d[0];
                    let edge = self.pending_points_3d[1];
                    let radius = center.distance(&edge);
                    self.insert_object_from_tool(
                        GeoObject::Sphere3D(Sphere3DObj::new(center, radius)),
                        "Sphere3D",
                        time,
                    );
                    self.pending_points_3d.clear();
                }
            }
            Tool::Polygon => {
                // Cube via polygon tool
                self.insert_object_from_tool(
                    GeoObject::Cube3D(Cube3DObj::new(c, approx_scale * 2.0)),
                    "Cube3D",
                    time,
                );
            }
            Tool::Function => {
                self.insert_object_from_tool(
                    GeoObject::Pyramid3D(Pyramid3DObj::new(
                        Point3D::new(c.x, c.y - approx_scale, c.z),
                        c,
                        approx_scale * 2.0,
                    )),
                    "Pyramid3D",
                    time,
                );
            }
            Tool::Point3D => {
                self.insert_object_from_tool(
                    GeoObject::Point3D(Point3DObj::new(c)),
                    "Point3D",
                    time,
                );
            }
            Tool::Segment3D => {
                let s = approx_scale * 2.0;
                self.insert_object_from_tool(
                    GeoObject::Segment3D(Segment3DObj::new(
                        Point3D::new(c.x - s, c.y, c.z),
                        Point3D::new(c.x + s, c.y, c.z),
                    )),
                    "Segment3D",
                    time,
                );
            }
            Tool::Line3D => {
                let direction = self
                    .camera
                    .construction_plane()
                    .map(|plane| plane.axis_u)
                    .unwrap_or(Point3D::new(1.0, 0.0, 0.0));
                self.insert_object_from_tool(
                    GeoObject::Line3D(Line3DObj::from_point_and_direction(c, direction)),
                    "Line3D",
                    time,
                );
            }
            Tool::Plane3D => {
                if let Some(plane) = self.camera.construction_plane() {
                    let normal = plane.normal;
                    let d = -(normal.x * c.x + normal.y * c.y + normal.z * c.z);
                    self.insert_object_from_tool(
                        GeoObject::Plane3D(Plane3DObj::from_equation(
                            normal.x, normal.y, normal.z, d,
                        )),
                        "Plane3D",
                        time,
                    );
                }
            }
            Tool::Sphere3D => {
                self.insert_object_from_tool(
                    GeoObject::Sphere3D(Sphere3DObj::new(c, approx_scale * 1.5)),
                    "Sphere3D",
                    time,
                );
            }
            Tool::Cube3D => {
                self.insert_object_from_tool(
                    GeoObject::Cube3D(Cube3DObj::new(c, approx_scale * 2.0)),
                    "Cube3D",
                    time,
                );
            }
            Tool::Cylinder3D => {
                self.insert_object_from_tool(
                    GeoObject::Cylinder3D(Cylinder3DObj::new(
                        c,
                        Point3D::new(c.x, c.y + approx_scale * 3.0, c.z),
                        approx_scale,
                    )),
                    "Cylinder3D",
                    time,
                );
            }
            Tool::Cone3D => {
                self.insert_object_from_tool(
                    GeoObject::Cone3D(Cone3DObj::new(
                        c,
                        Point3D::new(c.x, c.y + approx_scale * 3.0, c.z),
                        approx_scale,
                    )),
                    "Cone3D",
                    time,
                );
            }
            Tool::Torus3D => {
                self.insert_object_from_tool(
                    GeoObject::Torus3D(Torus3DObj::new(c, approx_scale * 1.5, approx_scale * 0.4)),
                    "Torus3D",
                    time,
                );
            }
            Tool::MoebiusStrip => {
                self.insert_object_from_tool(
                    GeoObject::MoebiusStrip(MoebiusStripObj::new(c, 2.0, 0.5)),
                    "MoebiusStrip",
                    time,
                );
            }
            Tool::Surface3D => {
                let expression = format!(
                    "(x - ({:.12}))^2 + (y - ({:.12}))^2 + ({:.12})",
                    c.x, c.y, c.z
                );
                self.insert_object_from_tool(
                    GeoObject::Surface3D(Surface3DObj::new(
                        expression,
                        (c.x - 2.0, c.x + 2.0),
                        (c.y - 2.0, c.y + 2.0),
                    )),
                    "Surface3D",
                    time,
                );
            }
            Tool::ParametricCurve3D => {
                self.insert_object_from_tool(
                    GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
                        &format!("({:.12}) + cos(t)", c.x),
                        &format!("({:.12}) + sin(t)", c.y),
                        &format!("({:.12}) + t/4", c.z),
                        0.0,
                        12.566,
                    )),
                    "ParametricCurve3D",
                    time,
                );
            }
            Tool::VectorField3D => {
                self.insert_object_from_tool(
                    GeoObject::VectorField3D(VectorField3DObj::new("-y", "x", "z/3").with_bounds(
                        (c.x - 3.0, c.x + 3.0),
                        (c.y - 3.0, c.y + 3.0),
                        (c.z - 3.0, c.z + 3.0),
                    )),
                    "VectorField3D",
                    time,
                );
            }
            Tool::HyperSurface4D => {
                self.execute_command_and_record("Hypercube[]", time);
            }
            Tool::Attractor => {
                self.execute_command_and_record("Lorenz[]", time);
            }
            Tool::Fractal => {
                self.input_text = "Mandelbrot[]".to_string();
            }
            _ => {}
        }
    }

    pub fn draw_3d_grid(
        &self,
        painter: &egui::Painter,
        canvas: Rect,
        w: f32,
        h: f32,
        overlay_only: bool,
        canvas_resize_preview: bool,
    ) {
        let origin = canvas.min;

        // Dynamic step calculation based on camera distance (sanitizado para no NaN/black)
        let dist = self.camera.sanitized_distance();
        let fov_rad = self.camera.fov.to_radians().clamp(0.01, 3.0);
        let frustum_height = 2.0 * dist * (fov_rad * 0.5).tan().abs().max(1e-6);
        let pixels_per_unit = (h / frustum_height) as f64;
        let target_world_step = 120.0 / pixels_per_unit.max(1e-30);
        let magnitude = target_world_step.log10().floor();
        let base = 10f64.powf(magnitude);
        let factor = target_world_step / base;

        let major_step = if factor < 2.0 {
            1.0 * base
        } else if factor < 5.0 {
            2.0 * base
        } else {
            5.0 * base
        };
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

        // Grid colors (soft grey, adapting to dark mode)
        let major_color = if self.dark_mode {
            Color32::from_rgba_unmultiplied(255, 255, 255, 28)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 35)
        };
        let major_stroke = Stroke::new(0.5, major_color);

        // Grid range: center around camera target projection on XZ plane
        let center_x = self.camera.target.x as f64;
        let center_z = self.camera.target.z as f64;
        let aspect = w / h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let start_x = ((center_x - view_range) / major_step).floor() * major_step;
        let end_x = ((center_x + view_range) / major_step).ceil() * major_step;
        let start_z = ((center_z - view_range) / major_step).floor() * major_step;
        let end_z = ((center_z + view_range) / major_step).ceil() * major_step;

        let line_count_x = ((end_x - start_x) / major_step).round() as i64;
        let line_count_z = ((end_z - start_z) / major_step).round() as i64;

        if self.show_grid && line_count_x <= 2000 && line_count_z <= 2000 {
            // Draw grid lines parallel to Z axis (varying z, fixed x)
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * major_step;
                let stroke = major_stroke;
                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                if let Some((a, b)) = project_segment(&self.camera, &p1, &p2, w, h) {
                    if !overlay_only {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            stroke,
                        );
                    }
                }
            }

            // Draw grid lines parallel to X axis (varying x, fixed z)
            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * major_step;
                let stroke = major_stroke;
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                if let Some((a, b)) = project_segment(&self.camera, &p1, &p2, w, h) {
                    if !overlay_only {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            stroke,
                        );
                    }
                }
            }
        }

        // Draw Axes
        let axis_len = view_range;
        let red_stroke = Stroke::new(2.0, Color32::from_rgb(220, 50, 50));
        let green_stroke = Stroke::new(2.0, Color32::from_rgb(50, 180, 50));
        let blue_stroke = Stroke::new(2.0, Color32::from_rgb(50, 50, 220));

        // X Axis
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            w,
            h,
        ) {
            if !overlay_only {
                painter.line_segment(
                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                    red_stroke,
                );
            }
        }
        // Y Axis (vertical)
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            w,
            h,
        ) {
            if !overlay_only {
                painter.line_segment(
                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                    green_stroke,
                );
            }
        }
        // Z Axis
        if let Some((a, b)) = project_segment(
            &self.camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            w,
            h,
        ) {
            if !overlay_only {
                painter.line_segment(
                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                    blue_stroke,
                );
            }
        }

        // Axis labels
        let label_font = egui::FontId::proportional(14.0);
        if let Some(pos) = self.camera.project(&Point3D::new(axis_len, 0.0, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "X",
                label_font.clone(),
                red_stroke.color,
            );
        }
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, axis_len, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "Y",
                label_font.clone(),
                green_stroke.color,
            );
        }
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, axis_len), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "Z",
                label_font.clone(),
                blue_stroke.color,
            );
        }

        if canvas_resize_preview {
            return;
        }

        // Draw Axis Numbers
        let precision = if major_step > 0.0 {
            let log = major_step.log10();
            if log < 0.0 {
                (log.abs().ceil() as usize + 2).clamp(1, 14)
            } else {
                0
            }
        } else {
            2
        };
        let format_num = |v: f64| -> String {
            if v.abs() < major_step * 1e-5 {
                return "0".to_string();
            }
            let mut s = format!("{:.*}", precision, v);
            if s.contains('.') {
                s = s.trim_end_matches('0').to_string();
                s = s.trim_end_matches('.').to_string();
            }
            if s.is_empty() || s == "-" {
                "0".to_string()
            } else {
                s
            }
        };

        let text_color = if self.dark_mode {
            Color32::from_gray(180)
        } else {
            Color32::from_gray(80)
        };
        let font = egui::FontId::proportional(grafito_ui::tokens::TYPE_XS);
        let tick_stroke = Stroke::new(1.0, text_color);

        // Numbers on X Axis (Z=0, Y=0)
        let start_x_num = ((center_x - view_range) / major_step).floor() * major_step;
        let end_x_num = ((center_x + view_range) / major_step).ceil() * major_step;
        let num_count_x = ((end_x_num - start_x_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_x <= 2000 {
            for xi in 0..=num_count_x {
                let x = start_x_num + xi as f64 * major_step;
                if x.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = x - cam_pos.x as f64;
                let dy = 0.0 - cam_pos.y as f64;
                let dz = 0.0 - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.sanitized_distance() as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(x, 0.0, -tick_size),
                    &Point3D::new(x, 0.0, tick_size),
                    w,
                    h,
                ) {
                    if !overlay_only {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            tick_stroke,
                        );
                    }
                }
                if let Some(pos) = self.camera.project(&Point3D::new(x, 0.0, 0.0), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 50.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(0.0, 6.0),
                        egui::Align2::CENTER_TOP,
                        format_num(x),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Numbers on Y Axis (vertical: X=0, Z=0)
        let center_y = self.camera.target.y as f64;
        let start_y_num = ((center_y - view_range) / major_step).floor() * major_step;
        let end_y_num = ((center_y + view_range) / major_step).ceil() * major_step;
        let num_count_y = ((end_y_num - start_y_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_y <= 2000 {
            for yi in 0..=num_count_y {
                let y = start_y_num + yi as f64 * major_step;
                if y.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = 0.0 - cam_pos.x as f64;
                let dy = y - cam_pos.y as f64;
                let dz = 0.0 - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.sanitized_distance() as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(-tick_size, y, 0.0),
                    &Point3D::new(tick_size, y, 0.0),
                    w,
                    h,
                ) {
                    if !overlay_only {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            tick_stroke,
                        );
                    }
                }
                if let Some(pos) = self.camera.project(&Point3D::new(0.0, y, 0.0), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 40.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(-6.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        format_num(y),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Numbers on Z Axis (X=0, Y=0)
        let start_z_num = ((center_z - view_range) / major_step).floor() * major_step;
        let end_z_num = ((center_z + view_range) / major_step).ceil() * major_step;
        let num_count_z = ((end_z_num - start_z_num) / major_step).round() as i64;
        let mut prev_screen_pos: Option<Vec2> = None;
        if num_count_z <= 2000 {
            for zi in 0..=num_count_z {
                let z = start_z_num + zi as f64 * major_step;
                if z.abs() < major_step * 1e-5 {
                    continue;
                }
                let cam_pos = self.camera.position();
                let dx = 0.0 - cam_pos.x as f64;
                let dy = 0.0 - cam_pos.y as f64;
                let dz = z - cam_pos.z as f64;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist > self.camera.sanitized_distance() as f64 * 1.8 {
                    continue;
                }

                let tick_size = major_step * 0.05;
                if let Some((a, b)) = project_segment(
                    &self.camera,
                    &Point3D::new(-tick_size, 0.0, z),
                    &Point3D::new(tick_size, 0.0, z),
                    w,
                    h,
                ) {
                    if !overlay_only {
                        painter.line_segment(
                            [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                            tick_stroke,
                        );
                    }
                }
                if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, z), w, h) {
                    let sp = Vec2::new(pos.0, pos.1);
                    if let Some(prev) = prev_screen_pos {
                        if (sp - prev).length() < 50.0 {
                            continue;
                        }
                    }
                    prev_screen_pos = Some(sp);
                    painter.text(
                        origin + sp + Vec2::new(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format_num(z),
                        font.clone(),
                        text_color,
                    );
                }
            }
        }

        // Origin "0" label
        if let Some(pos) = self.camera.project(&Point3D::new(0.0, 0.0, 0.0), w, h) {
            painter.text(
                origin + Vec2::new(pos.0, pos.1) + Vec2::new(-6.0, 6.0),
                egui::Align2::RIGHT_TOP,
                "0",
                font.clone(),
                text_color,
            );
        }
    }

    pub(crate) fn draw_3d_objects(
        &mut self,
        painter: &egui::Painter,
        canvas: Rect,
        w: f32,
        h: f32,
        options: Cpu3dRenderOptions,
    ) {
        let Cpu3dRenderOptions {
            overlay_only,
            motion_preview,
            typed_four_d_phase,
        } = options;
        let origin = canvas.min;
        let label_color = if self.dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        };

        // The CPU fallback has no depth buffer, so use camera-space depth for
        // painter ordering. Radial distance is incorrect for off-axis objects.
        let mut objects_with_depth: Vec<(f32, &GeoObject, Option<ProjectedRegularPolytope>)> =
            Vec::new();

        for (_, obj) in self.document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }

            // Keep the one typed projection used for both depth sorting and drawing.
            // GPU-composited, unlabeled typed objects do not need a CPU projection at all.
            let typed_projection = match obj {
                GeoObject::RegularPolychoron4D(polychoron) => {
                    if !typed_cpu_projection_is_needed(obj, overlay_only) {
                        continue;
                    }
                    project_regular_polychoron_cpu(
                        polychoron,
                        typed_four_d_phase_for_object(obj, typed_four_d_phase),
                    )
                }
                GeoObject::RegularPolytopeND(polytope) => {
                    if !typed_cpu_projection_is_needed(obj, overlay_only) {
                        continue;
                    }
                    project_regular_polytope_nd_cpu(
                        polytope,
                        typed_four_d_phase_for_object(obj, typed_four_d_phase),
                    )
                }
                _ => None,
            };

            let center = match obj {
                GeoObject::Point3D(p) => p.position.to_vec3(),
                GeoObject::Segment3D(l) => (l.a.to_vec3() + l.b.to_vec3()) * 0.5,
                GeoObject::Plane3D(p) => plane_point_and_basis(p.a, p.b, p.c, p.d)
                    .map(|(point, _, _)| point.to_vec3())
                    .unwrap_or(Vec3::ZERO),
                GeoObject::Line3D(l) => l.point.to_vec3(),
                GeoObject::Sphere3D(s) => s.center.to_vec3(),
                GeoObject::Cube3D(c) => c.center.to_vec3(),
                GeoObject::Tetrahedron3D(t) => t.center.to_vec3(),
                GeoObject::Pyramid3D(p) => (p.base_center.to_vec3() + p.apex.to_vec3()) * 0.5,
                GeoObject::Cone3D(c) => (c.base_center.to_vec3() + c.apex.to_vec3()) * 0.5,
                GeoObject::Cylinder3D(c) => {
                    (c.base_center.to_vec3() + c.top_center.to_vec3()) * 0.5
                }
                GeoObject::Torus3D(t) => t.center.to_vec3(),
                GeoObject::MoebiusStrip(m) => m.center.to_vec3(),
                GeoObject::Surface3D(surface) => {
                    grafito_core::parametric_sampling::evaluate_surface_3d(
                        surface,
                        2,
                        &self.document.variables,
                    )
                    .get(1)
                    .and_then(|row| row.get(1))
                    .map(|point| point.to_vec3())
                    .unwrap_or(Vec3::ZERO)
                }
                GeoObject::ParametricCurve3D(c) => {
                    grafito_core::parametric_sampling::evaluate_parametric_curve_3d(
                        c,
                        2,
                        &self.document.variables,
                    )
                    .get(1)
                    .map(|&(x, y, z)| Vec3::new(x as f32, y as f32, z as f32))
                    .unwrap_or(Vec3::ZERO)
                }
                GeoObject::Attractor3D(a) => Vec3::new(a.x0 as f32, a.y0 as f32, a.z0 as f32) * 0.2,
                GeoObject::RegularPolychoron4D(_) => typed_projection
                    .as_ref()
                    .and_then(projected_polytope_center)
                    .unwrap_or(Vec3::ZERO),
                GeoObject::RegularPolytopeND(_) => typed_projection
                    .as_ref()
                    .and_then(projected_polytope_center)
                    .unwrap_or(Vec3::ZERO),
                GeoObject::HyperSurface4D(_) => Vec3::ZERO,
                GeoObject::VectorField3D(v) => Vec3::new(
                    (v.x_min + v.x_max) as f32 * 0.5,
                    (v.y_min + v.y_max) as f32 * 0.5,
                    (v.z_min + v.z_max) as f32 * 0.5,
                ),
                GeoObject::Prism3D(prism) => {
                    let base = grafito_render::prism_base_vertices(prism);
                    let base_center = base
                        .iter()
                        .fold(Vec3::ZERO, |sum, point| sum + point.to_vec3())
                        / base.len().max(1) as f32;
                    base_center + prism.direction.to_vec3() * 0.5
                }
                GeoObject::Quadric3D(quadric) => grafito_render::quadric_ellipsoid_params(quadric)
                    .map(|ellipsoid| ellipsoid.center.to_vec3())
                    .unwrap_or(Vec3::ZERO),
                _ => continue, // Skip non-3D objects
            };

            let depth = camera_view_depth(&self.camera, center);
            if depth.is_finite() {
                objects_with_depth.push((depth, obj, typed_projection));
            }
        }

        objects_with_depth.sort_by(|a, b| b.0.total_cmp(&a.0));

        // Render objects in sorted order
        for (_, obj, typed_projection) in objects_with_depth {
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(pt) =
                        projected_point_position(&self.camera, p.position, Vec2::new(w, h))
                    {
                        let pos = origin + pt;
                        if should_draw_cpu_3d_geometry(obj, overlay_only) {
                            painter.circle_filled(pos, p.size.min(5.0), to_color32(p.color));
                        }
                        if !p.label.is_empty() {
                            painter.text(
                                pos + Vec2::new(6.0, -6.0),
                                egui::Align2::LEFT_BOTTOM,
                                &p.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Segment3D(l) => {
                    if let Some((a, b)) = project_segment(&self.camera, &l.a, &l.b, w, h) {
                        if !overlay_only {
                            painter.line_segment(
                                [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                                Stroke::new(l.width, to_color32(l.color)),
                            );
                        }
                        if !l.label.is_empty() {
                            let mid = (a.0 + b.0) * 0.5;
                            let mid_y = (a.1 + b.1) * 0.5;
                            painter.text(
                                origin + Vec2::new(mid, mid_y - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                &l.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Plane3D(p) => {
                    if let Some((center, u, v)) = plane_point_and_basis(p.a, p.b, p.c, p.d) {
                        let extent = 8.0;
                        let steps = 8;
                        let color = to_color32(p.color);
                        let stroke =
                            Stroke::new(1.0, color.linear_multiply(p.opacity.clamp(0.1, 1.0)));
                        if !overlay_only {
                            for i in 0..=steps {
                                let t = -extent + 2.0 * extent * i as f32 / steps as f32;
                                let a = offset_point(center, u, v, -extent, t);
                                let b = offset_point(center, u, v, extent, t);
                                if let Some((pa, pb)) = project_segment(&self.camera, &a, &b, w, h)
                                {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pa.0, pa.1),
                                            origin + Vec2::new(pb.0, pb.1),
                                        ],
                                        stroke,
                                    );
                                }
                                let a = offset_point(center, u, v, t, -extent);
                                let b = offset_point(center, u, v, t, extent);
                                if let Some((pa, pb)) = project_segment(&self.camera, &a, &b, w, h)
                                {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pa.0, pa.1),
                                            origin + Vec2::new(pb.0, pb.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }
                        if !p.label.is_empty() {
                            if let Some(pt) = self.camera.project(&center, w, h) {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1 - 8.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &p.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::Line3D(l) => {
                    let dir = l.direction.to_vec3();
                    if dir.length_squared() > 1e-12 {
                        let unit = dir.normalize();
                        let a = Point3D::from_vec3(l.point.to_vec3() - unit * 40.0);
                        let b = Point3D::from_vec3(l.point.to_vec3() + unit * 40.0);
                        if let Some((pa, pb)) = project_segment(&self.camera, &a, &b, w, h) {
                            if !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pa.0, pa.1),
                                        origin + Vec2::new(pb.0, pb.1),
                                    ],
                                    Stroke::new(l.width, to_color32(l.color)),
                                );
                            }
                            if !l.label.is_empty() {
                                painter.text(
                                    origin
                                        + Vec2::new((pa.0 + pb.0) * 0.5, (pa.1 + pb.1) * 0.5 - 8.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &l.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::Sphere3D(s) => {
                    // Wireframe with lighting: 3 orthogonal great circles
                    let center = s.center.to_vec3();
                    let r = s.radius as f32;
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize(); // Light from upper-right

                    let axes = [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)];
                    for &(u, v) in &axes {
                        let pts_3d: Vec<Vec3> = Camera3D::circle_points(center, u, v, r, 32);

                        let pts_screen: Vec<Option<(f32, f32)>> = pts_3d
                            .iter()
                            .map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                            .collect();

                        for i in 0..pts_3d.len() {
                            let i2 = (i + 1) % pts_3d.len();

                            if let (Some(p1), Some(p2)) = (pts_screen[i], pts_screen[i2]) {
                                // Calculate normal at midpoint of segment
                                let mid_3d = (pts_3d[i] + pts_3d[i2]) * 0.5;
                                let normal = (mid_3d - center).normalize();

                                // Apply lighting
                                let lit_color =
                                    grafito_render::calculate_lighting(s.color, normal, light_dir);
                                let stroke = Stroke::new(s.width, to_color32(lit_color));

                                if !overlay_only {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(p1.0, p1.1),
                                            origin + Vec2::new(p2.0, p2.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }
                    }
                    if !s.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(s.center.x, s.center.y + s.radius + 0.3, s.center.z),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &s.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cube3D(cube) => {
                    let geom = grafito_geometry::Cube3D::new(cube.center, cube.size);
                    let vs = geom.vertices();
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Edges with their face normals for lighting
                    let edges_with_normals = [
                        // Bottom face (normal: -Y)
                        ((0, 1), Vec3::new(0.0, -1.0, 0.0)),
                        ((1, 2), Vec3::new(0.0, -1.0, 0.0)),
                        ((2, 3), Vec3::new(0.0, -1.0, 0.0)),
                        ((3, 0), Vec3::new(0.0, -1.0, 0.0)),
                        // Top face (normal: +Y)
                        ((4, 5), Vec3::new(0.0, 1.0, 0.0)),
                        ((5, 6), Vec3::new(0.0, 1.0, 0.0)),
                        ((6, 7), Vec3::new(0.0, 1.0, 0.0)),
                        ((7, 4), Vec3::new(0.0, 1.0, 0.0)),
                        // Vertical edges (use average of adjacent face normals)
                        ((0, 4), Vec3::new(-1.0, 0.0, -1.0).normalize()),
                        ((1, 5), Vec3::new(1.0, 0.0, -1.0).normalize()),
                        ((2, 6), Vec3::new(1.0, 0.0, 1.0).normalize()),
                        ((3, 7), Vec3::new(-1.0, 0.0, 1.0).normalize()),
                    ];

                    for &((a, b), normal) in &edges_with_normals {
                        if let (Some(pa), Some(pb)) = (
                            self.camera.project(&vs[a], w, h),
                            self.camera.project(&vs[b], w, h),
                        ) {
                            let lit_color =
                                grafito_render::calculate_lighting(cube.color, normal, light_dir);
                            let stroke = Stroke::new(cube.width, to_color32(lit_color));
                            if !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pa.0, pa.1),
                                        origin + Vec2::new(pb.0, pb.1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                    if !cube.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(
                                cube.center.x,
                                cube.center.y + cube.size * 0.7,
                                cube.center.z,
                            ),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &cube.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Tetrahedron3D(tetrahedron) => {
                    let geometry = grafito_geometry::Tetrahedron3D::new(
                        tetrahedron.center,
                        tetrahedron.edge_length,
                    );
                    let vertices = geometry.vertices();
                    if !overlay_only {
                        if let Some(fill) = tetrahedron.fill_color {
                            for (_, face) in
                                projected_tetrahedron_faces(&self.camera, &geometry, w, h)
                            {
                                painter.add(egui::Shape::convex_polygon(
                                    face.into_iter()
                                        .map(|(x, y)| origin + Vec2::new(x, y))
                                        .collect(),
                                    to_color32(fill),
                                    Stroke::NONE,
                                ));
                            }
                        }
                        let stroke = Stroke::new(tetrahedron.width, to_color32(tetrahedron.color));
                        for [start, end] in geometry.edges() {
                            if let Some((a, b)) = project_segment(
                                &self.camera,
                                &vertices[start],
                                &vertices[end],
                                w,
                                h,
                            ) {
                                painter.line_segment(
                                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                                    stroke,
                                );
                            }
                        }
                    }
                    if !tetrahedron.label.is_empty() {
                        if let Some(point) = self.camera.project(&vertices[0], w, h) {
                            painter.text(
                                origin + Vec2::new(point.0, point.1 - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                &tetrahedron.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Pyramid3D(py) => {
                    let geom =
                        grafito_geometry::Pyramid3D::new(py.base_center, py.apex, py.base_size);
                    let base = geom.base_vertices();
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Render base edges (normal: -Y)
                    let base_normal = Vec3::new(0.0, -1.0, 0.0);
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        let a_proj = self.camera.project(&base[i], w, h);
                        let b_proj = self.camera.project(&base[j], w, h);
                        if let (Some(a), Some(b)) = (a_proj, b_proj) {
                            let lit_color = grafito_render::calculate_lighting(
                                py.color,
                                base_normal,
                                light_dir,
                            );
                            let stroke = Stroke::new(py.width, to_color32(lit_color));
                            if !overlay_only {
                                painter.line_segment(
                                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(b.0, b.1)],
                                    stroke,
                                );
                            }
                        }
                    }

                    // Render lateral edges (calculate normal for each triangular face)
                    let apex_proj = self.camera.project(&py.apex, w, h);
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        let a_proj = self.camera.project(&base[i], w, h);

                        // Calculate face normal using cross product
                        let v1 = base[j].to_vec3() - base[i].to_vec3();
                        let v2 = py.apex.to_vec3() - base[i].to_vec3();
                        let face_normal = v1.cross(v2).normalize();

                        if let (Some(a), Some(ap)) = (a_proj, apex_proj) {
                            let lit_color = grafito_render::calculate_lighting(
                                py.color,
                                face_normal,
                                light_dir,
                            );
                            let stroke = Stroke::new(py.width, to_color32(lit_color));
                            if !overlay_only {
                                painter.line_segment(
                                    [origin + Vec2::new(a.0, a.1), origin + Vec2::new(ap.0, ap.1)],
                                    stroke,
                                );
                            }
                        }
                    }

                    if !py.label.is_empty() {
                        if let Some(pt) = self.camera.project(&py.apex, w, h) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1 + 14.0),
                                egui::Align2::CENTER_TOP,
                                &py.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cone3D(cone) => {
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Base circle (normal: -Y)
                    let base_normal = Vec3::new(0.0, -1.0, 0.0);
                    let base_pts_3d: Vec<Vec3> = Camera3D::circle_points(
                        cone.base_center.to_vec3(),
                        Vec3::X,
                        Vec3::Z,
                        cone.radius as f32,
                        32,
                    );
                    let base_pts: Vec<(f32, f32)> = base_pts_3d
                        .iter()
                        .filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                        .collect();

                    for i in 0..base_pts.len() {
                        let j = (i + 1) % base_pts.len();
                        let lit_color =
                            grafito_render::calculate_lighting(cone.color, base_normal, light_dir);
                        let stroke = Stroke::new(cone.width, to_color32(lit_color));
                        if !overlay_only {
                            painter.line_segment(
                                [
                                    origin + Vec2::new(base_pts[i].0, base_pts[i].1),
                                    origin + Vec2::new(base_pts[j].0, base_pts[j].1),
                                ],
                                stroke,
                            );
                        }
                    }

                    // Lines from base to apex (calculate lateral surface normal)
                    if let Some(ap) = self.camera.project(&cone.apex, w, h) {
                        for bp_3d in &base_pts_3d {
                            let bp_3d = *bp_3d;
                            if let Some(bp) = self.camera.project(&Point3D::from_vec3(bp_3d), w, h)
                            {
                                // Calculate lateral surface normal at this point
                                let radial = (bp_3d - cone.base_center.to_vec3()).normalize();
                                let axial =
                                    (cone.apex.to_vec3() - cone.base_center.to_vec3()).normalize();
                                let lateral_normal = (radial + axial * 0.5).normalize();

                                let lit_color = grafito_render::calculate_lighting(
                                    cone.color,
                                    lateral_normal,
                                    light_dir,
                                );
                                let stroke = Stroke::new(cone.width, to_color32(lit_color));
                                if !overlay_only {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(bp.0, bp.1),
                                            origin + Vec2::new(ap.0, ap.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }
                    }

                    if !cone.label.is_empty() {
                        if let Some(pt) = self.camera.project(&cone.apex, w, h) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1 + 14.0),
                                egui::Align2::CENTER_TOP,
                                &cone.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Cylinder3D(cyl) => {
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                    // Top and bottom circles with their normals
                    let circles = [
                        (cyl.base_center, Vec3::new(0.0, -1.0, 0.0)), // Bottom (normal: -Y)
                        (cyl.top_center, Vec3::new(0.0, 1.0, 0.0)),   // Top (normal: +Y)
                    ];

                    for &(center, normal) in &circles {
                        let pts_3d: Vec<Vec3> = Camera3D::circle_points(
                            center.to_vec3(),
                            Vec3::X,
                            Vec3::Z,
                            cyl.radius as f32,
                            24,
                        );
                        let pts: Vec<(f32, f32)> = pts_3d
                            .iter()
                            .filter_map(|pt| self.camera.project(&Point3D::from_vec3(*pt), w, h))
                            .collect();

                        for i in 0..pts.len() {
                            let j = (i + 1) % pts.len();
                            let lit_color =
                                grafito_render::calculate_lighting(cyl.color, normal, light_dir);
                            let stroke = Stroke::new(cyl.width, to_color32(lit_color));
                            if !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i].0, pts[i].1),
                                        origin + Vec2::new(pts[j].0, pts[j].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }

                    // Vertical lines with radial normals
                    if let (Some(_a), Some(_b)) = (
                        self.camera.project(&cyl.base_center, w, h),
                        self.camera.project(&cyl.top_center, w, h),
                    ) {
                        for angle in [
                            0.0,
                            std::f32::consts::PI * 0.5,
                            std::f32::consts::PI,
                            std::f32::consts::PI * 1.5,
                        ] {
                            let rx = angle.cos() * cyl.radius as f32;
                            let rz = angle.sin() * cyl.radius as f32;

                            // Radial normal pointing outward
                            let radial_normal = Vec3::new(angle.cos(), 0.0, angle.sin());

                            let ca = self.camera.project(
                                &Point3D::new(
                                    cyl.base_center.x + rx as f64,
                                    cyl.base_center.y,
                                    cyl.base_center.z + rz as f64,
                                ),
                                w,
                                h,
                            );
                            let cb = self.camera.project(
                                &Point3D::new(
                                    cyl.top_center.x + rx as f64,
                                    cyl.top_center.y,
                                    cyl.top_center.z + rz as f64,
                                ),
                                w,
                                h,
                            );

                            if let (Some(ca), Some(cb)) = (ca, cb) {
                                let lit_color = grafito_render::calculate_lighting(
                                    cyl.color,
                                    radial_normal,
                                    light_dir,
                                );
                                let stroke = Stroke::new(cyl.width, to_color32(lit_color));
                                if !overlay_only {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(ca.0, ca.1),
                                            origin + Vec2::new(cb.0, cb.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }
                    }

                    if !cyl.label.is_empty() {
                        if let Some(pt) = self.camera.project(
                            &Point3D::new(
                                cyl.top_center.x,
                                cyl.top_center.y + 0.5,
                                cyl.top_center.z,
                            ),
                            w,
                            h,
                        ) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1),
                                egui::Align2::CENTER_BOTTOM,
                                &cyl.label,
                                egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                label_color,
                            );
                        }
                    }
                }
                GeoObject::Torus3D(torus) => {
                    let stroke = Stroke::new(torus.width, to_color32(torus.color));
                    let steps_u = 24;
                    let steps_v = 12;
                    let r_maj = torus.r_major;
                    let r = torus.r_minor;
                    let mut pts = vec![vec![(0.0_f32, 0.0_f32); steps_v + 1]; steps_u + 1];
                    let mut valid = vec![vec![false; steps_v + 1]; steps_u + 1];
                    for i in 0..=steps_u {
                        let u = (i as f64) * std::f64::consts::PI * 2.0 / (steps_u as f64);
                        for j in 0..=steps_v {
                            let v = (j as f64) * std::f64::consts::PI * 2.0 / (steps_v as f64);
                            let x = torus.center.x + (r_maj + r * v.cos()) * u.cos();
                            let z = torus.center.z + (r_maj + r * v.cos()) * u.sin();
                            let y = torus.center.y + r * v.sin();
                            if let Some(p) = self.camera.project(&Point3D::new(x, y, z), w, h) {
                                pts[i][j] = (p.0, p.1);
                                valid[i][j] = true;
                            }
                        }
                    }
                    for i in 0..steps_u {
                        for j in 0..steps_v {
                            if valid[i][j] && valid[i + 1][j] && !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i + 1][j].0, pts[i + 1][j].1),
                                    ],
                                    stroke,
                                );
                            }
                            if valid[i][j] && valid[i][j + 1] && !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i][j + 1].0, pts[i][j + 1].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                }
                GeoObject::MoebiusStrip(moe) => {
                    let stroke = Stroke::new(moe.width, to_color32(moe.color));
                    let steps_u = 32;
                    let steps_v = 8;
                    let r_maj = moe.radius;
                    let w_max = moe.width_r;
                    let mut pts = vec![vec![(0.0_f32, 0.0_f32); steps_v + 1]; steps_u + 1];
                    let mut valid = vec![vec![false; steps_v + 1]; steps_u + 1];
                    for i in 0..=steps_u {
                        let u = (i as f64) * std::f64::consts::PI * 2.0 / (steps_u as f64);
                        for j in 0..=steps_v {
                            let v = -w_max / 2.0 + w_max * (j as f64) / (steps_v as f64);
                            let x = moe.center.x + (r_maj + v * (u / 2.0).cos()) * u.cos();
                            let z = moe.center.z + (r_maj + v * (u / 2.0).cos()) * u.sin();
                            let y = moe.center.y + v * (u / 2.0).sin();
                            if let Some(p) = self.camera.project(&Point3D::new(x, y, z), w, h) {
                                pts[i][j] = (p.0, p.1);
                                valid[i][j] = true;
                            }
                        }
                    }
                    for i in 0..steps_u {
                        for j in 0..=steps_v {
                            if valid[i][j] && valid[i + 1][j] && !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i + 1][j].0, pts[i + 1][j].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                    for i in 0..=steps_u {
                        for j in 0..steps_v {
                            if valid[i][j] && valid[i][j + 1] && !overlay_only {
                                painter.line_segment(
                                    [
                                        origin + Vec2::new(pts[i][j].0, pts[i][j].1),
                                        origin + Vec2::new(pts[i][j + 1].0, pts[i][j + 1].1),
                                    ],
                                    stroke,
                                );
                            }
                        }
                    }
                }
                GeoObject::Surface3D(surf) => {
                    if surf.is_parametric {
                        // Parametric surfaces must remain visible while the GPU callback is
                        // compiling, unavailable, or recovering from a failed scene upload.
                        let grid = grafito_core::parametric_sampling::samples_or_compute_surface(
                            surf,
                            motion_preview_surface_resolution(
                                surf.mesh_res.clamp(8, 50),
                                motion_preview,
                            ),
                            &self.document.variables,
                        );
                        let color = to_color32(surf.color);
                        let stroke = Stroke::new(surf.width, color);
                        let project = |point: Point3D| {
                            self.camera
                                .project(&point, w, h)
                                .map(|(x, y)| origin + Vec2::new(x, y))
                        };
                        for rows in grid.windows(2) {
                            for column in 0..rows[0].len().saturating_sub(1) {
                                let p00 = rows[0][column];
                                let p10 = rows[1][column];
                                let p01 = rows[0][column + 1];
                                let p11 = rows[1][column + 1];
                                if surf.solid && !overlay_only {
                                    if let (Some(a), Some(b), Some(c)) =
                                        (project(p00), project(p10), project(p11))
                                    {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![a, b, c],
                                            color,
                                            Stroke::NONE,
                                        ));
                                    }
                                    if let (Some(a), Some(b), Some(c)) =
                                        (project(p00), project(p11), project(p01))
                                    {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![a, b, c],
                                            color,
                                            Stroke::NONE,
                                        ));
                                    }
                                }
                                if !overlay_only {
                                    if let (Some(a), Some(b)) = (project(p00), project(p10)) {
                                        painter.line_segment([a, b], stroke);
                                    }
                                    if let (Some(a), Some(b)) = (project(p00), project(p01)) {
                                        painter.line_segment([a, b], stroke);
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    if surf.solid && !overlay_only {
                        // Solid Gouraud-shaded surface
                        let res = motion_preview_surface_resolution(
                            surf.mesh_res.clamp(8, 50),
                            motion_preview,
                        );
                        let xs = surf.x_min;
                        let xe = surf.x_max;
                        let ys = surf.y_min;
                        let ye = surf.y_max;
                        let dx = (xe - xs) / res as f64;
                        let dy = (ye - ys) / res as f64;

                        // Evaluate all z values — checked_mul + try_reserve para evitar OOM.
                        let dim = match res.checked_add(1) {
                            Some(dim) => dim,
                            None => continue,
                        };
                        let capacity = match dim.checked_mul(dim) {
                            Some(cap) => cap,
                            None => continue,
                        };
                        let mut pts = Vec::new();
                        if pts.try_reserve(capacity).is_err() {
                            continue;
                        }
                        for i in 0..=res {
                            for j in 0..=res {
                                pts.push((xs + j as f64 * dx, ys + i as f64 * dy));
                            }
                        }
                        if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                            &surf.expr,
                            pts.iter().copied(),
                            &self.document.variables,
                        ) {
                            let light_dir = glam::Vec3::new(0.5, 0.8, 0.3).normalize();
                            let ambient = 0.4;
                            let base = to_color32(surf.color);
                            // Triangulate and draw depth-sorted faces
                            for i in 0..res {
                                for j in 0..res {
                                    let idx00 = i * (res + 1) + j;
                                    let idx10 = i * (res + 1) + j + 1;
                                    let idx01 = (i + 1) * (res + 1) + j;
                                    let idx11 = (i + 1) * (res + 1) + j + 1;

                                    let (x0, y0) = pts[idx00];
                                    let z0 = z_vals[idx00].unwrap_or(f64::NAN);
                                    let (x1, y1) = pts[idx10];
                                    let z1 = z_vals[idx10].unwrap_or(f64::NAN);
                                    let (x2, y2) = pts[idx01];
                                    let z2 = z_vals[idx01].unwrap_or(f64::NAN);
                                    let (x3, y3) = pts[idx11];
                                    let z3 = z_vals[idx11].unwrap_or(f64::NAN);

                                    if !z0.is_finite()
                                        || !z1.is_finite()
                                        || !z2.is_finite()
                                        || !z3.is_finite()
                                        || z0.abs() > 100.0
                                        || z1.abs() > 100.0
                                        || z2.abs() > 100.0
                                        || z3.abs() > 100.0
                                    {
                                        continue;
                                    }

                                    // Compute two triangle normals
                                    let v00 = surf.explicit_sample_point(x0, y0, z0).to_vec3();
                                    let v10 = surf.explicit_sample_point(x1, y1, z1).to_vec3();
                                    let v01 = surf.explicit_sample_point(x2, y2, z2).to_vec3();
                                    let v11 = surf.explicit_sample_point(x3, y3, z3).to_vec3();

                                    let n1 = (v10 - v00).cross(v01 - v00).normalize();
                                    let n2 = (v11 - v10).cross(v01 - v10).normalize();

                                    let shade1 = (ambient
                                        + (1.0 - ambient) * n1.dot(light_dir).max(0.0))
                                    .clamp(0.0, 1.0);
                                    let shade2 = (ambient
                                        + (1.0 - ambient) * n2.dot(light_dir).max(0.0))
                                    .clamp(0.0, 1.0);

                                    let c1 = Color32::from_rgba_unmultiplied(
                                        (base.r() as f32 * shade1) as u8,
                                        (base.g() as f32 * shade1) as u8,
                                        (base.b() as f32 * shade1) as u8,
                                        255,
                                    );
                                    let c2 = Color32::from_rgba_unmultiplied(
                                        (base.r() as f32 * shade2) as u8,
                                        (base.g() as f32 * shade2) as u8,
                                        (base.b() as f32 * shade2) as u8,
                                        255,
                                    );

                                    // Project and draw triangle 1
                                    if let (Some(p0), Some(p1), Some(p2)) = (
                                        self.camera.project(
                                            &surf.explicit_sample_point(x0, y0, z0),
                                            w,
                                            h,
                                        ),
                                        self.camera.project(
                                            &surf.explicit_sample_point(x1, y1, z1),
                                            w,
                                            h,
                                        ),
                                        self.camera.project(
                                            &surf.explicit_sample_point(x2, y2, z2),
                                            w,
                                            h,
                                        ),
                                    ) {
                                        let pts1 = vec![
                                            origin + Vec2::new(p0.0, p0.1),
                                            origin + Vec2::new(p1.0, p1.1),
                                            origin + Vec2::new(p2.0, p2.1),
                                        ];
                                        painter.add(egui::Shape::convex_polygon(
                                            pts1,
                                            c1,
                                            Stroke::new(0.5, c1),
                                        ));
                                    }
                                    // Project and draw triangle 2
                                    if let (Some(p1), Some(p2), Some(p3)) = (
                                        self.camera.project(
                                            &surf.explicit_sample_point(x1, y1, z1),
                                            w,
                                            h,
                                        ),
                                        self.camera.project(
                                            &surf.explicit_sample_point(x2, y2, z2),
                                            w,
                                            h,
                                        ),
                                        self.camera.project(
                                            &surf.explicit_sample_point(x3, y3, z3),
                                            w,
                                            h,
                                        ),
                                    ) {
                                        let pts2 = vec![
                                            origin + Vec2::new(p1.0, p1.1),
                                            origin + Vec2::new(p2.0, p2.1),
                                            origin + Vec2::new(p3.0, p3.1),
                                        ];
                                        painter.add(egui::Shape::convex_polygon(
                                            pts2,
                                            c2,
                                            Stroke::new(0.5, c2),
                                        ));
                                    }
                                }
                            }
                        }
                        if !surf.label.is_empty() {
                            if let Some(pt) = self.camera.project(
                                &surf.explicit_sample_point(
                                    (surf.x_min + surf.x_max) * 0.5,
                                    (surf.y_min + surf.y_max) * 0.5,
                                    1.0,
                                ),
                                w,
                                h,
                            ) {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1),
                                    egui::Align2::CENTER_BOTTOM,
                                    &surf.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                        continue;
                    }
                    // Original wireframe rendering
                    let stroke = Stroke::new(surf.width, to_color32(surf.color));
                    let steps: usize = 20;
                    let xs = surf.x_min;
                    let xe = surf.x_max;
                    let ys = surf.y_min;
                    let ye = surf.y_max;
                    let x_step = (xe - xs) / steps as f64;
                    let y_step = (ye - ys) / steps as f64;
                    // Collect all (x, y) points for X-axis lines — checked + try_reserve
                    let dim_x = match steps.checked_add(1) {
                        Some(dim) => dim,
                        None => continue,
                    };
                    let cap_x = match dim_x.checked_mul(dim_x) {
                        Some(cap) => cap,
                        None => continue,
                    };
                    let mut pts_x = Vec::new();
                    if pts_x.try_reserve(cap_x).is_err() {
                        continue;
                    }
                    for i in 0..=steps {
                        let y = ys + i as f64 * y_step;
                        for j in 0..=steps {
                            let x = xs + j as f64 * x_step;
                            pts_x.push((x, y));
                        }
                    }

                    if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                        &surf.expr,
                        pts_x.iter().copied(),
                        &self.document.variables,
                    ) {
                        for i in 0..=steps {
                            let mut prev: Option<(f32, f32)> = None;
                            for j in 0..=steps {
                                let idx = i * (steps + 1) + j;
                                let (x, y) = pts_x[idx];
                                if let Some(z) = z_vals[idx] {
                                    if z.is_finite() && z.abs() < 100.0 {
                                        if let Some(pt) = self.camera.project(
                                            &surf.explicit_sample_point(x, y, z),
                                            w,
                                            h,
                                        ) {
                                            if let Some(pp) = prev {
                                                if !overlay_only {
                                                    painter.line_segment(
                                                        [
                                                            origin + Vec2::new(pp.0, pp.1),
                                                            origin + Vec2::new(pt.0, pt.1),
                                                        ],
                                                        stroke,
                                                    );
                                                }
                                            }
                                            prev = Some(pt);
                                            continue;
                                        }
                                    }
                                }
                                prev = None;
                            }
                        }
                    }

                    // Collect all (x, y) points for Y-axis lines — checked + try_reserve
                    let dim_y = match steps.checked_add(1) {
                        Some(dim) => dim,
                        None => continue,
                    };
                    let cap_y = match dim_y.checked_mul(dim_y) {
                        Some(cap) => cap,
                        None => continue,
                    };
                    let mut pts_y = Vec::new();
                    if pts_y.try_reserve(cap_y).is_err() {
                        continue;
                    }
                    for j in 0..=steps {
                        let x = xs + j as f64 * x_step;
                        for i in 0..=steps {
                            let y = ys + i as f64 * y_step;
                            pts_y.push((x, y));
                        }
                    }

                    if let Ok(z_vals) = grafito_geometry::expr::eval_surface_batch(
                        &surf.expr,
                        pts_y.iter().copied(),
                        &self.document.variables,
                    ) {
                        for j in 0..=steps {
                            let mut prev: Option<(f32, f32)> = None;
                            for i in 0..=steps {
                                let idx = j * (steps + 1) + i;
                                let (x, y) = pts_y[idx];
                                if let Some(z) = z_vals[idx] {
                                    if z.is_finite() && z.abs() < 100.0 {
                                        if let Some(pt) = self.camera.project(
                                            &surf.explicit_sample_point(x, y, z),
                                            w,
                                            h,
                                        ) {
                                            if let Some(pp) = prev {
                                                if !overlay_only {
                                                    painter.line_segment(
                                                        [
                                                            origin + Vec2::new(pp.0, pp.1),
                                                            origin + Vec2::new(pt.0, pt.1),
                                                        ],
                                                        stroke,
                                                    );
                                                }
                                            }
                                            prev = Some(pt);
                                            continue;
                                        }
                                    }
                                }
                                prev = None;
                            }
                        }
                    }
                }
                GeoObject::ParametricCurve3D(curve) => {
                    let stroke = Stroke::new(curve.width, to_color32(curve.color));
                    let mut prev: Option<(Point3D, (f32, f32))> = None;
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_3d(
                        curve,
                        500,
                        &self.document.variables,
                    );
                    let stride = motion_preview_sample_stride(samples.len(), motion_preview);
                    for point in samples.iter().step_by(stride) {
                        let (x, y, z) = *point;
                        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                            prev = None;
                            continue;
                        }
                        let point = Point3D::new(x, y, z);
                        if let Some(pt) = self.camera.project(&point, w, h) {
                            if let Some((_, pp)) = prev.filter(|(previous, _)| {
                                curve_3d_segment_is_continuous(*previous, point, &self.camera)
                            }) {
                                if !overlay_only {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pp.0, pp.1),
                                            origin + Vec2::new(pt.0, pt.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                            prev = Some((point, pt));
                            continue;
                        }
                        prev = None;
                    }
                }
                GeoObject::Attractor3D(att) => {
                    use grafito_geometry::attractors::integrate_attractor;
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};

                    // Calculate hash of attractor parameters
                    let mut hasher = DefaultHasher::new();
                    att.attractor_type.hash(&mut hasher);
                    for p in &att.params {
                        p.to_bits().hash(&mut hasher);
                    }
                    att.x0.to_bits().hash(&mut hasher);
                    att.y0.to_bits().hash(&mut hasher);
                    att.z0.to_bits().hash(&mut hasher);
                    att.dt.to_bits().hash(&mut hasher);
                    att.steps.hash(&mut hasher);
                    att.skip.hash(&mut hasher);
                    let param_hash = hasher.finish();

                    // Check cache or compute new points
                    let needs_refresh = self
                        .attractor_cache
                        .get(&att.id)
                        .is_none_or(|(cached_hash, _)| *cached_hash != param_hash);
                    if needs_refresh {
                        let atype = att.model();
                        let new_pts = integrate_attractor(
                            &atype, att.x0, att.y0, att.z0, att.dt, att.steps, att.skip,
                        );
                        self.attractor_cache.insert(att.id, (param_hash, new_pts));
                    }
                    let Some((_, pts)) = self.attractor_cache.get(&att.id) else {
                        continue;
                    };

                    let stroke = Stroke::new(att.width, to_color32(att.color));
                    let mut prev: Option<(f32, f32)> = None;
                    let stride = motion_preview_sample_stride(pts.len(), motion_preview);
                    for pt in pts.iter().step_by(stride) {
                        let scaled_pt = Point3D::new(pt.x * 0.2, pt.y * 0.2, pt.z * 0.2);
                        if let Some(sp) = self.camera.project(&scaled_pt, w, h) {
                            if let Some(pp) = prev {
                                if !overlay_only {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(pp.0, pp.1),
                                            origin + Vec2::new(sp.0, sp.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                            prev = Some(sp);
                        } else {
                            prev = None;
                        }
                    }

                    if !att.label.is_empty() {
                        if let Some(first) = pts.first() {
                            let first = Point3D::new(first.x * 0.2, first.y * 0.2, first.z * 0.2);
                            if let Some(pt) = self.camera.project(&first, w, h) {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1 - 10.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &att.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::RegularPolychoron4D(polychoron) => {
                    let draw_cpu_geometry = should_draw_cpu_3d_geometry(obj, overlay_only);
                    if let Some(geometry) = typed_projection {
                        if draw_cpu_geometry {
                            if should_draw_polychoron_faces(
                                polychoron.fill_color.is_some(),
                                motion_preview,
                            ) {
                                if let Some(fill) = polychoron.fill_color {
                                    for (_, face) in
                                        projected_polychoron_faces(&self.camera, &geometry, w, h)
                                    {
                                        painter.add(egui::Shape::convex_polygon(
                                            face.into_iter()
                                                .map(|(x, y)| origin + Vec2::new(x, y))
                                                .collect(),
                                            to_color32(fill),
                                            Stroke::NONE,
                                        ));
                                    }
                                }
                            }

                            let stroke =
                                Stroke::new(polychoron.width, to_color32(polychoron.color));
                            let stride = motion_preview_polytope_edge_stride(
                                geometry.edges().len(),
                                motion_preview,
                            );
                            for &[first, second] in geometry.edges().iter().step_by(stride) {
                                let Some((start, end)) = geometry
                                    .vertices()
                                    .get(first)
                                    .zip(geometry.vertices().get(second))
                                else {
                                    continue;
                                };
                                if let Some((a, b)) =
                                    project_segment(&self.camera, start, end, w, h)
                                {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(a.0, a.1),
                                            origin + Vec2::new(b.0, b.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }

                        if !polychoron.label.is_empty() {
                            if let Some(point) = geometry
                                .vertices()
                                .first()
                                .and_then(|point| self.camera.project(point, w, h))
                            {
                                painter.text(
                                    origin + Vec2::new(point.0, point.1 - 10.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &polychoron.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::RegularPolytopeND(polytope) => {
                    let draw_cpu_geometry = should_draw_cpu_3d_geometry(obj, overlay_only);
                    if let Some(geometry) = typed_projection {
                        if draw_cpu_geometry {
                            let stroke = Stroke::new(polytope.width, to_color32(polytope.color));
                            let stride = motion_preview_polytope_edge_stride(
                                geometry.edges().len(),
                                motion_preview,
                            );
                            for &[first, second] in geometry.edges().iter().step_by(stride) {
                                let Some((start, end)) = geometry
                                    .vertices()
                                    .get(first)
                                    .zip(geometry.vertices().get(second))
                                else {
                                    continue;
                                };
                                if let Some((a, b)) =
                                    project_segment(&self.camera, start, end, w, h)
                                {
                                    painter.line_segment(
                                        [
                                            origin + Vec2::new(a.0, a.1),
                                            origin + Vec2::new(b.0, b.1),
                                        ],
                                        stroke,
                                    );
                                }
                            }
                        }

                        if !polytope.label.is_empty() {
                            if let Some(point) = geometry
                                .vertices()
                                .first()
                                .and_then(|point| self.camera.project(point, w, h))
                            {
                                painter.text(
                                    origin + Vec2::new(point.0, point.1 - 10.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &polytope.label,
                                    egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::HyperSurface4D(hs) => {
                    let draw_cpu_geometry = should_draw_cpu_3d_geometry(obj, overlay_only);
                    let stroke = Stroke::new(hs.width, to_color32(hs.color));
                    let [a_xy, a_xz, a_xw] = effective_four_d_angles(
                        &hs.rotation_angles,
                        self.transient_render_state.four_d_phase(),
                    );
                    let scale = hs.params.first().copied().unwrap_or(1.0);
                    match hs.surface_type.as_str() {
                        "hypercube" => {
                            let mut verts_4d: Vec<[f64; 4]> = Vec::with_capacity(16);
                            for i in 0..16u8 {
                                verts_4d.push([
                                    if i & 1 != 0 { scale } else { -scale },
                                    if i & 2 != 0 { scale } else { -scale },
                                    if i & 4 != 0 { scale } else { -scale },
                                    if i & 8 != 0 { scale } else { -scale },
                                ]);
                            }
                            let cos_xy = a_xy.cos();
                            let sin_xy = a_xy.sin();
                            let cos_xz = a_xz.cos();
                            let sin_xz = a_xz.sin();
                            let cos_xw = a_xw.cos();
                            let sin_xw = a_xw.sin();
                            let projected: Vec<Point3D> = verts_4d
                                .iter()
                                .map(|v| {
                                    let mut p = *v;
                                    let nx = p[0] * cos_xy - p[1] * sin_xy;
                                    let ny = p[0] * sin_xy + p[1] * cos_xy;
                                    p[0] = nx;
                                    p[1] = ny;
                                    let nx = p[0] * cos_xz - p[2] * sin_xz;
                                    let nz = p[0] * sin_xz + p[2] * cos_xz;
                                    p[0] = nx;
                                    p[2] = nz;
                                    let nx = p[0] * cos_xw - p[3] * sin_xw;
                                    let nw = p[0] * sin_xw + p[3] * cos_xw;
                                    p[0] = nx;
                                    p[3] = nw;
                                    let w_factor = 1.0 / (3.0 - p[3] / scale);
                                    Point3D::new(p[0] * w_factor, p[1] * w_factor, p[2] * w_factor)
                                })
                                .collect();
                            let edges: Vec<(usize, usize)> = (0..16usize)
                                .flat_map(|i| {
                                    (0..16usize)
                                        .filter(move |&j| j > i && (i ^ j).count_ones() == 1)
                                        .map(move |j| (i, j))
                                })
                                .collect();
                            for &(a, b) in &edges {
                                if let (Some(pa), Some(pb)) = (
                                    self.camera.project(&projected[a], w, h),
                                    self.camera.project(&projected[b], w, h),
                                ) {
                                    if draw_cpu_geometry {
                                        painter.line_segment(
                                            [
                                                origin + Vec2::new(pa.0, pa.1),
                                                origin + Vec2::new(pb.0, pb.1),
                                            ],
                                            stroke,
                                        );
                                    }
                                }
                            }
                            if !hs.label.is_empty() {
                                if let Some(pt) = self.camera.project(&projected[0], w, h) {
                                    painter.text(
                                        origin + Vec2::new(pt.0, pt.1 - 10.0),
                                        egui::Align2::CENTER_BOTTOM,
                                        &hs.label,
                                        egui::FontId::proportional(grafito_ui::tokens::TYPE_SM),
                                        label_color,
                                    );
                                }
                            }
                        }
                        "hypersphere" => {
                            let res = hs.resolution.clamp(8, 30);
                            let mut pts_3d: Vec<Vec<Point3D>> = Vec::new();
                            let cos_xy = a_xy.cos();
                            let sin_xy = a_xy.sin();
                            let cos_xz = a_xz.cos();
                            let sin_xz = a_xz.sin();
                            let cos_xw = a_xw.cos();
                            let sin_xw = a_xw.sin();
                            for i in 0..=res {
                                let phi = std::f64::consts::PI * i as f64 / res as f64;
                                let mut ring = Vec::new();
                                for j in 0..=res * 2 {
                                    let theta = std::f64::consts::TAU * j as f64 / (res * 2) as f64;
                                    let mut p = [
                                        scale * phi.sin() * theta.cos(),
                                        scale * phi.sin() * theta.sin(),
                                        scale * phi.cos(),
                                        0.0,
                                    ];
                                    let nx = p[0] * cos_xy - p[1] * sin_xy;
                                    let ny = p[0] * sin_xy + p[1] * cos_xy;
                                    p[0] = nx;
                                    p[1] = ny;
                                    let nx = p[0] * cos_xz - p[2] * sin_xz;
                                    let nz = p[0] * sin_xz + p[2] * cos_xz;
                                    p[0] = nx;
                                    p[2] = nz;
                                    let nx = p[0] * cos_xw - p[3] * sin_xw;
                                    let nw = p[0] * sin_xw + p[3] * cos_xw;
                                    p[0] = nx;
                                    p[3] = nw;
                                    let w_factor = 1.0 / (3.0 - p[3] / scale);
                                    ring.push(Point3D::new(
                                        p[0] * w_factor,
                                        p[1] * w_factor,
                                        p[2] * w_factor,
                                    ));
                                }
                                pts_3d.push(ring);
                            }
                            for ring in &pts_3d {
                                let mut prev: Option<(f32, f32)> = None;
                                for pt in ring {
                                    if let Some(sp) = self.camera.project(pt, w, h) {
                                        if let Some(pp) = prev {
                                            if draw_cpu_geometry {
                                                painter.line_segment(
                                                    [
                                                        origin + Vec2::new(pp.0, pp.1),
                                                        origin + Vec2::new(sp.0, sp.1),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                        }
                                        prev = Some(sp);
                                    } else {
                                        prev = None;
                                    }
                                }
                            }
                            for j in 0..pts_3d.first().map(|r| r.len()).unwrap_or(0) {
                                let mut prev: Option<(f32, f32)> = None;
                                for ring in &pts_3d {
                                    if let Some(pt) = ring.get(j) {
                                        if let Some(sp) = self.camera.project(pt, w, h) {
                                            if let Some(pp) = prev {
                                                if draw_cpu_geometry {
                                                    painter.line_segment(
                                                        [
                                                            origin + Vec2::new(pp.0, pp.1),
                                                            origin + Vec2::new(sp.0, sp.1),
                                                        ],
                                                        stroke,
                                                    );
                                                }
                                            }
                                            prev = Some(sp);
                                        } else {
                                            prev = None;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                GeoObject::VectorField3D(vf) if !overlay_only => {
                    let stroke = Stroke::new(1.5, to_color32(vf.color));
                    for (start, end) in
                        grafito_render::sample_vector_field_3d(vf, &self.document.variables)
                    {
                        if let (Some(pa), Some(pb)) = (
                            self.camera.project(&start, w, h),
                            self.camera.project(&end, w, h),
                        ) {
                            painter.line_segment(
                                [
                                    origin + Vec2::new(pa.0, pa.1),
                                    origin + Vec2::new(pb.0, pb.1),
                                ],
                                stroke,
                            );
                        }
                    }
                }
                GeoObject::Prism3D(prism) => {
                    let base = grafito_render::prism_base_vertices(prism);
                    if base.len() >= 3 {
                        let top = grafito_render::prism_top_vertices(prism);
                        let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();

                        // Caras translúcidas (base, tapa y laterales).
                        if !overlay_only {
                            if let Some(fill) = prism.fill_color {
                                let fill32 = to_color32(fill);
                                // Base y tapa: abanico desde el primer vértice
                                // (válido para bases convexas; las no convexas
                                // se aproximan, igual que el path GPU).
                                for index in 1..base.len().saturating_sub(1) {
                                    for face in [
                                        [base[0], base[index], base[index + 1]],
                                        [top[0], top[index], top[index + 1]],
                                    ] {
                                        if let Some(points) =
                                            projected_face_points(&self.camera, &face, w, h, origin)
                                        {
                                            painter.add(egui::Shape::convex_polygon(
                                                points,
                                                fill32,
                                                Stroke::NONE,
                                            ));
                                        }
                                    }
                                }
                                // Laterales: cada cara es un paralelogramo.
                                for index in 0..base.len() {
                                    let next = (index + 1) % base.len();
                                    if let Some(points) = projected_face_points(
                                        &self.camera,
                                        &[base[index], base[next], top[next], top[index]],
                                        w,
                                        h,
                                        origin,
                                    ) {
                                        painter.add(egui::Shape::convex_polygon(
                                            points,
                                            fill32,
                                            Stroke::NONE,
                                        ));
                                    }
                                }
                            }
                        }

                        // Aristas con iluminación por cara.
                        let base_normal = face_normal(base[0], base[1], base[2]).unwrap_or(Vec3::Y);
                        let top_normal = face_normal(top[0], top[1], top[2]).unwrap_or(Vec3::Y);
                        let vertical_normal = (base_normal + top_normal).normalize_or_zero();
                        for index in 0..base.len() {
                            let next = (index + 1) % base.len();
                            for (a, b, normal) in [
                                (base[index], base[next], base_normal),
                                (top[index], top[next], top_normal),
                                (base[index], top[index], vertical_normal),
                            ] {
                                if let (Some(pa), Some(pb)) =
                                    (self.camera.project(&a, w, h), self.camera.project(&b, w, h))
                                {
                                    let lit = grafito_render::calculate_lighting(
                                        prism.color,
                                        normal,
                                        light_dir,
                                    );
                                    let stroke = Stroke::new(prism.width, to_color32(lit));
                                    if !overlay_only {
                                        painter.line_segment(
                                            [
                                                origin + Vec2::new(pa.0, pa.1),
                                                origin + Vec2::new(pb.0, pb.1),
                                            ],
                                            stroke,
                                        );
                                    }
                                }
                            }
                        }

                        if !prism.label.is_empty() {
                            let top_center = top
                                .iter()
                                .fold(Vec3::ZERO, |sum, point| sum + point.to_vec3())
                                / top.len().max(1) as f32;
                            if let Some(pt) =
                                self.camera.project(&Point3D::from_vec3(top_center), w, h)
                            {
                                painter.text(
                                    origin + Vec2::new(pt.0, pt.1 - 8.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &prism.label,
                                    egui::FontId::proportional(12.0),
                                    label_color,
                                );
                            }
                        }
                    }
                }
                GeoObject::Quadric3D(quadric) => {
                    // Paso intermedio honesto: elipsoide wireframe paramétrico.
                    // TODO(full-quadric): clasificación general y términos cruzados.
                    let ellipsoid = grafito_render::quadric_ellipsoid_params(quadric)
                        .unwrap_or_else(grafito_render::QuadricEllipsoid::placeholder);
                    let center = ellipsoid.center.to_vec3();
                    let radii = ellipsoid.radii;
                    let light_dir = Vec3::new(0.5, 1.0, 0.3).normalize();
                    for (u, v) in [(Vec3::X, Vec3::Y), (Vec3::X, Vec3::Z), (Vec3::Y, Vec3::Z)] {
                        let normal = u.cross(v).normalize_or_zero();
                        let mut prev: Option<(f32, f32)> = None;
                        for index in 0..=32 {
                            let angle = std::f32::consts::TAU * index as f32 / 32.0;
                            let direction = u * angle.cos() + v * angle.sin();
                            let point = Point3D::from_vec3(
                                center
                                    + Vec3::new(
                                        direction.x * radii.x,
                                        direction.y * radii.y,
                                        direction.z * radii.z,
                                    ),
                            );
                            if let Some(projected) = self.camera.project(&point, w, h) {
                                if let Some(prev) = prev {
                                    let lit = grafito_render::calculate_lighting(
                                        quadric.color,
                                        normal,
                                        light_dir,
                                    );
                                    let stroke = Stroke::new(quadric.width, to_color32(lit));
                                    if !overlay_only {
                                        painter.line_segment(
                                            [
                                                origin + Vec2::new(prev.0, prev.1),
                                                origin + Vec2::new(projected.0, projected.1),
                                            ],
                                            stroke,
                                        );
                                    }
                                }
                                prev = Some(projected);
                            } else {
                                prev = None;
                            }
                        }
                    }
                    if !quadric.label.is_empty() {
                        if let Some(pt) = self.camera.project(&ellipsoid.center, w, h) {
                            painter.text(
                                origin + Vec2::new(pt.0, pt.1 - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                &quadric.label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod gpu_overlay_tests {
    use super::*;

    #[test]
    fn point_geometry_is_cpu_owned_only_during_fallback() {
        let point = GeoObject::Point3D(Point3DObj::new(Point3D::new(1.0, 2.0, 3.0)));

        assert!(should_draw_cpu_3d_geometry(&point, false));
        assert!(!should_draw_cpu_3d_geometry(&point, true));
    }

    #[test]
    fn four_d_motion_offsets_all_projection_planes_without_changing_base_angles() {
        let base = [0.3, 0.5, 0.7];

        assert_eq!(effective_four_d_angles(&base, 0.0), base);
        let animated = effective_four_d_angles(&base, 1.0);
        assert_ne!(animated, base);
        assert!(animated[0] > base[0]);
        assert!(animated[1] > base[1]);
        assert!(animated[2] > base[2]);
    }

    #[test]
    fn four_d_rotation_is_continuous_across_the_phase_wrap() {
        let base = [0.3, 0.5, 0.7];
        let before = effective_four_d_angles(&base, std::f64::consts::TAU - 1e-6);
        let after = effective_four_d_angles(&base, 0.0);

        for (left, right) in before.into_iter().zip(after) {
            assert!((left.sin() - right.sin()).abs() < 1e-5);
            assert!((left.cos() - right.cos()).abs() < 1e-5);
        }
    }

    #[test]
    fn motion_preview_downsamples_dense_cpu_3d_paths() {
        assert_eq!(motion_preview_sample_stride(512, true), 1);
        assert_eq!(motion_preview_sample_stride(4_096, true), 4);
        assert_eq!(motion_preview_sample_stride(4_096, false), 1);
    }

    fn test_camera() -> Camera3D {
        let mut camera = Camera3D::new(4.0 / 3.0);
        camera.theta = 0.0;
        camera.phi = 0.0;
        camera.distance = 10.0;
        camera.target = glam::Vec3::ZERO;
        camera
    }

    #[test]
    fn prism_fallback_bounds_cover_base_and_top() {
        use grafito_core::{GeoObject, Prism3DObj};
        use grafito_geometry::Point3D;

        let prism = GeoObject::Prism3D(Prism3DObj::new(
            vec![
                Point3D::new(-1.0, -1.0, 0.0),
                Point3D::new(1.0, -1.0, 0.0),
                Point3D::new(1.0, 1.0, 0.0),
                Point3D::new(-1.0, 1.0, 0.0),
            ],
            Point3D::new(0.0, 0.0, 2.0),
        ));
        let bounds = fallback_object_bounds(&prism, &std::collections::HashMap::new())
            .expect("prism fallback bounds");

        assert!(bounds.min.x <= -1.0 && bounds.max.x >= 1.0);
        assert!(bounds.min.y <= -1.0 && bounds.max.y >= 1.0);
        assert!(bounds.min.z <= 0.0 && bounds.max.z >= 2.0);
    }

    #[test]
    fn quadric_ellipsoid_bounds_match_radii() {
        use grafito_core::{GeoObject, Quadric3DObj};

        // x²/4 + y²/9 + z²/16 = 1 → radios (2, 3, 4).
        let quadric = GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
            0.25,
            1.0 / 9.0,
            1.0 / 16.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            -1.0,
        ]));
        let bounds = fallback_object_bounds(&quadric, &std::collections::HashMap::new())
            .expect("quadric fallback bounds");

        assert!((bounds.min.x + 2.0).abs() < 1.0e-6 && (bounds.max.x - 2.0).abs() < 1.0e-6);
        assert!((bounds.min.y + 3.0).abs() < 1.0e-6 && (bounds.max.y - 3.0).abs() < 1.0e-6);
        assert!((bounds.min.z + 4.0).abs() < 1.0e-6 && (bounds.max.z - 4.0).abs() < 1.0e-6);
    }

    #[test]
    fn picker_hits_prism_and_quadric_via_fallback_bounds() {
        use grafito_core::{GeoObject, Prism3DObj, Quadric3DObj};
        use grafito_geometry::Point3D;

        let camera = test_camera();
        let pointer = egui::vec2(400.0, 300.0);
        let canvas_size = egui::vec2(800.0, 600.0);

        let mut prism_doc = grafito_core::Document::new();
        let prism_id = prism_doc
            .try_add_object(GeoObject::Prism3D(Prism3DObj::new(
                vec![
                    Point3D::new(-1.0, -1.0, 0.0),
                    Point3D::new(1.0, -1.0, 0.0),
                    Point3D::new(1.0, 1.0, 0.0),
                    Point3D::new(-1.0, 1.0, 0.0),
                ],
                Point3D::new(0.0, 0.0, 2.0),
            )))
            .expect("prism fixture");
        assert_eq!(
            pick_3d_object(&prism_doc, &camera, pointer, canvas_size),
            Some(prism_id),
            "prism must be pickable through its fallback AABB"
        );

        let mut quadric_doc = grafito_core::Document::new();
        let quadric_id = quadric_doc
            .try_add_object(GeoObject::Quadric3D(Quadric3DObj::from_coeffs([
                1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0,
            ])))
            .expect("quadric fixture");
        assert_eq!(
            pick_3d_object(&quadric_doc, &camera, pointer, canvas_size),
            Some(quadric_id),
            "quadric must be pickable through its ellipsoid AABB"
        );
    }

    #[test]
    fn quadric_ellipsoid_params_rejects_non_ellipsoid() {
        use grafito_core::Quadric3DObj;

        // Hiperboloide x² + y² - z² = 1: c < 0 → no es elipsoide real.
        let hyperbolic =
            Quadric3DObj::from_coeffs([1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
        assert!(grafito_render::quadric_ellipsoid_params(&hyperbolic).is_none());

        // Elipsoide desplazado: (x-1)²/4 + y² + z² = 1 → centro (1, 0, 0).
        let shifted =
            Quadric3DObj::from_coeffs([0.25, 1.0, 1.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.75]);
        let ellipsoid =
            grafito_render::quadric_ellipsoid_params(&shifted).expect("shifted ellipsoid");
        assert!((ellipsoid.center.x - 1.0).abs() < 1.0e-9);
        assert!((ellipsoid.radii.x - 2.0).abs() < 1.0e-6);
    }
}
