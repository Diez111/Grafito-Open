//! Shared parametric curve/surface sampling evaluation and caching support.
//!
//! Parametric curves and surfaces are sampled once per parameter/variable
//! change and the results are cached inside the owning object. Both the CPU
//! painter path (`grafito-app`) and the GPU geometry builder path
//! (`grafito-render`) consume the cached samples.

use crate::object::{
    Curve2DSamples, Curve3DSamples, ParametricCacheKey, ParametricCurve2DObj, ParametricCurve3DObj,
    PolarCurveObj, Surface3DObj, SurfaceCacheKey, SurfaceSamples,
};
use grafito_geometry::expr;
use grafito_geometry::Point3D;
use rayon::prelude::*;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::RwLockReadGuard;

/// Maximum number of samples for curve evaluation.
const MAX_CURVE_STEPS: usize = 4000;

/// Maximum grid resolution for surface evaluation.
const MAX_SURFACE_RES: usize = 128;

/// Hash document variables for use in cache keys.
pub fn variables_hash(variables: &HashMap<String, f64>) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (k, v) in variables.iter() {
        k.hash(&mut hasher);
        v.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn finite_clamp(v: f64) -> f64 {
    if v.is_finite() && v.abs() < 1e6 {
        v
    } else {
        f64::NAN
    }
}

fn resolve_expr(expr: Option<&str>, fallback: f64, variables: &HashMap<String, f64>) -> f64 {
    match expr {
        Some(e) => {
            let vars: Vec<(String, f64)> = variables.iter().map(|(k, v)| (k.clone(), *v)).collect();
            expr::evaluate(e, &vars)
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
        }
        None => fallback,
    }
}

pub fn surface_expr_hash(surf: &Surface3DObj) -> u64 {
    let mut hasher = DefaultHasher::new();
    surf.is_parametric.hash(&mut hasher);
    surf.expr.hash(&mut hasher);
    surf.expr_x.hash(&mut hasher);
    surf.expr_y.hash(&mut hasher);
    surf.expr_z.hash(&mut hasher);
    hasher.finish()
}

fn eval_ast_or_compiled(
    ast: Option<&grafito_geometry::ast::Expr>,
    compiled: Option<&expr::CompiledExpr>,
    vars: &[(String, f64)],
    x_name: &str,
    x: f64,
    y_name: &str,
    y: f64,
) -> f64 {
    ast.map(|a| finite_clamp(a.eval_2d(x_name, x, y_name, y)))
        .or_else(|| compiled.and_then(|c| c.eval(vars).ok()).map(finite_clamp))
        .unwrap_or(f64::NAN)
}

/// Evaluate a 2D parametric curve over its `t` domain.
pub fn evaluate_parametric_curve_2d(
    pc: &ParametricCurve2DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> Curve2DSamples {
    let steps = steps.clamp(1, MAX_CURVE_STEPS);
    let t_min = resolve_expr(pc.t_min_expr.as_deref(), pc.t_min, variables);
    let t_max = resolve_expr(pc.t_max_expr.as_deref(), pc.t_max, variables);
    if !t_min.is_finite() || !t_max.is_finite() || t_min >= t_max {
        return Curve2DSamples::new();
    }
    let dt = (t_max - t_min) / steps as f64;

    let ast_x = expr::prepare_function_ast(&pc.expr_x, variables, &["t"]).ok();
    let ast_y = expr::prepare_function_ast(&pc.expr_y, variables, &["t"]).ok();
    let compiled_x = ast_x
        .is_none()
        .then(|| expr::CompiledExpr::new(&pc.expr_x, variables).ok())
        .flatten();
    let compiled_y = ast_y
        .is_none()
        .then(|| expr::CompiledExpr::new(&pc.expr_y, variables).ok())
        .flatten();

    (0..=steps)
        .into_par_iter()
        .map(|i| {
            let t = t_min + i as f64 * dt;
            let x = ast_x
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_x
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            let y = ast_y
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_y
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            (x, y)
        })
        .collect()
}

/// Evaluate a 3D parametric curve over its `t` domain.
pub fn evaluate_parametric_curve_3d(
    pc: &ParametricCurve3DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> Curve3DSamples {
    let steps = steps.clamp(1, MAX_CURVE_STEPS);
    let t_min = resolve_expr(pc.t_min_expr.as_deref(), pc.t_min, variables);
    let t_max = resolve_expr(pc.t_max_expr.as_deref(), pc.t_max, variables);
    if !t_min.is_finite() || !t_max.is_finite() || t_min >= t_max {
        return Curve3DSamples::new();
    }
    let dt = (t_max - t_min) / steps as f64;

    let ast_x = expr::prepare_function_ast(&pc.expr_x, variables, &["t"]).ok();
    let ast_y = expr::prepare_function_ast(&pc.expr_y, variables, &["t"]).ok();
    let ast_z = expr::prepare_function_ast(&pc.expr_z, variables, &["t"]).ok();
    let compiled_x = ast_x
        .is_none()
        .then(|| expr::CompiledExpr::new(&pc.expr_x, variables).ok())
        .flatten();
    let compiled_y = ast_y
        .is_none()
        .then(|| expr::CompiledExpr::new(&pc.expr_y, variables).ok())
        .flatten();
    let compiled_z = ast_z
        .is_none()
        .then(|| expr::CompiledExpr::new(&pc.expr_z, variables).ok())
        .flatten();

    (0..=steps)
        .into_par_iter()
        .map(|i| {
            let t = t_min + i as f64 * dt;
            let x = ast_x
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_x
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            let y = ast_y
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_y
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            let z = ast_z
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_z
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            (x, y, z)
        })
        .collect()
}

/// Evaluate a polar curve `r(t)` and convert to Cartesian `(x, y)`.
pub fn evaluate_polar_curve(
    pol: &PolarCurveObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> Curve2DSamples {
    let steps = steps.clamp(1, MAX_CURVE_STEPS);
    let t_min = resolve_expr(pol.t_min_expr.as_deref(), pol.t_min, variables);
    let t_max = resolve_expr(pol.t_max_expr.as_deref(), pol.t_max, variables);
    if !t_min.is_finite() || !t_max.is_finite() || t_min >= t_max {
        return Curve2DSamples::new();
    }
    let dt = (t_max - t_min) / steps as f64;

    let ast_r = expr::prepare_function_ast(&pol.expr_r, variables, &["t"]).ok();
    let compiled_r = ast_r
        .is_none()
        .then(|| expr::CompiledExpr::new(&pol.expr_r, variables).ok())
        .flatten();

    (0..=steps)
        .into_par_iter()
        .map(|i| {
            let t = t_min + i as f64 * dt;
            let r = ast_r
                .as_ref()
                .map(|ast| finite_clamp(ast.eval_at("t", t)))
                .or_else(|| {
                    compiled_r
                        .as_ref()
                        .and_then(|c| c.eval_at("t", t).ok().map(finite_clamp))
                })
                .unwrap_or(f64::NAN);
            if r.is_finite() {
                (r * t.cos(), r * t.sin())
            } else {
                (f64::NAN, f64::NAN)
            }
        })
        .collect()
}

/// Evaluate a 3D surface over its domain.
pub fn evaluate_surface_3d(
    surf: &Surface3DObj,
    res: usize,
    variables: &HashMap<String, f64>,
) -> SurfaceSamples {
    let res = res.clamp(1, MAX_SURFACE_RES);
    if surf.is_parametric {
        let u_min = surf.u_min;
        let u_max = surf.u_max;
        let v_min = surf.v_min;
        let v_max = surf.v_max;
        if !u_min.is_finite()
            || !u_max.is_finite()
            || !v_min.is_finite()
            || !v_max.is_finite()
            || u_min >= u_max
            || v_min >= v_max
        {
            return SurfaceSamples::new();
        }
        let du = (u_max - u_min) / res as f64;
        let dv = (v_max - v_min) / res as f64;
        let ast_x = expr::prepare_function_ast(&surf.expr_x, variables, &["u", "v"]).ok();
        let ast_y = expr::prepare_function_ast(&surf.expr_y, variables, &["u", "v"]).ok();
        let ast_z = expr::prepare_function_ast(&surf.expr_z, variables, &["u", "v"]).ok();
        let compiled_x = ast_x
            .is_none()
            .then(|| expr::CompiledExpr::new(&surf.expr_x, variables).ok())
            .flatten();
        let compiled_y = ast_y
            .is_none()
            .then(|| expr::CompiledExpr::new(&surf.expr_y, variables).ok())
            .flatten();
        let compiled_z = ast_z
            .is_none()
            .then(|| expr::CompiledExpr::new(&surf.expr_z, variables).ok())
            .flatten();

        return (0..=res)
            .into_par_iter()
            .map(|i| {
                let u = u_min + i as f64 * du;
                let mut row = Vec::with_capacity(res + 1);
                for j in 0..=res {
                    let v = v_min + j as f64 * dv;
                    let vars = [("u".to_string(), u), ("v".to_string(), v)];
                    let x = eval_ast_or_compiled(
                        ast_x.as_ref(),
                        compiled_x.as_ref(),
                        &vars,
                        "u",
                        u,
                        "v",
                        v,
                    );
                    let y = eval_ast_or_compiled(
                        ast_y.as_ref(),
                        compiled_y.as_ref(),
                        &vars,
                        "u",
                        u,
                        "v",
                        v,
                    );
                    let z = eval_ast_or_compiled(
                        ast_z.as_ref(),
                        compiled_z.as_ref(),
                        &vars,
                        "u",
                        u,
                        "v",
                        v,
                    );
                    row.push(Point3D::new(x, z, y));
                }
                row
            })
            .collect();
    }

    let x_min = resolve_expr(surf.x_min_expr.as_deref(), surf.x_min, variables);
    let x_max = resolve_expr(surf.x_max_expr.as_deref(), surf.x_max, variables);
    let y_min = resolve_expr(surf.y_min_expr.as_deref(), surf.y_min, variables);
    let y_max = resolve_expr(surf.y_max_expr.as_deref(), surf.y_max, variables);
    if !x_min.is_finite()
        || !x_max.is_finite()
        || !y_min.is_finite()
        || !y_max.is_finite()
        || x_min >= x_max
        || y_min >= y_max
    {
        return SurfaceSamples::new();
    }
    let dx = (x_max - x_min) / res as f64;
    let dy = (y_max - y_min) / res as f64;

    // Superficie compleja: z = |f(x + iy)|
    if surf.is_complex {
        let Ok(complex_ast) = grafito_complex::complex_expr::parse(&surf.expr) else {
            return SurfaceSamples::new();
        };
        let base_vars: HashMap<String, num_complex::Complex64> = variables
            .iter()
            .map(|(k, v)| (k.clone(), num_complex::Complex64::new(*v, 0.0)))
            .collect();
        return (0..=res)
            .into_par_iter()
            .map(|i| {
                let x = x_min + i as f64 * dx;
                let mut row = Vec::with_capacity(res + 1);
                for j in 0..=res {
                    let y = y_min + j as f64 * dy;
                    let mut cmap = base_vars.clone();
                    cmap.insert("z".to_string(), num_complex::Complex64::new(x, y));
                    let z = match complex_ast.eval(&cmap) {
                        Ok(fz) if fz.re.is_finite() && fz.im.is_finite() => fz.norm(),
                        _ => 0.0,
                    };
                    row.push(Point3D::new(x, z, y));
                }
                row
            })
            .collect();
    }

    let ast = expr::prepare_function_ast(&surf.expr, variables, &["x", "y"]).ok();
    let compiled = ast
        .is_none()
        .then(|| expr::CompiledExpr::new(&surf.expr, variables).ok())
        .flatten();

    (0..=res)
        .into_par_iter()
        .map(|i| {
            let x = x_min + i as f64 * dx;
            let mut row = Vec::with_capacity(res + 1);
            for j in 0..=res {
                let y = y_min + j as f64 * dy;
                let vars = [("x".to_string(), x), ("y".to_string(), y)];
                let z =
                    eval_ast_or_compiled(ast.as_ref(), compiled.as_ref(), &vars, "x", x, "y", y);
                row.push(Point3D::new(x, z, y));
            }
            row
        })
        .collect()
}

/// Compute or retrieve cached samples for a 2D parametric curve.
pub fn samples_or_compute_curve_2d<'a>(
    pc: &'a ParametricCurve2DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> RwLockReadGuard<'a, Curve2DSamples> {
    let t_min = resolve_expr(pc.t_min_expr.as_deref(), pc.t_min, variables);
    let t_max = resolve_expr(pc.t_max_expr.as_deref(), pc.t_max, variables);
    let key = ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return pc.cached_samples.read().unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            });
        }
    }

    let samples = evaluate_parametric_curve_2d(pc, steps, variables);
    *pc.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    pc.cached_samples.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
}

