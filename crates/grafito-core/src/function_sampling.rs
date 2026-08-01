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

use grafito_geometry::expr::eval_integral_batch;

/// Hard limit on cached points emitted by one adaptive explicit-function CPU
/// sampling pass.
///
/// This keeps CPU fallback work bounded. It improves normal high-frequency
/// plots, but it cannot guarantee resolving arbitrary frequencies such as
/// `sin(10000*x)` at every zoom level.
pub const MAX_SAMPLES_PER_FUNCTION: usize = 50_000;

/// Hard limit on expression evaluations performed by one adaptive explicit-function CPU
/// sampling pass.
pub const MAX_EVALUATIONS_PER_FUNCTION: usize = 100_000;

const MIN_BASE_INTERVALS: usize = 10;
const MAX_BASE_INTERVALS: usize = 200;
const MAX_REFINEMENT_DEPTH: u8 = 8;
const MIDPOINT_DEVIATION_RATIO: f64 = 0.01;
const SLOPE_CHANGE_RATIO: f64 = 0.04;
const POLE_MIN_MAGNITUDE: f64 = 8.0;
const CURVATURE_PROBE_FRACTION: f64 = 0.381_966_011_250_105_1;

#[derive(Clone, Copy)]
struct AdaptiveInterval {
    x0: f64,
    y0: Option<f64>,
    x2: f64,
    y2: Option<f64>,
    depth: u8,
}

enum AdaptiveWork {
    Interval(AdaptiveInterval),
    Emit((f64, Option<f64>)),
}

#[derive(Clone, Copy)]
struct RefinementEvidence {
    should_refine: bool,
    likely_pole: bool,
}

fn refinement_evidence(
    x0: f64,
    y0: Option<f64>,
    x1: f64,
    y1: Option<f64>,
    x2: f64,
    y2: Option<f64>,
    probe: Option<(f64, Option<f64>)>,
) -> RefinementEvidence {
    let (Some(y0), Some(y1), Some(y2)) = (y0, y1, y2) else {
        // Refine around invalid values so the explicit None samples isolate a
        // discontinuity instead of leaving a wide ambiguous interval.
        return RefinementEvidence {
            should_refine: true,
            likely_pole: false,
        };
    };

    let span = x2 - x0;
    if !span.is_finite() || span <= 0.0 || x1 <= x0 || x1 >= x2 {
        return RefinementEvidence {
            should_refine: false,
            likely_pole: false,
        };
    }

    let scale = 1.0 + y0.abs().max(y1.abs()).max(y2.abs());
    let midpoint_deviation = (y1 - (y0 + y2) * 0.5).abs();
    let probe_deviation = match probe {
        Some((probe_x, Some(probe_y))) if probe_x > x0 && probe_x < x2 => {
            let t = (probe_x - x0) / span;
            (probe_y - (y0 + (y2 - y0) * t)).abs()
        }
        Some((_, None)) => {
            return RefinementEvidence {
                should_refine: true,
                likely_pole: false,
            };
        }
        _ => 0.0,
    };
    let left_slope = (y1 - y0) / (x1 - x0);
    let right_slope = (y2 - y1) / (x2 - x1);
    let slope_change = (right_slope - left_slope).abs() * span;
    let curved = midpoint_deviation > scale * MIDPOINT_DEVIATION_RATIO
        || probe_deviation > scale * MIDPOINT_DEVIATION_RATIO
        || slope_change > scale * SLOPE_CHANGE_RATIO;

    // A sign flip alone is a normal zero crossing. A pole instead reverses
    // two steep secants and places the midpoint outside the endpoint chord.
    let opposite_signs = y0 * y2 < 0.0;
    let secants_reverse = left_slope * right_slope < 0.0;
    let midpoint_outside_chord = y1 < y0.min(y2) || y1 > y0.max(y2);
    let likely_pole = opposite_signs
        && secants_reverse
        && midpoint_outside_chord
        && y0.abs().min(y2.abs()) > POLE_MIN_MAGNITUDE
        && midpoint_deviation > y0.abs().min(y2.abs()) * 0.5;

    RefinementEvidence {
        should_refine: curved || likely_pole,
        likely_pole,
    }
}

