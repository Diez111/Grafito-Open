//! Evaluación y cache de campos vectoriales 2D.
//!
//! Los campos vectoriales se muestrean una vez por cambio de expresión, dominio
//! o variables del documento. Las muestras `(x, y, u, v)` se guardan dentro del
//! objeto [`VectorField2DObj`] para que tanto el pintor CPU (`grafito-app`) como
//! el constructor de geometría GPU (`grafito-render`) las reutilicen.

use crate::object::{VectorField2DObj, VectorFieldCacheKey, VectorFieldSamples};
use grafito_geometry::expr;
use rayon::prelude::*;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::RwLockReadGuard;

/// Expande los límites visibles por `pad_factor` y los alinea a una grilla
/// gruesa para que pequeños desplazamientos no invaliden la caché.
pub fn padded_snapped_bounds(
    view_bounds: (f64, f64, f64, f64),
    pad_factor: f64,
    snap_cells: usize,
) -> (f64, f64, f64, f64) {
    let (vx_min, vx_max, vy_min, vy_max) = view_bounds;
    let cx = (vx_min + vx_max) * 0.5;
    let cy = (vy_min + vy_max) * 0.5;
    let half_w = (vx_max - vx_min) * 0.5 * pad_factor;
    let half_h = (vy_max - vy_min) * 0.5 * pad_factor;

    let cells = snap_cells.max(1) as f64;
    // Redondear el ancho/alto para evitar diferencias de punto flotante.
    let width = ((vx_max - vx_min) * 1e12).round() / 1e12;
    let height = ((vy_max - vy_min) * 1e12).round() / 1e12;
    let cell_x = width / cells;
    let cell_y = height / cells;

    let (x_min, mut x_max) = if cell_x > 0.0 {
        (
            ((cx - half_w) / cell_x).floor() * cell_x,
            ((cx + half_w) / cell_x).ceil() * cell_x,
        )
    } else {
        (cx - half_w, cx + half_w)
    };

    let (y_min, mut y_max) = if cell_y > 0.0 {
        (
            ((cy - half_h) / cell_y).floor() * cell_y,
            ((cy + half_h) / cell_y).ceil() * cell_y,
        )
    } else {
        (cy - half_h, cy + half_h)
    };

    if x_min >= x_max {
        x_max = x_min + f64::EPSILON;
    }
    if y_min >= y_max {
        y_max = y_min + f64::EPSILON;
    }

    (x_min, x_max, y_min, y_max)
}

/// Construye una clave de caché para un campo vectorial.
pub fn cache_key(
    vf: &VectorField2DObj,
    bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> VectorFieldCacheKey {
    let mut hasher = DefaultHasher::new();
    for (k, v) in variables.iter() {
        k.hash(&mut hasher);
        v.to_bits().hash(&mut hasher);
    }
    VectorFieldCacheKey {
        expr_u: vf.expr_u.clone(),
        expr_v: vf.expr_v.clone(),
        view_bounds: bounds,
        grid_size,
        variables_hash: hasher.finish(),
    }
}

/// Evalúa un campo vectorial 2D sobre un dominio rectangular.
///
/// `bounds` es `(x_min, x_max, y_min, y_max)`. El parámetro `_view` se mantiene
/// en la firma por compatibilidad con el resto de evaluadores cacheados.
pub fn evaluate_vector_field_2d(
    vf: &VectorField2DObj,
    bounds: (f64, f64, f64, f64),
    _view: &grafito_geometry::ViewTransform,
    variables: &HashMap<String, f64>,
) -> VectorFieldSamples {
    let (x_min, x_max, y_min, y_max) = bounds;
    if !x_min.is_finite()
        || !x_max.is_finite()
        || !y_min.is_finite()
        || !y_max.is_finite()
        || x_min >= x_max
        || y_min >= y_max
    {
        return Vec::new();
    }

    let grid_size = vf.density.clamp(5, 128);
    let nx = grid_size as f64;
    let ny = grid_size as f64;
    let dx = (x_max - x_min) / nx;
    let dy = (y_max - y_min) / ny;

    let ast_u = expr::prepare_function_ast(&vf.expr_u, variables, &["x", "y"]).ok();
    let ast_v = expr::prepare_function_ast(&vf.expr_v, variables, &["x", "y"]).ok();

    (0..=grid_size)
        .into_par_iter()
        .flat_map(|j| {
            let y = y_min + j as f64 * dy;
            (0..=grid_size)
                .map(|i| {
                    let x = x_min + i as f64 * dx;

                    let u = ast_u
                        .as_ref()
                        .map(|ast| finite_clamp(ast.eval_2d("x", x, "y", y)))
                        .or_else(|| eval_fallback(&vf.expr_u, x, y, variables))
                        .unwrap_or(f64::NAN);

                    let v = ast_v
                        .as_ref()
                        .map(|ast| finite_clamp(ast.eval_2d("x", x, "y", y)))
                        .or_else(|| eval_fallback(&vf.expr_v, x, y, variables))
                        .unwrap_or(f64::NAN);

                    (x, y, u, v)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn finite_clamp(v: f64) -> f64 {
    if v.is_finite() && v.abs() < 1e6 {
        v
    } else {
        f64::NAN
    }
}

fn eval_fallback(expr: &str, x: f64, y: f64, variables: &HashMap<String, f64>) -> Option<f64> {
    let mut vars: Vec<(String, f64)> = variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
    vars.push(("x".to_string(), x));
    vars.push(("y".to_string(), y));
    expr::evaluate(expr, &vars)
        .ok()
        .map(finite_clamp)
        .filter(|v| v.is_finite())
}

/// Obtiene las muestras cacheadas o las computa si cambió la clave.
pub fn samples_or_compute<'a>(
    vf: &'a VectorField2DObj,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> RwLockReadGuard<'a, VectorFieldSamples> {
    let grid_size = grid_size.clamp(5, 128);
    let padded_bounds = padded_snapped_bounds(view_bounds, 2.0, 64);
    let key = cache_key(vf, padded_bounds, grid_size, variables);

    {
        let cached_key = vf.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if cached_key.as_ref() == Some(&key) {
            return vf.cached_samples.read().unwrap_or_else(|p| p.into_inner());
        }
    }

    // Si hay una caché previa que contiene los nuevos límites, reutilizarla.
    {
        let cached_key = vf.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if let Some(cached) = cached_key.as_ref() {
            let (rx_min, rx_max, ry_min, ry_max) = cached.view_bounds;
            let (vx_min, vx_max, vy_min, vy_max) = view_bounds;
            if cached.grid_size == grid_size
                && cached.expr_u == vf.expr_u
                && cached.expr_v == vf.expr_v
                && cached.variables_hash == key.variables_hash
                && vx_min >= rx_min
                && vx_max <= rx_max
                && vy_min >= ry_min
                && vy_max <= ry_max
            {
                return vf.cached_samples.read().unwrap_or_else(|p| p.into_inner());
            }
        }
    }

    let samples = evaluate_vector_field_2d(
        vf,
        padded_bounds,
        &grafito_geometry::ViewTransform::default(),
        variables,
    );
    *vf.cached_samples.write().unwrap_or_else(|p| p.into_inner()) = samples;
    *vf.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    vf.cached_samples.read().unwrap_or_else(|p| p.into_inner())
}
