use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

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
            far: 1000.0,
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
        self.distance = (self.distance * factor).clamp(0.5, 200.0);
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

    /// Project a 3D point to normalized device coordinates, then to screen.
    /// Returns (screen_x, screen_y, w) where w is used for clipping.
    pub fn project(&self, p: &Point3D, screen_w: f32, screen_h: f32) -> Option<(f32, f32)> {
        let clip = self.mvp() * p.to_vec3().extend(1.0);
        if clip.w < self.near {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let sx = (ndc_x + 1.0) * 0.5 * screen_w;
        let sy = (1.0 - ndc_y) * 0.5 * screen_h;
        Some((sx, sy))
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
