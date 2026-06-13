use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// 2D point in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }

    pub fn from_vec2(v: Vec2) -> Self {
        Self::new(v.x as f64, v.y as f64)
    }

    pub fn distance(&self, other: &Point2) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// 2D line (infinite) defined by two points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line2 {
    pub a: Point2,
    pub b: Point2,
}

impl Line2 {
    pub fn new(a: Point2, b: Point2) -> Self {
        Self { a, b }
    }

    pub fn direction(&self) -> Vec2 {
        Vec2::new((self.b.x - self.a.x) as f32, (self.b.y - self.a.y) as f32)
    }

    pub fn length(&self) -> f64 {
        self.a.distance(&self.b)
    }
}

/// Circle defined by center and radius.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub center: Point2,
    pub radius: f64,
}

impl Circle {
    pub fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn contains(&self, p: &Point2) -> bool {
        self.center.distance(p) <= self.radius
    }
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AABB {
    pub min: Point2,
    pub max: Point2,
}

impl AABB {
    pub fn new(min: Point2, max: Point2) -> Self {
        Self { min, max }
    }

    pub fn expand(&mut self, p: &Point2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn contains(&self, p: &Point2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// 2D camera/view transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewTransform {
    pub offset: Point2,
    pub scale: f64,
    pub screen_size: Vec2,
    pub x_log: bool,
    pub y_log: bool,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            offset: Point2::new(0.0, 0.0),
            scale: 50.0,
            screen_size: Vec2::new(800.0, 600.0),
            x_log: false,
            y_log: false,
        }
    }
}

impl ViewTransform {
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            offset: Point2::new(0.0, 0.0),
            scale: 50.0,
            screen_size: Vec2::new(screen_width, screen_height),
            x_log: false,
            y_log: false,
        }
    }

    /// Transform world coordinate to screen using current axis modes
    pub fn world_to_screen(&self, world: Point2) -> Vec2 {
        let ox = self.screen_size.x as f64 * 0.5 + self.offset.x;
        let oy = self.screen_size.y as f64 * 0.5 + self.offset.y;
        let sx = if self.x_log {
            if world.x <= 0.0 {
                ox - 1000.0 * self.scale
            } else {
                ox + world.x.log10() * self.scale
            }
        } else {
            ox + world.x * self.scale
        };
        let sy = if self.y_log {
            if world.y <= 0.0 {
                oy + 1000.0 * self.scale
            } else {
                oy - world.y.log10() * self.scale
            }
        } else {
            oy - world.y * self.scale
        };
        Vec2::new(sx as f32, sy as f32)
    }

    /// Transform screen point to world coordinates
    pub fn screen_to_world(&self, screen: Vec2) -> Point2 {
        let ox = self.screen_size.x as f64 * 0.5 + self.offset.x;
        let oy = self.screen_size.y as f64 * 0.5 + self.offset.y;
        let wx = if self.x_log {
            10_f64.powf((screen.x as f64 - ox) / self.scale)
        } else {
            (screen.x as f64 - ox) / self.scale
        };
        let wy = if self.y_log {
            10_f64.powf((oy - screen.y as f64) / self.scale)
        } else {
            (oy - screen.y as f64) / self.scale
        };
        Point2::new(wx, wy)
    }

    /// Toggle log mode for both axes (log-log, semi-log, or linear)
    pub fn toggle_log_x(&mut self) {
        self.x_log = !self.x_log;
    }
    pub fn toggle_log_y(&mut self) {
        self.y_log = !self.y_log;
    }

    pub fn pan(&mut self, delta_screen: Vec2) {
        self.offset.x += delta_screen.x as f64;
        self.offset.y += delta_screen.y as f64;
    }

    pub fn zoom(&mut self, factor: f32, anchor_screen: Vec2) {
        if factor.is_nan() || factor.is_infinite() || factor <= 1e-4 {
            return;
        }
        let anchor_world = self.screen_to_world(anchor_screen);
        if (self.x_log && anchor_world.x <= 0.0) || (self.y_log && anchor_world.y <= 0.0) {
            return;
        }
        self.scale = (self.scale * factor as f64).clamp(1e-15, 1e15);
        let ox = self.screen_size.x as f64 * 0.5 + self.offset.x;
        let oy = self.screen_size.y as f64 * 0.5 + self.offset.y;
        // Use log-aware coordinate transform, consistent with world_to_screen()
        let sx = if self.x_log {
            ox + anchor_world.x.log10() * self.scale
        } else {
            ox + anchor_world.x * self.scale
        };
        let sy = if self.y_log {
            oy - anchor_world.y.log10() * self.scale
        } else {
            oy - anchor_world.y * self.scale
        };
        self.offset.x += anchor_screen.x as f64 - sx;
        self.offset.y += anchor_screen.y as f64 - sy;
    }

    pub fn projection_matrix(&self) -> Mat4 {
        let half_size = self.screen_size * 0.5;
        Mat4::orthographic_lh(
            -half_size.x,
            half_size.x,
            -half_size.y,
            half_size.y,
            -1.0,
            1.0,
        )
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::from_translation(Vec3::new(self.offset.x as f32, self.offset.y as f32, 0.0))
            * Mat4::from_scale(Vec3::new(self.scale as f32, self.scale as f32, 1.0))
    }

    pub fn mvp_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}

/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const RED: Self = Self::new(0.9, 0.2, 0.2, 1.0);
    pub const GREEN: Self = Self::new(0.2, 0.8, 0.2, 1.0);
    pub const BLUE: Self = Self::new(0.2, 0.4, 0.9, 1.0);
    pub const GRAY: Self = Self::new(0.5, 0.5, 0.5, 1.0);
    pub const LIGHT_GRAY: Self = Self::new(0.85, 0.85, 0.85, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}
