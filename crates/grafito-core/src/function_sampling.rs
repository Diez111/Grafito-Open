//! Shared function sampling evaluation and caching support.
//!
//! The heavy per-pixel evaluation is performed once per view/expression change
//! and the resulting (x, y) samples are cached inside [`FunctionObj`]. Both the
//! CPU painter path (`grafito-app`) and the GPU geometry builder path
//! (`grafito-render`) consume the cached samples.

use crate::object::{FunctionCacheKey, FunctionObj, FunctionSamples};
use crate::RenderQuality;
use grafito_geometry::expr;
use rayon::prelude::*;
use std::collections::HashMap;

/// Expand a 1D visible domain by `pad_factor` and snap to a coarse grid so that
/// small pans do not invalidate the cache.
pub fn padded_snapped_domain(domain: (f64, f64), pad_factor: f64, snap_cells: usize) -> (f64, f64) {
    let (min, max) = domain;
    let c = (min + max) * 0.5;
    let half = (max - min) * 0.5 * pad_factor;
    let cells = snap_cells.max(1) as f64;
    // Round the width before computing the snap cell so that tiny floating
    // point differences in equivalent domains do not produce different keys.
    let width = ((max - min) * 1e12).round() / 1e12;
    let cell = width / cells;

    let (new_min, mut new_max) = if cell > 0.0 {
        (
            ((c - half) / cell).floor() * cell,
            ((c + half) / cell).ceil() * cell,
        )
    } else {
        (c - half, c + half)
    };

    // Defensive: ensure a non-degenerate domain.
    if new_min >= new_max {
        new_max = new_min + f64::EPSILON;
    }

    (new_min, new_max)
}

/// Choose a 1D sample count that keeps samples close to screen pixels while
/// avoiding excessive work on high-DPI or huge canvases.
pub fn recommended_grid_size_for_quality(width: f32, quality: RenderQuality) -> usize {
    let base = (width as f64 * 2.0).clamp(1000.0, 10000.0) as usize;
    match quality {
        RenderQuality::Preview => base.min(512),
        RenderQuality::Normal => base.min(2000),
        RenderQuality::High => base,
    }
}

/// Compute or retrieve cached (x, y) samples for a function.
///
/// The cache key covers the expression, padded/snapped domain, grid resolution
/// and document variables. When any of these change the samples are
/// re-evaluated; otherwise the previous samples are returned.
pub fn samples_or_compute<'a>(
    fun: &'a FunctionObj,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> std::sync::RwLockReadGuard<'a, FunctionSamples> {
    let padded_domain = padded_snapped_domain(domain, 2.0, 64);
    let key = cache_key(fun, padded_domain, grid_size, variables);
    {
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| p.into_inner());
        if let Some(cached) = cached_key.as_ref() {
            if cached == &key {
                // Verify cached domain contains requested domain.
                if cached.domain.0 <= domain.0 && cached.domain.1 >= domain.1 {
                    return fun.cached_samples.read().unwrap_or_else(|p| p.into_inner());
                }
            }
        }
    }

    let samples = evaluate_function_samples(fun, padded_domain, grid_size, variables);
    *fun.cached_samples
        .write()
        .unwrap_or_else(|p| p.into_inner()) = samples;
    *fun.cached_key.write().unwrap_or_else(|p| p.into_inner()) = Some(key);
    fun.cached_samples.read().unwrap_or_else(|p| p.into_inner())
}

/// Build a cache key for the given function, domain and variables.
pub fn cache_key(
    fun: &FunctionObj,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> FunctionCacheKey {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for (k, v) in variables.iter() {
        k.hash(&mut hasher);
        v.to_bits().hash(&mut hasher);
    }
    FunctionCacheKey {
        expr: fun.expr.clone(),
        domain,
        grid_size,
        variables_hash: hasher.finish(),
    }
}

/// Evaluate a 1D function over a world-space domain.
///
/// Returns a parallel-evaluated list of (x, y) samples. Values that are
/// non-finite or too large are returned as `None` so the renderer can break
/// the stroke at those points.
fn evaluate_function_samples(
    fun: &FunctionObj,
    domain: (f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
) -> FunctionSamples {
    let (min, max) = domain;
    if grid_size == 0 || !min.is_finite() || !max.is_finite() || min >= max {
        return Vec::new();
    }
    let dx = (max - min) / grid_size as f64;
    let parsed_ast = expr::prepare_function_ast(&fun.expr, variables, &["x"]).ok();

    (0..=grid_size)
        .into_par_iter()
        .map(|i| {
            let x = min + i as f64 * dx;
            let y = if let Some(ast) = &parsed_ast {
                let res = ast.eval_at("x", x);
                if res.is_finite() && res.abs() < 1e6 {
                    Some(res)
                } else {
                    None
                }
            } else {
                expr::eval_function_with_vars(&fun.expr, x, variables)
                    .ok()
                    .filter(|v| v.is_finite() && v.abs() < 1e6)
            };
            (x, y)
        })
        .collect()
}
