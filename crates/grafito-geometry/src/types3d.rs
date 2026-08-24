use glam::{DVec3, Mat4, Vec3, Vec4};
use serde::{Deserialize, Serialize};

/// Máxima coordenada de mundo que puede llegar de forma segura al MVP de f32.
pub const MAX_WORLD_COORDINATE: f64 = 1.0e12;

/// 3D point in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3D {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }

    pub fn from_vec3(v: Vec3) -> Self {
        Self::new(v.x as f64, v.y as f64, v.z as f64)
    }

    pub fn to_dvec3(&self) -> DVec3 {
        DVec3::new(self.x, self.y, self.z)
    }

    pub fn from_dvec3(v: DVec3) -> Self {
        Self::new(v.x, v.y, v.z)
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn distance(&self, other: &Point3D) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// 3D line segment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment3D {
    pub a: Point3D,
    pub b: Point3D,
}

impl Segment3D {
    pub fn new(a: Point3D, b: Point3D) -> Self {
        Self { a, b }
    }
}

/// Rayo 3D normalizado y recortado al intervalo visible de la camara.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3D {
    pub origin: Point3D,
    pub direction: Point3D,
    pub min_distance: f64,
    pub max_distance: f64,
}

/// Distancia minima entre un rayo recortado y un segmento.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaySegmentProximity {
    pub distance_along_ray: f64,
    pub separation: f64,
}

