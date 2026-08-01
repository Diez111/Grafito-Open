use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, OnceLock};

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

/// Máximo de píxeles que una evaluación de fractal puede materializar.
pub const MAX_FRACTAL_PIXELS: usize = 160_000;
/// Máximo de iteraciones acumuladas que una evaluación de fractal puede ejecutar.
pub const MAX_FRACTAL_WORK_UNITS: usize = 64_000_000;
/// Máximo de iteraciones por píxel aceptado por las rutas interactivas.
pub const MAX_FRACTAL_ITER: u32 = 10_000;

const MAX_CACHED_FRACTAL_PIXELS: usize = MAX_FRACTAL_PIXELS * 2;
const MAX_CACHED_FRACTALS: usize = 4;

/// Error de validación de una petición de fractal antes de reservar o iterar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FractalError {
    InvalidBounds,
    IterationLimitExceeded { requested: u32, maximum: u32 },
    PixelBudgetExceeded { requested: usize, maximum: usize },
    WorkBudgetExceeded { requested: usize, maximum: usize },
}

impl fmt::Display for FractalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBounds => write!(f, "fractal bounds must be finite and ordered"),
            Self::IterationLimitExceeded { requested, maximum } => {
                write!(f, "fractal max_iter {requested} exceeds maximum {maximum}")
            }
            Self::PixelBudgetExceeded { requested, maximum } => write!(
                f,
                "fractal pixel request {requested} exceeds maximum {maximum}"
            ),
            Self::WorkBudgetExceeded { requested, maximum } => write!(
                f,
                "fractal work request {requested} exceeds maximum {maximum}"
            ),
        }
    }
}