/// Compute or retrieve cached samples for a 3D parametric curve.
pub fn samples_or_compute_curve_3d<'a>(
    pc: &'a ParametricCurve3DObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> RwLockReadGuard<'a, Curve3DSamples> {
    let t_min = resolve_expr(pc.t_min_expr.as_deref(), pc.t_min, variables);
    let t_max = resolve_expr(pc.t_max_expr.as_deref(), pc.t_max, variables);
    let key = ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: variables_hash(variables),
    };
    {
        let cached_key = pc.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return pc.cached_samples.read().unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            });
        }
    }

    let samples = evaluate_parametric_curve_3d(pc, steps, variables);
    *pc.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pc.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    pc.cached_samples.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
}

/// Compute or retrieve cached samples for a polar curve.
pub fn samples_or_compute_polar<'a>(
    pol: &'a PolarCurveObj,
    steps: usize,
    variables: &HashMap<String, f64>,
) -> RwLockReadGuard<'a, Curve2DSamples> {
    let t_min = resolve_expr(pol.t_min_expr.as_deref(), pol.t_min, variables);
    let t_max = resolve_expr(pol.t_max_expr.as_deref(), pol.t_max, variables);
    let key = ParametricCacheKey {
        t_domain: (t_min, t_max),
        steps,
        variables_hash: variables_hash(variables),
    };
    {
        let cached_key = pol.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return pol.cached_samples.read().unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            });
        }
    }

    let samples = evaluate_polar_curve(pol, steps, variables);
    *pol.cached_samples.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = samples;
    *pol.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    pol.cached_samples.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
}