impl Ray3D {
    /// Crea un rayo cuya direccion se normaliza. Los limites se expresan en
    /// unidades del mundo desde `origin`.
    pub fn new(
        origin: Point3D,
        direction: Point3D,
        min_distance: f64,
        max_distance: f64,
    ) -> Option<Self> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !min_distance.is_finite()
            || !max_distance.is_finite()
            || min_distance < 0.0
            || max_distance <= min_distance
        {
            return None;
        }
        let direction = direction.to_dvec3();
        let length = direction.length();
        if !length.is_finite() || length <= 1.0e-12 {
            return None;
        }
        Some(Self {
            origin,
            direction: Point3D::from_dvec3(direction / length),
            min_distance,
            max_distance,
        })
    }

    /// Punto situado a una distancia `distance` sobre el rayo visible.
    pub fn point_at(&self, distance: f64) -> Option<Point3D> {
        if !distance.is_finite() || distance < self.min_distance || distance > self.max_distance {
            return None;
        }
        let point = self.origin.to_dvec3() + self.direction.to_dvec3() * distance;
        point.is_finite().then_some(Point3D::from_dvec3(point))
    }

    /// Interseca el rayo con un plano definido por punto y normal.
    pub fn intersect_plane(&self, point: Point3D, normal: Point3D) -> Option<(f64, Point3D)> {
        if !point.is_finite() || !normal.is_finite() {
            return None;
        }
        let normal = normal.to_dvec3();
        let normal_length = normal.length();
        if !normal_length.is_finite() || normal_length <= 1.0e-12 {
            return None;
        }
        let denominator = self.direction.to_dvec3().dot(normal);
        if !denominator.is_finite() || denominator.abs() <= normal_length * 1.0e-12 {
            return None;
        }
        let distance = (point.to_dvec3() - self.origin.to_dvec3()).dot(normal) / denominator;
        let hit = self.point_at(distance)?;
        Some((distance, hit))
    }

    /// Devuelve la primera interseccion visible con una esfera.
    pub fn intersect_sphere(&self, center: Point3D, radius: f64) -> Option<f64> {
        if !center.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        let to_center = center.to_dvec3() - self.origin.to_dvec3();
        let direction = self.direction.to_dvec3();
        let projected_distance = to_center.dot(direction);
        let perpendicular_squared = to_center.cross(direction).length_squared();
        let radius_squared = radius * radius;
        if !projected_distance.is_finite()
            || !perpendicular_squared.is_finite()
            || !radius_squared.is_finite()
            || perpendicular_squared > radius_squared
        {
            return None;
        }
        let half_chord = (radius_squared - perpendicular_squared).max(0.0).sqrt();
        [
            projected_distance - half_chord,
            projected_distance + half_chord,
        ]
        .into_iter()
        .find(|distance| {
            distance.is_finite() && *distance >= self.min_distance && *distance <= self.max_distance
        })
    }

    /// Devuelve la primera interseccion visible con una caja alineada a ejes.
    pub fn intersect_aabb(&self, bounds: Aabb3D) -> Option<f64> {
        let origin = self.origin.to_dvec3();
        let direction = self.direction.to_dvec3();
        let min = bounds.min.to_dvec3();
        let max = bounds.max.to_dvec3();
        let mut entry = self.min_distance;
        let mut exit = self.max_distance;

        for axis in 0..3 {
            let origin_component = origin[axis];
            let direction_component = direction[axis];
            if direction_component.abs() <= 1.0e-12 {
                if origin_component < min[axis] || origin_component > max[axis] {
                    return None;
                }
                continue;
            }
            let inverse = direction_component.recip();
            let mut first = (min[axis] - origin_component) * inverse;
            let mut second = (max[axis] - origin_component) * inverse;
            if first > second {
                std::mem::swap(&mut first, &mut second);
            }
            entry = entry.max(first);
            exit = exit.min(second);
            if !entry.is_finite() || !exit.is_finite() || entry > exit {
                return None;
            }
        }
        Some(entry)
    }

    /// Calcula la proximidad al segmento mas cercana dentro del frustum.
    pub fn closest_to_segment(&self, a: Point3D, b: Point3D) -> Option<RaySegmentProximity> {
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        let ray_start = self.point_at(self.min_distance)?.to_dvec3();
        let ray_end = self.point_at(self.max_distance)?.to_dvec3();
        let segment_start = a.to_dvec3();
        let segment_end = b.to_dvec3();
        let ray_delta = ray_end - ray_start;
        let segment_delta = segment_end - segment_start;
        let between_starts = ray_start - segment_start;
        let ray_length_squared = ray_delta.length_squared();
        let segment_length_squared = segment_delta.length_squared();
        if !ray_length_squared.is_finite() || !segment_length_squared.is_finite() {
            return None;
        }

        let (mut ray_parameter, mut segment_parameter);
        if ray_length_squared <= 1.0e-24 && segment_length_squared <= 1.0e-24 {
            ray_parameter = 0.0;
            segment_parameter = 0.0;
        } else if ray_length_squared <= 1.0e-24 {
            ray_parameter = 0.0;
            segment_parameter =
                (segment_delta.dot(between_starts) / segment_length_squared).clamp(0.0, 1.0);
        } else {
            let ray_projection = ray_delta.dot(between_starts);
            if segment_length_squared <= 1.0e-24 {
                segment_parameter = 0.0;
                ray_parameter = (-ray_projection / ray_length_squared).clamp(0.0, 1.0);
            } else {
                let segment_projection = segment_delta.dot(between_starts);
                let cross_projection = ray_delta.dot(segment_delta);
                let denominator = ray_length_squared * segment_length_squared
                    - cross_projection * cross_projection;
                ray_parameter = if denominator.abs() > 1.0e-24 {
                    ((cross_projection * segment_projection
                        - ray_projection * segment_length_squared)
                        / denominator)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                segment_parameter = (cross_projection * ray_parameter + segment_projection)
                    / segment_length_squared;
                if segment_parameter < 0.0 {
                    segment_parameter = 0.0;
                    ray_parameter = (-ray_projection / ray_length_squared).clamp(0.0, 1.0);
                } else if segment_parameter > 1.0 {
                    segment_parameter = 1.0;
                    ray_parameter =
                        ((cross_projection - ray_projection) / ray_length_squared).clamp(0.0, 1.0);
                }
            }
        }

        let closest_on_ray = ray_start + ray_delta * ray_parameter;
        let closest_on_segment = segment_start + segment_delta * segment_parameter;
        let separation = closest_on_ray.distance(closest_on_segment);
        let distance_along_ray =
            self.min_distance + ray_parameter * (self.max_distance - self.min_distance);
        (separation.is_finite() && distance_along_ray.is_finite()).then_some(RaySegmentProximity {
            distance_along_ray,
            separation,
        })
    }
}

