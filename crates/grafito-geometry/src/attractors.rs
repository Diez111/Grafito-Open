use crate::types3d::Point3D;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttractorType {
    Lorenz { sigma: f64, rho: f64, beta: f64 },
    Rossler { a: f64, b: f64, c: f64 },
    Thomas { b: f64 },
    Aizawa { a: f64, b: f64, c: f64, d: f64, e: f64, f: f64 },
    Chen { a: f64, b: f64, c: f64 },
    Halvorsen { a: f64 },
    Dadras { p: f64, q: f64, r: f64, s: f64, e: f64 },
    Chua { alpha: f64, beta: f64, m0: f64, m1: f64 },
    Sprott { a: f64, b: f64 },
    ThreeScroll { a: f64, b: f64, c: f64, d: f64, e: f64, f: f64 },
}

impl AttractorType {
    pub fn lorenz() -> Self { Self::Lorenz { sigma: 10.0, rho: 28.0, beta: 8.0 / 3.0 } }
    pub fn rossler() -> Self { Self::Rossler { a: 0.2, b: 0.2, c: 5.7 } }
    pub fn thomas() -> Self { Self::Thomas { b: 0.208186 } }
    pub fn aizawa() -> Self { Self::Aizawa { a: 0.95, b: 0.7, c: 0.6, d: 3.5, e: 0.25, f: 0.1 } }
    pub fn chen() -> Self { Self::Chen { a: 35.0, b: 3.0, c: 28.0 } }
    pub fn halvorsen() -> Self { Self::Halvorsen { a: 1.89 } }
    pub fn dadras() -> Self { Self::Dadras { p: 3.0, q: 2.7, r: 1.7, s: 2.0, e: 9.0 } }
    pub fn chua() -> Self { Self::Chua { alpha: 15.6, beta: 28.0, m0: -1.143, m1: -0.714 } }
    pub fn sprott() -> Self { Self::Sprott { a: 2.07, b: 1.79 } }
    pub fn three_scroll() -> Self { Self::ThreeScroll { a: 0.4, b: 0.01, c: 0.3, d: 0.4, e: 0.01, f: 0.3 } }
}

fn deriv(attractor: &AttractorType, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    match attractor {
        AttractorType::Lorenz { sigma, rho, beta } => {
            (sigma * (y - x), x * (rho - z) - y, x * y - beta * z)
        }
        AttractorType::Rossler { a, b, c } => {
            (-(y + z), x + a * y, b + z * (x - c))
        }
        AttractorType::Thomas { b } => {
            (y.sin() - b * x, z.sin() - b * y, x.sin() - b * z)
        }
        AttractorType::Aizawa { a, b, c, d, e, f } => {
            let dx = (z - b) * x - d * y;
            let dy = d * x + (z - b) * y;
            let dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + f * z * x * x * x;
            (dx, dy, dz)
        }
        AttractorType::Chen { a, b, c } => {
            (a * (y - x), (c - a) * x - x * z + c * y, x * y - b * z)
        }
        AttractorType::Halvorsen { a } => {
            (-a * x - 4.0 * y - 4.0 * z - y * y,
             -a * y - 4.0 * z - 4.0 * x - z * z,
             -a * z - 4.0 * x - 4.0 * y - x * x)
        }
        AttractorType::Dadras { p, q, r, s, e } => {
            (y - p * x + q * y * z,
             r * y - x * z + z,
             s * x * y - e * z)
        }
        AttractorType::Chua { alpha, beta, m0, m1 } => {
            let fx = if x <= -1.0 {
                m0 * (x + 1.0) - 1.0
            } else if x >= 1.0 {
                m0 * (x - 1.0) + 1.0
            } else {
                m1 * x
            };
            (alpha * (y - x - fx), x - y + z, -beta * y)
        }
        AttractorType::Sprott { a, b } => {
            (y + a * x * y + x * z, 1.0 - b * x * x + y * z, x - x * x - y * y)
        }
        AttractorType::ThreeScroll { a, b, c, d, e, f } => {
            (a * x * (1.0 - x) + b * y * z,
             c * y * (1.0 - y) + d * x * z,
             e * z * (1.0 - z) + f * x * y)
        }
    }
}