fn adaptive_chunk_samples(
    root: AdaptiveInterval,
    include_first: bool,
    point_budget: usize,
    evaluation_budget: usize,
    eval: &impl Fn(f64) -> Option<f64>,
) -> FunctionSamples {
    let mut samples = Vec::new();
    if include_first {
        samples.push((root.x0, root.y0));
    }

    let mut points_left = point_budget;
    let mut evaluations_left = evaluation_budget;
    let mut work = vec![AdaptiveWork::Interval(root)];

    while let Some(item) = work.pop() {
        match item {
            AdaptiveWork::Emit(sample) => samples.push(sample),
            AdaptiveWork::Interval(interval) => {
                if evaluations_left == 0 {
                    continue;
                }

                let x1 = (interval.x0 + interval.x2) * 0.5;
                if !x1.is_finite() || x1 <= interval.x0 || x1 >= interval.x2 {
                    continue;
                }
                evaluations_left -= 1;
                let y1 = eval(x1);
                let mut evidence = refinement_evidence(
                    interval.x0,
                    interval.y0,
                    x1,
                    y1,
                    interval.x2,
                    interval.y2,
                    None,
                );

                // A midpoint can alias a periodic curve. A deterministic
                // off-centre probe catches that case while remaining bounded
                // by the same per-function evaluation budget.
                if !evidence.should_refine && evaluations_left > 0 {
                    let probe_x =
                        interval.x0 + (interval.x2 - interval.x0) * CURVATURE_PROBE_FRACTION;
                    if probe_x > interval.x0 && probe_x < interval.x2 {
                        evaluations_left -= 1;
                        let probe_y = eval(probe_x);
                        evidence = refinement_evidence(
                            interval.x0,
                            interval.y0,
                            x1,
                            y1,
                            interval.x2,
                            interval.y2,
                            Some((probe_x, probe_y)),
                        );
                    }
                }

                if !evidence.should_refine {
                    continue;
                }

                let at_limit = interval.depth >= MAX_REFINEMENT_DEPTH;
                if evidence.likely_pole && (at_limit || evaluations_left == 0) {
                    if points_left > 0 {
                        points_left -= 1;
                        work.push(AdaptiveWork::Emit((x1, None)));
                    }
                    continue;
                }

                // Keep one output slot available for a later explicit break.
                // This avoids silently reconnecting a pole after a dense area
                // of finite high-frequency samples exhausts the local budget.
                if !at_limit && points_left > 1 {
                    points_left -= 1;
                    work.push(AdaptiveWork::Interval(AdaptiveInterval {
                        x0: x1,
                        y0: y1,
                        x2: interval.x2,
                        y2: interval.y2,
                        depth: interval.depth + 1,
                    }));
                    work.push(AdaptiveWork::Emit((x1, y1)));
                    work.push(AdaptiveWork::Interval(AdaptiveInterval {
                        x0: interval.x0,
                        y0: interval.y0,
                        x2: x1,
                        y2: y1,
                        depth: interval.depth + 1,
                    }));
                } else if (y1.is_none() || evidence.likely_pole) && points_left > 0 {
                    points_left -= 1;
                    work.push(AdaptiveWork::Emit((x1, None)));
                }
            }
        }
    }

    samples.push((root.x2, root.y2));
    samples
}

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
        let cached_key = fun.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if let Some(cached) = cached_key.as_ref() {
            if cached == &key {
                // Verify cached domain contains requested domain.
                if cached.domain.0 <= domain.0 && cached.domain.1 >= domain.1 {
                    return fun.cached_samples.read().unwrap_or_else(|p| {
                        log::warn!("cache lock envenenado; recuperando estado parcial");
                        p.into_inner()
                    });
                }
            }
        }
    }

    let samples = evaluate_function_samples(fun, padded_domain, grid_size, variables);
    *fun.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *fun.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    fun.cached_samples.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
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
        is_integral: fun.is_integral,
        integral_var: fun.integral_var.clone(),
        integral_lower: fun.integral_lower,
    }
}