/// Compute or retrieve cached grid for a 3D parametric surface.
pub fn samples_or_compute_surface<'a>(
    surf: &'a Surface3DObj,
    res: usize,
    variables: &HashMap<String, f64>,
) -> RwLockReadGuard<'a, SurfaceSamples> {
    let (x_min, x_max, y_min, y_max) = if surf.is_parametric {
        (surf.u_min, surf.u_max, surf.v_min, surf.v_max)
    } else {
        (
            resolve_expr(surf.x_min_expr.as_deref(), surf.x_min, variables),
            resolve_expr(surf.x_max_expr.as_deref(), surf.x_max, variables),
            resolve_expr(surf.y_min_expr.as_deref(), surf.y_min, variables),
            resolve_expr(surf.y_max_expr.as_deref(), surf.y_max, variables),
        )
    };
    let key = SurfaceCacheKey {
        x_domain: (x_min, x_max),
        y_domain: (y_min, y_max),
        res,
        is_parametric: surf.is_parametric,
        expr_hash: surface_expr_hash(surf),
        variables_hash: variables_hash(variables),
    };
    {
        let cached_key = surf.cached_key.read().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        });
        if cached_key.as_ref() == Some(&key) {
            return surf.cached_grid.read().unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            });
        }
    }

    let grid = evaluate_surface_3d(surf, res, variables);
    *surf.cached_grid.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = grid;
    *surf.cached_key.write().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    }) = Some(key);
    surf.cached_grid.read().unwrap_or_else(|p| {
        log::warn!("cache lock envenenado; recuperando estado parcial");
        p.into_inner()
    })
}