/// Caja 3D alineada con los ejes, usada como volumen de seleccion conservador.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb3D {
    pub min: Point3D,
    pub max: Point3D,
}

impl Aabb3D {
    pub fn new(min: Point3D, max: Point3D) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min.x <= max.x && min.y <= max.y && min.z <= max.z)
            .then_some(Self { min, max })
    }

    pub fn from_points(points: impl IntoIterator<Item = Point3D>) -> Option<Self> {
        let mut points = points.into_iter().filter(Point3D::is_finite);
        let first = points.next()?;
        let (mut min, mut max) = (first, first);
        for point in points {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            min.z = min.z.min(point.z);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            max.z = max.z.max(point.z);
        }
        Self::new(min, max)
    }
}

/// Plano de construccion orientado hacia la camara y anclado en su objetivo.
/// Los ejes permiten reutilizarlo posteriormente para arrastres restringidos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstructionPlane3D {
    pub origin: Point3D,
    pub normal: Point3D,
    pub axis_u: Point3D,
    pub axis_v: Point3D,
}

impl ConstructionPlane3D {
    pub fn intersect_ray(&self, ray: &Ray3D) -> Option<Point3D> {
        ray.intersect_plane(self.origin, self.normal)
            .map(|(_, point)| point)
    }

    pub fn local_coordinates(&self, point: Point3D) -> Option<(f64, f64)> {
        if !point.is_finite() {
            return None;
        }
        let offset = point.to_dvec3() - self.origin.to_dvec3();
        let u = offset.dot(self.axis_u.to_dvec3());
        let v = offset.dot(self.axis_v.to_dvec3());
        (u.is_finite() && v.is_finite()).then_some((u, v))
    }
}

/// 3D sphere (center + radius).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sphere3D {
    pub center: Point3D,
    pub radius: f64,
}

impl Sphere3D {
    pub fn new(center: Point3D, radius: f64) -> Self {
        Self { center, radius }
    }
}

/// Cube defined by center and half-size per axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cube3D {
    pub center: Point3D,
    pub size: f64,
}

impl Cube3D {
    pub fn new(center: Point3D, size: f64) -> Self {
        Self { center, size }
    }

    pub fn vertices(&self) -> [Point3D; 8] {
        let h = self.size * 0.5;
        let x = self.center.x;
        let y = self.center.y;
        let z = self.center.z;
        [
            Point3D::new(x - h, y - h, z - h),
            Point3D::new(x + h, y - h, z - h),
            Point3D::new(x + h, y + h, z - h),
            Point3D::new(x - h, y + h, z - h),
            Point3D::new(x - h, y - h, z + h),
            Point3D::new(x + h, y - h, z + h),
            Point3D::new(x + h, y + h, z + h),
            Point3D::new(x - h, y + h, z + h),
        ]
    }
}

/// Tetraedro regular definido por su centroide y longitud de arista.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tetrahedron3D {
    pub center: Point3D,
    pub edge_length: f64,
}

impl Tetrahedron3D {
    pub fn new(center: Point3D, edge_length: f64) -> Self {
        Self {
            center,
            edge_length,
        }
    }

    /// Devuelve el vértice superior seguido de los tres vértices de la base.
    pub fn vertices(&self) -> [Point3D; 4] {
        let edge = self.edge_length;
        let height = edge * (2.0 / 3.0_f64).sqrt();
        let base_y = self.center.y - height * 0.25;
        let base_z = edge / (2.0 * 3.0_f64.sqrt());
        [
            Point3D::new(self.center.x, self.center.y + height * 0.75, self.center.z),
            Point3D::new(self.center.x - edge * 0.5, base_y, self.center.z - base_z),
            Point3D::new(self.center.x + edge * 0.5, base_y, self.center.z - base_z),
            Point3D::new(self.center.x, base_y, self.center.z + edge / 3.0_f64.sqrt()),
        ]
    }

    /// Triángulos con winding exterior, indexados sobre `vertices()`.
    pub const fn faces(&self) -> [[usize; 3]; 4] {
        [[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]]
    }

    /// Aristas únicas, indexadas sobre `vertices()`.
    pub const fn edges(&self) -> [[usize; 2]; 6] {
        [[0, 1], [0, 2], [0, 3], [1, 2], [2, 3], [3, 1]]
    }