pub fn integrate_attractor(
    attractor: &AttractorType,
    x0: f64, y0: f64, z0: f64,
    dt: f64,
    steps: usize,
    skip: usize,
) -> Vec<Point3D> {
    let mut pts = Vec::with_capacity(steps.saturating_sub(skip));
    let mut x = x0;
    let mut y = y0;
    let mut z = z0;
    for i in 0..steps {
        let (k1x, k1y, k1z) = deriv(attractor, x, y, z);
        let (k2x, k2y, k2z) = deriv(attractor, x + 0.5 * dt * k1x, y + 0.5 * dt * k1y, z + 0.5 * dt * k1z);
        let (k3x, k3y, k3z) = deriv(attractor, x + 0.5 * dt * k2x, y + 0.5 * dt * k2y, z + 0.5 * dt * k2z);
        let (k4x, k4y, k4z) = deriv(attractor, x + dt * k3x, y + dt * k3y, z + dt * k3z);
        x += dt / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
        y += dt / 6.0 * (k1y + 2.0 * k2y + 2.0 * k3y + k4y);
        z += dt / 6.0 * (k1z + 2.0 * k2z + 2.0 * k3z + k4z);
        if x.is_nan() || y.is_nan() || z.is_nan() { break; }
        if i >= skip {
            pts.push(Point3D::new(x, y, z));
        }
    }
    pts
}

pub fn default_initial_conditions(attractor: &AttractorType) -> (f64, f64, f64) {
    match attractor {
        AttractorType::Lorenz { .. } => (0.1, 0.0, 0.0),
        AttractorType::Rossler { .. } => (1.0, 1.0, 1.0),
        AttractorType::Thomas { .. } => (1.1, 1.1, -0.01),
        AttractorType::Aizawa { .. } => (0.1, 0.0, 0.0),
        AttractorType::Chen { .. } => (-10.0, 0.0, 37.0),
        AttractorType::Halvorsen { .. } => (-1.48, -1.51, 2.04),
        AttractorType::Dadras { .. } => (0.1, 0.03, 0.0),
        AttractorType::Chua { .. } => (0.7, 0.0, 0.0),
        AttractorType::Sprott { .. } => (0.1, 0.1, 0.1),
        AttractorType::ThreeScroll { .. } => (0.1, 0.1, 0.1),
    }
}

pub fn default_dt(attractor: &AttractorType) -> f64 {
    match attractor {
        AttractorType::Lorenz { .. } => 0.005,
        AttractorType::Rossler { .. } => 0.01,
        AttractorType::Thomas { .. } => 0.03,
        AttractorType::Aizawa { .. } => 0.005,
        AttractorType::Chen { .. } => 0.002,
        AttractorType::Halvorsen { .. } => 0.005,
        AttractorType::Dadras { .. } => 0.005,
        AttractorType::Chua { .. } => 0.01,
        AttractorType::Sprott { .. } => 0.01,
        AttractorType::ThreeScroll { .. } => 0.05,
    }
}

pub fn default_steps(attractor: &AttractorType) -> usize {
    match attractor {
        AttractorType::Lorenz { .. } => 20000,
        AttractorType::Rossler { .. } => 15000,
        AttractorType::Thomas { .. } => 10000,
        AttractorType::Aizawa { .. } => 15000,
        AttractorType::Chen { .. } => 25000,
        AttractorType::Halvorsen { .. } => 15000,
        AttractorType::Dadras { .. } => 15000,
        AttractorType::Chua { .. } => 10000,
        AttractorType::Sprott { .. } => 10000,
        AttractorType::ThreeScroll { .. } => 5000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lorenz_produces_points() {
        let a = AttractorType::lorenz();
        let (x, y, z) = default_initial_conditions(&a);
        let pts = integrate_attractor(&a, x, y, z, default_dt(&a), 1000, 100);
        assert!(pts.len() > 800);
        for p in &pts {
            assert!(!p.x.is_nan() && !p.y.is_nan() && !p.z.is_nan());
        }
    }

    #[test]
    fn test_rossler_produces_points() {
        let a = AttractorType::rossler();
        let (x, y, z) = default_initial_conditions(&a);
        let pts = integrate_attractor(&a, x, y, z, default_dt(&a), 500, 50);
        assert!(pts.len() > 400);
    }

    #[test]
    fn test_thomas_butterfly() {
        let a = AttractorType::thomas();
        let (x, y, z) = default_initial_conditions(&a);
        let pts = integrate_attractor(&a, x, y, z, default_dt(&a), 500, 50);
        assert!(pts.len() > 400);
    }
}