/// Evaluate a 1D function over a world-space domain.
///
/// Returns a parallel-evaluated list of (x, y) samples. Non-finite values are
/// returned as `None` so the renderer can break the stroke at those points.
/// Finite `f64` values remain available to the CPU path; renderers perform
/// their own view-aware `f32` conversion checks when emitting geometry.
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

    if fun.is_integral {
        let xs = (0..=grid_size).map(|i| min + i as f64 * dx);
        let ys = eval_integral_batch(
            &fun.expr,
            &fun.integral_var,
            fun.integral_lower,
            xs,
            variables,
        );
        return (0..=grid_size)
            .into_par_iter()
            .map(|i| {
                let x = min + i as f64 * dx;
                let y = ys.get(i).copied().flatten().filter(|v| v.is_finite());
                (x, y)
            })
            .collect();
    }

    let parsed_ast = expr::prepare_function_ast(&fun.expr, variables, &["x"]).ok();
    let compiled = parsed_ast
        .is_none()
        .then(|| expr::CompiledExpr::new(&fun.expr, variables).ok())
        .flatten();

    // A syntax error cannot become a discontinuity through more sampling.
    // Preserve a cheap gap for the renderer instead of exhausting the adaptive
    // budget retrying the same unparseable expression at every probe point.
    if parsed_ast.is_none() && compiled.is_none() {
        return vec![(min, None), (max, None)];
    }

    let chunk_count = grid_size.clamp(MIN_BASE_INTERVALS, MAX_BASE_INTERVALS);
    let chunk_dx = (max - min) / chunk_count as f64;
    let point_budget = (MAX_SAMPLES_PER_FUNCTION.saturating_sub(chunk_count + 1)) / chunk_count;
    let evaluation_budget = MAX_EVALUATIONS_PER_FUNCTION
        .saturating_div(chunk_count)
        .saturating_sub(2);
    // The precision mode is thread-local, so capture the caller's choice before
    // evaluating parallel chunks on Rayon workers.
    let high_precision = grafito_geometry::precision::is_high_precision_mode();

    let chunks: Vec<FunctionSamples> = (0..chunk_count)
        .into_par_iter()
        .map(|i| {
            let x0 = min + i as f64 * chunk_dx;
            let x2 = min + (i + 1) as f64 * chunk_dx;

            let eval = |x: f64| -> Option<f64> {
                if high_precision {
                    if let Some(ast) = &parsed_ast {
                        let mut vars_map = std::collections::HashMap::new();
                        vars_map.insert("x".to_string(), grafito_geometry::dd::DD::from_f64(x));
                        let res = ast.eval_dd(&vars_map);
                        let value = res.to_f64();
                        if value.is_finite() {
                            return Some(value);
                        }
                        // A lower-precision fallback can round a DD-invalid
                        // Bessel order to an accepted integer order.
                        if ast.has_invalid_bessel_order_dd(&vars_map) {
                            return None;
                        }
                    }
                }
                if let Some(ast) = &parsed_ast {
                    let res = ast.eval_at("x", x);
                    if res.is_finite() {
                        Some(res)
                    } else {
                        None
                    }
                } else if let Some(c) = &compiled {
                    c.eval_at("x", x).ok().filter(|v| v.is_finite())
                } else {
                    expr::eval_function_with_vars(&fun.expr, x, variables)
                        .ok()
                        .filter(|v| v.is_finite())
                }
            };

            let y0 = eval(x0);
            let y2 = eval(x2);

            adaptive_chunk_samples(
                AdaptiveInterval {
                    x0,
                    y0,
                    x2,
                    y2,
                    depth: 0,
                },
                i == 0,
                point_budget,
                evaluation_budget,
                &eval,
            )
        })
        .collect();

    chunks.into_iter().flatten().collect()
}