    pub fn volume(&self) -> f64 {
        self.edge_length.powi(3) / (6.0 * 2.0_f64.sqrt())
    }

    /// Indica si todos los vértices derivados son seguros para el renderizador 3D.
    pub fn is_renderable(&self) -> bool {
        self.vertices().into_iter().all(|point| {
            point.is_finite()
                && point.x.abs() <= MAX_WORLD_COORDINATE
                && point.y.abs() <= MAX_WORLD_COORDINATE
                && point.z.abs() <= MAX_WORLD_COORDINATE
        })
    }
}

/// Pyramid: square base centered at `base_center` with `base_size`, apex at `apex`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pyramid3D {
    pub base_center: Point3D,
    pub apex: Point3D,
    pub base_size: f64,
}

impl Pyramid3D {
    pub fn new(base_center: Point3D, apex: Point3D, base_size: f64) -> Self {
        Self {
            base_center,
            apex,
            base_size,
        }
    }

    pub fn base_vertices(&self) -> [Point3D; 4] {
        let h = self.base_size * 0.5;
        let (cx, cy, cz) = (self.base_center.x, self.base_center.y, self.base_center.z);
        [
            Point3D::new(cx - h, cy, cz - h),
            Point3D::new(cx + h, cy, cz - h),
            Point3D::new(cx + h, cy, cz + h),
            Point3D::new(cx - h, cy, cz + h),
        ]
    }
}

/// Cone: circular base centered at `base_center` with `radius`, apex at `apex`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cone3D {
    pub base_center: Point3D,
    pub apex: Point3D,
    pub radius: f64,
}

impl Cone3D {
    pub fn new(base_center: Point3D, apex: Point3D, radius: f64) -> Self {
        Self {
            base_center,
            apex,
            radius,
        }
    }
}

/// Cylinder: centered on axis between base_center and top_center, radius r.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cylinder3D {
    pub base_center: Point3D,
    pub top_center: Point3D,
    pub radius: f64,
}

impl Cylinder3D {
    pub fn new(base_center: Point3D, top_center: Point3D, radius: f64) -> Self {
        Self {
            base_center,
            top_center,
            radius,
        }
    }
}

/// Orbit camera for 3D view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera3D {
    pub theta: f32,    // azimuth angle (radians)
    pub phi: f32,      // elevation angle (radians)
    pub distance: f32, // distance from target
    pub target: Vec3,  // look-at point
    pub fov: f32,      // vertical field of view in degrees
    pub near: f32,
    pub far: f32,
    pub aspect: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            theta: 0.8,
            phi: 0.6,
            distance: 10.0,
            target: Vec3::ZERO,
            fov: 60.0,
            near: 0.1,
            far: 10000.0,
            aspect: 1.6,
        }
    }
}

impl Camera3D {
    pub fn new(aspect: f32) -> Self {
        Self {
            aspect,
            ..Default::default()
        }
    }

    pub fn position(&self) -> Vec3 {
        Vec3::new(
            self.distance * self.phi.cos() * self.theta.cos(),
            self.distance * self.phi.sin(),
            self.distance * self.phi.cos() * self.theta.sin(),
        ) + self.target
    }

    pub fn orbit(&mut self, dtheta: f32, dphi: f32) {
        self.theta -= dtheta;
        self.phi = (self.phi + dphi).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
    }

