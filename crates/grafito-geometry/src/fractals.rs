#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FractalType {
    Mandelbrot { max_iter: u32 },
    Julia { cr: f64, ci: f64, max_iter: u32 },
    BurningShip { max_iter: u32 },
    Tricorn { max_iter: u32 },
    Newton { max_iter: u32 },
}

impl FractalType {
    pub fn mandelbrot() -> Self {
        Self::Mandelbrot { max_iter: 256 }
    }
    pub fn julia_dendrite() -> Self {
        Self::Julia {
            cr: -0.70176,
            ci: -0.3842,
            max_iter: 256,
        }
    }
    pub fn julia_siegel() -> Self {
        Self::Julia {
            cr: -0.39054,
            ci: -0.58679,
            max_iter: 256,
        }
    }
    pub fn julia_galaxy() -> Self {
        Self::Julia {
            cr: -0.742,
            ci: 0.1,
            max_iter: 256,
        }
    }
    pub fn burning_ship() -> Self {
        Self::BurningShip { max_iter: 256 }
    }
    pub fn tricorn() -> Self {
        Self::Tricorn { max_iter: 256 }
    }
    pub fn newton() -> Self {
        Self::Newton { max_iter: 64 }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FractalPixel {
    pub x: f64,
    pub y: f64,
    pub iter: u32,
    pub max_iter: u32,
    pub escaped: bool,
    pub smooth_value: f64,
}

fn mandelbrot_iter(cr: f64, ci: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = 0.0f64;
    let mut zi = 0.0f64;
    let mut zr2 = 0.0;
    let mut zi2 = 0.0;
    let mut i = 0u32;
    while i < max_iter && zr2 + zi2 <= 4.0 {
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        zr2 = zr * zr;
        zi2 = zi * zi;
        i += 1;
    }
    let smooth = if i < max_iter {
        let log_zn = (zr2 + zi2).ln() / 2.0;
        let nu = log_zn.ln() / std::f64::consts::LN_2;
        i as f64 + 1.0 - nu
    } else {
        max_iter as f64
    };
    (i, smooth)
}

fn julia_iter(zr0: f64, zi0: f64, cr: f64, ci: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = zr0;
    let mut zi = zi0;
    let mut zr2 = zr * zr;
    let mut zi2 = zi * zi;
    let mut i = 0u32;
    while i < max_iter && zr2 + zi2 <= 4.0 {
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        zr2 = zr * zr;
        zi2 = zi * zi;
        i += 1;
    }
    let smooth = if i < max_iter {
        let log_zn = (zr2 + zi2).ln() / 2.0;
        let nu = log_zn.ln() / std::f64::consts::LN_2;
        i as f64 + 1.0 - nu
    } else {
        max_iter as f64
    };
    (i, smooth)
}

fn burning_ship_iter(cr: f64, ci: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = 0.0f64;
    let mut zi = 0.0f64;
    let mut i = 0u32;
    while i < max_iter && zr * zr + zi * zi <= 4.0 {
        let new_zr = zr * zr - zi * zi + cr;
        zi = (2.0 * zr * zi).abs() + ci;
        zr = new_zr;
        i += 1;
    }
    let smooth = if i < max_iter {
        i as f64
    } else {
        max_iter as f64
    };
    (i, smooth)
}

fn tricorn_iter(cr: f64, ci: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = 0.0f64;
    let mut zi = 0.0f64;
    let mut i = 0u32;
    while i < max_iter && zr * zr + zi * zi <= 4.0 {
        let new_zr = zr * zr - zi * zi + cr;
        zi = -2.0 * zr * zi + ci;
        zr = new_zr;
        i += 1;
    }
    let smooth = if i < max_iter {
        i as f64
    } else {
        max_iter as f64
    };
    (i, smooth)
}

fn newton_iter(zr0: f64, zi0: f64, max_iter: u32) -> (u32, f64) {
    let mut zr = zr0;
    let mut zi = zi0;
    let mut i = 0u32;
    let tol = 1e-6;
    while i < max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        let denom = 3.0 * (zr2 + zi2);
        if denom.abs() < 1e-15 {
            break;
        }
        let fz_r = zr * zr2 - 3.0 * zr * zi2 - 1.0;
        let fz_i = 3.0 * zr2 * zi - zi * zi2;
        let fp_r = 3.0 * (zr2 - zi2);
        let fp_i = 6.0 * zr * zi;
        let fp_mag2 = fp_r * fp_r + fp_i * fp_i;
        if fp_mag2 < 1e-15 {
            break;
        }
        let new_zr = zr - (fz_r * fp_r + fz_i * fp_i) / fp_mag2;
        let new_zi = zi - (fz_i * fp_r - fz_r * fp_i) / fp_mag2;
        if (new_zr - zr).powi(2) + (new_zi - zi).powi(2) < tol * tol {
            return (i, i as f64);
        }
        zr = new_zr;
        zi = new_zi;
        i += 1;
    }
    (i, i as f64)
}

pub fn compute_fractal(
    fractal: &FractalType,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: usize,
    height: usize,
) -> Vec<FractalPixel> {
    use rayon::prelude::*;

    if width == 0 || height == 0 {
        return Vec::new();
    }
    let dx = (x_max - x_min) / width as f64;
    let dy = (y_max - y_min) / height as f64;

    let max_iter = match fractal {
        FractalType::Mandelbrot { max_iter }
        | FractalType::Julia { max_iter, .. }
        | FractalType::BurningShip { max_iter }
        | FractalType::Tricorn { max_iter }
        | FractalType::Newton { max_iter } => *max_iter,
    };

    (0..height)
        .into_par_iter()
        .flat_map(|j| {
            let y = y_min + j as f64 * dy;
            (0..width)
                .map(move |i| {
                    let x = x_min + i as f64 * dx;
                    let (iter, smooth) = match fractal {
                        FractalType::Mandelbrot { max_iter } => mandelbrot_iter(x, y, *max_iter),
                        FractalType::Julia { cr, ci, max_iter } => {
                            julia_iter(x, y, *cr, *ci, *max_iter)
                        }
                        FractalType::BurningShip { max_iter } => burning_ship_iter(x, y, *max_iter),
                        FractalType::Tricorn { max_iter } => tricorn_iter(x, y, *max_iter),
                        FractalType::Newton { max_iter } => newton_iter(x, y, *max_iter),
                    };
                    FractalPixel {
                        x,
                        y,
                        iter,
                        max_iter,
                        escaped: iter < max_iter,
                        smooth_value: smooth,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn fractal_color_hsv(iter: u32, max_iter: u32, smooth: f64) -> (f32, f32, f32, f32) {
    if iter >= max_iter || max_iter == 0 {
        return (0.0, 0.0, 0.0, 1.0);
    }
    let t = smooth / max_iter as f64;
    let h = (t * 360.0 * 4.0) % 360.0;
    let s = 0.85;
    let v = 0.95;
    hsv_to_rgb(h, s, v)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f32, f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    ((r + m) as f32, (g + m) as f32, (b + m) as f32, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mandelbrot_center() {
        let f = FractalType::mandelbrot();
        let pixels = compute_fractal(&f, -0.5, 0.5, -0.5, 0.5, 10, 10);
        assert_eq!(pixels.len(), 100);
        let center = &pixels[55];
        assert!(center.iter > 0);
    }

    #[test]
    fn test_julia_produces_pixels() {
        let f = FractalType::julia_dendrite();
        let pixels = compute_fractal(&f, -2.0, 2.0, -2.0, 2.0, 20, 20);
        assert_eq!(pixels.len(), 400);
        let escaped = pixels.iter().filter(|p| p.escaped).count();
        assert!(escaped > 0);
    }
}