impl std::error::Error for FractalError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FractalCacheKind {
    Mandelbrot { max_iter: u32 },
    Julia { cr: u64, ci: u64, max_iter: u32 },
    BurningShip { max_iter: u32 },
    Tricorn { max_iter: u32 },
    Newton { max_iter: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FractalCacheKey {
    kind: FractalCacheKind,
    x_min: u64,
    x_max: u64,
    y_min: u64,
    y_max: u64,
    width: usize,
    height: usize,
}

struct FractalCacheEntry {
    key: FractalCacheKey,
    pixels: Vec<FractalPixel>,
}

fn fractal_cache() -> &'static Mutex<VecDeque<FractalCacheEntry>> {
    static CACHE: OnceLock<Mutex<VecDeque<FractalCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn cache_key(
    fractal: &FractalType,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: usize,
    height: usize,
) -> FractalCacheKey {
    let kind = match fractal {
        FractalType::Mandelbrot { max_iter } => FractalCacheKind::Mandelbrot {
            max_iter: *max_iter,
        },
        FractalType::Julia { cr, ci, max_iter } => FractalCacheKind::Julia {
            cr: cr.to_bits(),
            ci: ci.to_bits(),
            max_iter: *max_iter,
        },
        FractalType::BurningShip { max_iter } => FractalCacheKind::BurningShip {
            max_iter: *max_iter,
        },
        FractalType::Tricorn { max_iter } => FractalCacheKind::Tricorn {
            max_iter: *max_iter,
        },
        FractalType::Newton { max_iter } => FractalCacheKind::Newton {
            max_iter: *max_iter,
        },
    };
    FractalCacheKey {
        kind,
        x_min: x_min.to_bits(),
        x_max: x_max.to_bits(),
        y_min: y_min.to_bits(),
        y_max: y_max.to_bits(),
        width,
        height,
    }
}

fn cached_pixels(key: FractalCacheKey) -> Option<Vec<FractalPixel>> {
    let mut cache = fractal_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let index = cache.iter().position(|entry| entry.key == key)?;
    let entry = cache.remove(index)?;
    let pixels = entry.pixels.clone();
    cache.push_back(entry);
    Some(pixels)
}

fn cache_pixels(key: FractalCacheKey, pixels: Vec<FractalPixel>) {
    if pixels.len() > MAX_CACHED_FRACTAL_PIXELS {
        return;
    }

    let mut cache = fractal_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.iter().any(|entry| entry.key == key) {
        return;
    }

    let mut cached_pixels = cache.iter().map(|entry| entry.pixels.len()).sum::<usize>();
    while !cache.is_empty()
        && (cache.len() >= MAX_CACHED_FRACTALS
            || cached_pixels.saturating_add(pixels.len()) > MAX_CACHED_FRACTAL_PIXELS)
    {
        if let Some(entry) = cache.pop_front() {
            cached_pixels -= entry.pixels.len();
        }
    }
    cache.push_back(FractalCacheEntry { key, pixels });
}

fn max_iter(fractal: &FractalType) -> u32 {
    match fractal {
        FractalType::Mandelbrot { max_iter }
        | FractalType::Julia { max_iter, .. }
        | FractalType::BurningShip { max_iter }
        | FractalType::Tricorn { max_iter }
        | FractalType::Newton { max_iter } => *max_iter,
    }
}

/// Verifica los presupuestos globales de píxeles y trabajo antes de computar.
pub fn validate_fractal_budget(
    width: usize,
    height: usize,
    max_iter: u32,
) -> Result<(), FractalError> {
    if max_iter > MAX_FRACTAL_ITER {
        return Err(FractalError::IterationLimitExceeded {
            requested: max_iter,
            maximum: MAX_FRACTAL_ITER,
        });
    }

    let pixels = width
        .checked_mul(height)
        .ok_or(FractalError::PixelBudgetExceeded {
            requested: usize::MAX,
            maximum: MAX_FRACTAL_PIXELS,
        })?;
    if pixels > MAX_FRACTAL_PIXELS {
        return Err(FractalError::PixelBudgetExceeded {
            requested: pixels,
            maximum: MAX_FRACTAL_PIXELS,
        });
    }

    let work = pixels
        .checked_mul(max_iter as usize)
        .ok_or(FractalError::WorkBudgetExceeded {
            requested: usize::MAX,
            maximum: MAX_FRACTAL_WORK_UNITS,
        })?;
    if work > MAX_FRACTAL_WORK_UNITS {
        return Err(FractalError::WorkBudgetExceeded {
            requested: work,
            maximum: MAX_FRACTAL_WORK_UNITS,
        });
    }
    Ok(())
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

/// Calcula un fractal con límites de recursos y caché LRU acotada para escenas estáticas.
pub fn try_compute_fractal(
    fractal: &FractalType,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: usize,
    height: usize,
) -> Result<Vec<FractalPixel>, FractalError> {
    use rayon::prelude::*;

    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }
    if !x_min.is_finite()
        || !x_max.is_finite()
        || !y_min.is_finite()
        || !y_max.is_finite()
        || x_min >= x_max
        || y_min >= y_max
    {
        return Err(FractalError::InvalidBounds);
    }
    let max_iter = max_iter(fractal);
    validate_fractal_budget(width, height, max_iter)?;

    let key = cache_key(fractal, x_min, x_max, y_min, y_max, width, height);
    if let Some(pixels) = cached_pixels(key) {
        return Ok(pixels);
    }

    let dx = (x_max - x_min) / width as f64;
    let dy = (y_max - y_min) / height as f64;

    let pixels: Vec<_> = (0..height)
        .into_par_iter()
        .flat_map(|j| {
            let y = y_min + j as f64 * dy;
            (0..width)
                .map(move |i| {
                    let x = x_min + i as f64 * dx;
                    let (iter, smooth) = match fractal {
                        FractalType::Mandelbrot { .. } => mandelbrot_iter(x, y, max_iter),
                        FractalType::Julia { cr, ci, .. } => julia_iter(x, y, *cr, *ci, max_iter),
                        FractalType::BurningShip { .. } => burning_ship_iter(x, y, max_iter),
                        FractalType::Tricorn { .. } => tricorn_iter(x, y, max_iter),
                        FractalType::Newton { .. } => newton_iter(x, y, max_iter),
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
        .collect();
    cache_pixels(key, pixels.clone());
    Ok(pixels)
}

/// Compatibilidad para los renderizadores existentes: entradas que exceden el
/// presupuesto producen una imagen vacía en vez de reservar o iterar sin límite.
pub fn compute_fractal(
    fractal: &FractalType,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    width: usize,
    height: usize,
) -> Vec<FractalPixel> {
    try_compute_fractal(fractal, x_min, x_max, y_min, y_max, width, height).unwrap_or_default()
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
    fn fractal_request_limits_pixels_work_and_iterations_before_computation() {
        assert!(validate_fractal_budget(MAX_FRACTAL_PIXELS - 1, 1, 1).is_ok());
        assert!(validate_fractal_budget(MAX_FRACTAL_PIXELS, 1, 1).is_ok());
        assert!(matches!(
            validate_fractal_budget(MAX_FRACTAL_PIXELS + 1, 1, 1),
            Err(FractalError::PixelBudgetExceeded { .. })
        ));

        let max_iter_at_work_limit = (MAX_FRACTAL_WORK_UNITS / MAX_FRACTAL_PIXELS) as u32;
        assert!(validate_fractal_budget(MAX_FRACTAL_PIXELS, 1, max_iter_at_work_limit).is_ok());
        assert!(matches!(
            validate_fractal_budget(MAX_FRACTAL_PIXELS, 1, max_iter_at_work_limit + 1),
            Err(FractalError::WorkBudgetExceeded { .. })
        ));

        for max_iter in [MAX_FRACTAL_ITER - 1, MAX_FRACTAL_ITER] {
            let fractal = FractalType::Mandelbrot { max_iter };
            assert!(try_compute_fractal(&fractal, -0.5, 0.5, -0.5, 0.5, 1, 1).is_ok());
        }
        let excessive = FractalType::Mandelbrot {
            max_iter: MAX_FRACTAL_ITER + 1,
        };
        assert!(matches!(
            try_compute_fractal(&excessive, -0.5, 0.5, -0.5, 0.5, 1, 1),
            Err(FractalError::IterationLimitExceeded { .. })
        ));
    }

    #[test]
    fn identical_valid_requests_are_retained_in_the_bounded_static_cache() {
        let fractal = FractalType::mandelbrot();
        let key = cache_key(&fractal, -0.5, 0.5, -0.5, 0.5, 8, 8);

        let first = try_compute_fractal(&fractal, -0.5, 0.5, -0.5, 0.5, 8, 8).unwrap();
        assert!(cached_pixels(key).is_some());
        let second = try_compute_fractal(&fractal, -0.5, 0.5, -0.5, 0.5, 8, 8).unwrap();

        assert_eq!(first.len(), second.len());
        assert_eq!(first[0].iter, second[0].iter);
    }

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