    pub fn zoom(&mut self, factor: f32) {
        if factor.is_nan() || factor.is_infinite() || factor <= 1e-4 {
            return;
        }
        // Rango amplio y continuo: acercarse 5x más que antes y alejarse
        // 25x más, sin bloquearse, coherente con los planos near/far.
        self.distance = (self.distance * factor).clamp(0.1, 5000.0);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = self.right();
        let up = self.up();
        let scale = self.distance * 0.002;
        self.target -= right * dx * scale;
        self.target -= up * dy * scale;
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.target, self.up())
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov.to_radians(),
            self.aspect.max(0.001),
            self.near,
            self.far,
        )
    }

    pub fn mvp(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Construye un rayo a partir de coordenadas locales al canvas mediante la
    /// inversa de la matriz vista-proyeccion. El intervalo del rayo queda
    /// recortado por los planos `near` y `far` de la camara.
    pub fn screen_ray(
        &self,
        local_x: f32,
        local_y: f32,
        screen_w: f32,
        screen_h: f32,
    ) -> Option<Ray3D> {
        if !local_x.is_finite()
            || !local_y.is_finite()
            || !screen_w.is_finite()
            || !screen_h.is_finite()
            || screen_w <= 0.0
            || screen_h <= 0.0
            || local_x < 0.0
            || local_x > screen_w
            || local_y < 0.0
            || local_y > screen_h
            || !self.theta.is_finite()
            || !self.phi.is_finite()
            || !self.distance.is_finite()
            || self.distance <= 1.0e-6
            || !self.target.is_finite()
            || !self.fov.is_finite()
            || self.fov <= 1.0e-3
            || self.fov >= 179.0
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
        {
            return None;
        }

        let position = self.position();
        if !position.is_finite() {
            return None;
        }
        let aspect = screen_w / screen_h;
        let view = Mat4::look_at_rh(position, self.target, self.up());
        let projection = Mat4::perspective_rh(self.fov.to_radians(), aspect, self.near, self.far);
        let view_projection = projection * view;
        let determinant = view_projection.determinant();
        // Usa tolerancia relativa a la dimensión en lugar de umbral absoluto 1e-12.
        let det_eps = crate::matrices::dimension_relative_epsilon(4, 4);
        if !view_projection.is_finite()
            || !determinant.is_finite()
            || determinant.abs() <= det_eps as f32
        {
            return None;
        }
        let inverse = view_projection.inverse();
        if !inverse.is_finite() {
            return None;
        }

        let ndc_x = local_x.mul_add(2.0 / screen_w, -1.0);
        let ndc_y = 1.0 - local_y * (2.0 / screen_h);
        let unproject = |depth: f32| -> Option<Vec3> {
            let world = inverse * Vec4::new(ndc_x, ndc_y, depth, 1.0);
            if !world.is_finite() || world.w.abs() <= f32::EPSILON {
                return None;
            }
            let point = world.truncate() / world.w;
            point.is_finite().then_some(point)
        };
        let near_point = unproject(0.0)?;
        let far_point = unproject(1.0)?;
        let direction = (far_point - near_point).normalize_or_zero();
        if !direction.is_finite() || direction.length_squared() <= 1.0e-12 {
            return None;
        }
        let min_distance = (near_point - position).dot(direction) as f64;
        let max_distance = (far_point - position).dot(direction) as f64;
        Ray3D::new(
            Point3D::from_vec3(position),
            Point3D::from_vec3(direction),
            min_distance,
            max_distance,
        )
    }

    /// Plano de colocacion perpendicular a la vista y pasante por `target`.
    pub fn construction_plane(&self) -> Option<ConstructionPlane3D> {
        if !self.target.is_finite() || !self.distance.is_finite() || self.distance <= 1.0e-6 {
            return None;
        }
        let position = self.position();
        let normal = (self.target - position).normalize_or_zero();
        let axis_u = self.right();
        let axis_v = axis_u.cross(normal).normalize_or_zero();
        if !position.is_finite()
            || !normal.is_finite()
            || !axis_u.is_finite()
            || !axis_v.is_finite()
            || normal.length_squared() <= 1.0e-12
            || axis_u.length_squared() <= 1.0e-12
            || axis_v.length_squared() <= 1.0e-12
        {
            return None;
        }
        Some(ConstructionPlane3D {
            origin: Point3D::from_vec3(self.target),
            normal: Point3D::from_vec3(normal),
            axis_u: Point3D::from_vec3(axis_u),
            axis_v: Point3D::from_vec3(axis_v),
        })
    }

    pub fn up(&self) -> Vec3 {
        Vec3::Y
    }
    pub fn right(&self) -> Vec3 {
        let forward = self.target - self.position();
        let mut r = forward.cross(self.up());
        if r.length_squared() < 1e-12 {
            r = forward.cross(Vec3::X);
            if r.length_squared() < 1e-12 {
                r = Vec3::X;
            }
        }
        r.normalize()
    }

    /// Project a point inside the camera depth range to screen coordinates.
    pub fn project(&self, p: &Point3D, screen_w: f32, screen_h: f32) -> Option<(f32, f32)> {
        if !p.x.is_finite()
            || !p.y.is_finite()
            || !p.z.is_finite()
            || !screen_w.is_finite()
            || !screen_h.is_finite()
            || screen_w <= 0.0
            || screen_h <= 0.0
            || !self.near.is_finite()
            || self.near <= 0.0
            || !self.far.is_finite()
            || self.far <= self.near
        {
            return None;
        }
        let point = p.to_vec3();
        if !point.is_finite() {
            return None;
        }
        let clip = self.mvp() * point.extend(1.0);
        if !clip.is_finite() {
            return None;
        }
        if clip.w < self.near {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !ndc.is_finite() || !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        let sx = (ndc.x + 1.0) * 0.5 * screen_w;
        let sy = (1.0 - ndc.y) * 0.5 * screen_h;
        (sx.is_finite() && sy.is_finite()).then_some((sx, sy))
    }

    /// Generate circle points in 3D for rendering spheres/cylinders/cones.
    /// Returns points on a circle at `center` in the `u`-`v` plane with `radius`.
    pub fn circle_points(
        center: Vec3,
        u: Vec3,
        v: Vec3,
        radius: f32,
        segments: usize,
    ) -> Vec<Vec3> {
        (0..=segments)
            .map(|i| {
                let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
                center + u * radius * angle.cos() + v * radius * angle.sin()
            })
            .collect()
    }
}

/// Indica si dos muestras consecutivas pueden unirse sin cruzar un salto de curva.
///
/// La cota depende de la distancia de cámara para conservar curvas suaves mientras
/// corta polos finitos que de otro modo producirían un segmento artificial.
pub fn curve_3d_segment_is_continuous(a: Point3D, b: Point3D, camera: &Camera3D) -> bool {
    if !a.x.is_finite()
        || !a.y.is_finite()
        || !a.z.is_finite()
        || !b.x.is_finite()
        || !b.y.is_finite()
        || !b.z.is_finite()
        || !camera.distance.is_finite()
    {
        return false;
    }
    let distance = (b.x - a.x).hypot(b.y - a.y).hypot(b.z - a.z);
    let camera_scale = camera.distance.abs().max(1.0) as f64;
    distance.is_finite() && distance <= (camera_scale * 64.0).max(64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_tetrahedron_derives_centered_outward_faces_and_equal_edges() {
        let center = Point3D::new(1.0, -2.0, 3.0);
        let tetrahedron = Tetrahedron3D::new(center, 2.0);
        let vertices = tetrahedron.vertices();

        assert_eq!(tetrahedron.faces().len(), 4);
        assert_eq!(tetrahedron.edges().len(), 6);
        for [start, end] in tetrahedron.edges() {
            assert!((vertices[start].distance(&vertices[end]) - 2.0).abs() < 1.0e-12);
        }

        let centroid = vertices
            .iter()
            .fold(DVec3::ZERO, |sum, point| sum + point.to_dvec3())
            / vertices.len() as f64;
        assert!((centroid - center.to_dvec3()).length() < 1.0e-12);
        assert!((tetrahedron.volume() - 8.0 / (6.0 * 2.0_f64.sqrt())).abs() < 1.0e-12);
        assert!(tetrahedron.is_renderable());
        assert!(
            !Tetrahedron3D::new(Point3D::new(MAX_WORLD_COORDINATE, 0.0, 0.0), 2.0).is_renderable()
        );

        for [a, b, c] in tetrahedron.faces() {
            let face_center =
                (vertices[a].to_dvec3() + vertices[b].to_dvec3() + vertices[c].to_dvec3()) / 3.0;
            let normal = (vertices[b].to_dvec3() - vertices[a].to_dvec3())
                .cross(vertices[c].to_dvec3() - vertices[a].to_dvec3());
            assert!(normal.dot(face_center - center.to_dvec3()) > 0.0);
        }
    }
}
