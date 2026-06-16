//! Análisis matemático de funciones y curvas.
//!
//! Provee rutinas numéricas robustas para encontrar raíces, extremos,
//! puntos de inflexión, interceptos, asíntotas y aproximaciones de Taylor.
//! Las funciones son puras: no dependen de UI ni de estado de documento.

use crate::expr::{eval_batch_1d, eval_batch_2d, eval_function_with_vars};
use crate::Point2;
use std::collections::HashMap;

const DEFAULT_SAMPLES: usize = 800;
const DEFAULT_REFINE_ITER: usize = 30;
const TOL: f64 = 1e-9;
const EPS: f64 = 1e-6;

/// Tipo de característica encontrada por el análisis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisFeature {
    Root,
    YIntercept,
    XIntercept,
    LocalMaximum,
    LocalMinimum,
    Inflection,
    VerticalAsymptote,
    HorizontalAsymptote,
    ObliqueAsymptote,
    Intersection,
    Equilibrium,
    Centroid,
}

impl AnalysisFeature {
    pub fn label(&self) -> &'static str {
        match self {
            AnalysisFeature::Root => "Raíz",
            AnalysisFeature::YIntercept => "Intersección Y",
            AnalysisFeature::XIntercept => "Intersección X",
            AnalysisFeature::LocalMaximum => "Máximo",
            AnalysisFeature::LocalMinimum => "Mínimo",
            AnalysisFeature::Inflection => "Punto de inflexión",
            AnalysisFeature::VerticalAsymptote => "Asíntota vertical",
            AnalysisFeature::HorizontalAsymptote => "Asíntota horizontal",
            AnalysisFeature::ObliqueAsymptote => "Asíntota oblicua",
            AnalysisFeature::Intersection => "Intersección",
            AnalysisFeature::Equilibrium => "Equilibrio",
            AnalysisFeature::Centroid => "Centroide",
        }
    }
}

/// Resultado de una característica de análisis en el plano.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub feature: AnalysisFeature,
    pub point: Point2,
    /// Valor adicional asociado (por ejemplo, la pendiente de una asíntota oblicua).
    pub value: Option<f64>,
    /// Posición secundaria para elementos que no son puntos (extremos de asíntotas).
    pub secondary: Option<Point2>,
    /// Etiqueta descriptiva legible.
    pub label: String,
}

/// Opciones para un análisis de función.
#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub domain_min: f64,
    pub domain_max: f64,
    pub samples: usize,
    pub find_roots: bool,
    pub find_extrema: bool,
    pub find_inflections: bool,
    pub find_y_intercept: bool,
    pub find_asymptotes: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            domain_min: -20.0,
            domain_max: 20.0,
            samples: DEFAULT_SAMPLES,
            find_roots: true,
            find_extrema: true,
            find_inflections: true,
            find_y_intercept: true,
            find_asymptotes: true,
        }
    }
}

/// Analiza una función explícita `y = f(x)`.
pub fn analyze_function(
    expr: &str,
    vars: &HashMap<String, f64>,
    opts: &AnalysisOptions,
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();

    if opts.find_y_intercept && opts.domain_min <= 0.0 && opts.domain_max >= 0.0 {
        if let Ok(y0) = eval_function_with_vars(expr, 0.0, vars) {
            if y0.is_finite() {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::YIntercept,
                    point: Point2::new(0.0, y0),
                    value: Some(y0),
                    secondary: None,
                    label: format!("Intersección Y: (0.00, {:.4})", y0),
                });
            }
        }
    }

    if opts.find_asymptotes {
        results.extend(find_asymptotes(expr, vars, opts));
    }

    let samples = opts.samples.max(4);
    let mut xs = Vec::with_capacity(samples + 1);
    for i in 0..=samples {
        let t = i as f64 / samples as f64;
        let x = opts.domain_min + t * (opts.domain_max - opts.domain_min);
        xs.push(x);
    }
    let ys = match eval_batch_1d(expr, "x", xs.iter().copied(), vars) {
        Ok(batch) => batch
            .into_iter()
            .map(|y| y.filter(|v| v.is_finite()))
            .collect(),
        Err(_) => vec![None; xs.len()],
    };

    if opts.find_roots {
        results.extend(extract_roots(expr, vars, &xs, &ys, opts));
    }

    if opts.find_extrema {
        results.extend(extract_extrema(expr, vars, &xs, &ys, opts));
    }

    if opts.find_inflections {
        results.extend(extract_inflections(expr, vars, &xs, &ys, opts));
    }

    results
}

fn f64_or_nan(expr: &str, x: f64, vars: &HashMap<String, f64>) -> f64 {
    eval_function_with_vars(expr, x, vars).unwrap_or(f64::NAN)
}

fn f64_or_nan_var(expr: &str, var: &str, t: f64, vars: &HashMap<String, f64>) -> f64 {
    eval_batch_1d(expr, var, std::iter::once(t), vars)
        .ok()
        .and_then(|mut res| res.pop().flatten())
        .unwrap_or(f64::NAN)
}

fn finite_or_nan(y: Option<f64>) -> f64 {
    y.filter(|v| v.is_finite()).unwrap_or(f64::NAN)
}

fn derivative_var(expr: &str, var: &str, t: f64, vars: &HashMap<String, f64>) -> f64 {
    let h = (t.abs().max(1.0) * EPS).max(1e-12);
    let f = |t: f64| f64_or_nan_var(expr, var, t, vars);
    let f1 = f(t - 2.0 * h);
    let f2 = f(t - h);
    let f3 = f(t + h);
    let f4 = f(t + 2.0 * h);
    if [f1, f2, f3, f4].iter().all(|v| v.is_finite()) {
        (f1 - 8.0 * f2 + 8.0 * f3 - f4) / (12.0 * h)
    } else {
        f64::NAN
    }
}

fn second_derivative_var(expr: &str, var: &str, t: f64, vars: &HashMap<String, f64>) -> f64 {
    let h = (t.abs().max(1.0) * EPS).max(1e-12);
    let f = |t: f64| f64_or_nan_var(expr, var, t, vars);
    let fm = f(t - h);
    let f0 = f(t);
    let fp = f(t + h);
    if fm.is_finite() && f0.is_finite() && fp.is_finite() {
        (fm - 2.0 * f0 + fp) / (h * h)
    } else {
        f64::NAN
    }
}

fn derivative(expr: &str, x: f64, vars: &HashMap<String, f64>) -> f64 {
    derivative_var(expr, "x", x, vars)
}

fn second_derivative(expr: &str, x: f64, vars: &HashMap<String, f64>) -> f64 {
    second_derivative_var(expr, "x", x, vars)
}

fn newton_refine<F, G>(mut x: f64, f: F, df: G, max_iter: usize) -> Option<f64>
where
    F: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
{
    for _ in 0..max_iter {
        let fx = f(x);
        if fx.abs() < TOL {
            return Some(x);
        }
        let dfx = df(x);
        if dfx.abs() < 1e-15 {
            return None;
        }
        x -= fx / dfx;
    }
    None
}

fn bisect<F: Fn(f64) -> f64>(f: F, mut a: f64, mut b: f64, max_iter: usize) -> Option<f64> {
    let mut fa = f(a);
    let fb = f(b);
    if fa * fb > 0.0 {
        return None;
    }
    for _ in 0..max_iter {
        let mid = (a + b) * 0.5;
        let fm = f(mid);
        if fm.abs() < TOL {
            return Some(mid);
        }
        if fa * fm <= 0.0 {
            b = mid;
        } else {
            a = mid;
            fa = fm;
        }
    }
    Some((a + b) * 0.5)
}

fn extract_roots(
    expr: &str,
    vars: &HashMap<String, f64>,
    xs: &[f64],
    ys: &[Option<f64>],
    _opts: &AnalysisOptions,
) -> Vec<AnalysisResult> {
    let mut roots = Vec::new();
    for i in 1..xs.len() {
        let x0 = xs[i - 1];
        let x1 = xs[i];
        let y0 = finite_or_nan(ys[i - 1]);
        let y1 = finite_or_nan(ys[i]);

        if y1 == 0.0 && y1.is_finite() {
            roots.push(x1);
        } else if (y0.is_finite() && y1.is_finite()) && (y0 * y1 <= 0.0 && (y0 != 0.0 || y1 != 0.0))
        {
            let f = |x: f64| f64_or_nan(expr, x, vars);
            let df = |x: f64| derivative(expr, x, vars);
            let root = newton_refine(x1, f, df, DEFAULT_REFINE_ITER)
                .or_else(|| bisect(f, x0, x1, DEFAULT_REFINE_ITER))
                .unwrap_or(x1);
            roots.push(root);
        }
    }

    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-5);

    roots
        .into_iter()
        .map(|x| AnalysisResult {
            feature: AnalysisFeature::Root,
            point: Point2::new(x, 0.0),
            value: None,
            secondary: None,
            label: format!("Raíz: ({:.4}, 0.00)", x),
        })
        .collect()
}

fn extract_extrema(
    expr: &str,
    vars: &HashMap<String, f64>,
    xs: &[f64],
    _ys: &[Option<f64>],
    _opts: &AnalysisOptions,
) -> Vec<AnalysisResult> {
    let mut extrema = Vec::new();
    let df = |x: f64| derivative(expr, x, vars);

    for i in 1..xs.len() {
        let x0 = xs[i - 1];
        let x1 = xs[i];
        let d0 = df(x0);
        let d1 = df(x1);

        if d0.is_finite() && d1.is_finite() && d0 * d1 <= 0.0 && (d0 != 0.0 || d1 != 0.0) {
            let root = newton_refine(
                x1,
                df,
                |x| second_derivative(expr, x, vars),
                DEFAULT_REFINE_ITER,
            )
            .or_else(|| bisect(df, x0, x1, DEFAULT_REFINE_ITER))
            .unwrap_or(x1);
            if let Ok(y) = eval_function_with_vars(expr, root, vars) {
                if y.is_finite() {
                    let feature = if d0 > 0.0 {
                        AnalysisFeature::LocalMaximum
                    } else {
                        AnalysisFeature::LocalMinimum
                    };
                    extrema.push(AnalysisResult {
                        feature,
                        point: Point2::new(root, y),
                        value: Some(y),
                        secondary: None,
                        label: format!(
                            "{}: ({:.4}, {:.4})",
                            if feature == AnalysisFeature::LocalMaximum {
                                "Máximo"
                            } else {
                                "Mínimo"
                            },
                            root,
                            y
                        ),
                    });
                }
            }
        }
    }

    extrema.sort_by(|a, b| {
        a.point
            .x
            .partial_cmp(&b.point.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    extrema.dedup_by(|a, b| (a.point.x - b.point.x).abs() < 1e-5);
    extrema
}

fn extract_inflections(
    expr: &str,
    vars: &HashMap<String, f64>,
    xs: &[f64],
    _ys: &[Option<f64>],
    _opts: &AnalysisOptions,
) -> Vec<AnalysisResult> {
    let mut inflections = Vec::new();
    let d2f = |x: f64| second_derivative(expr, x, vars);
    let d3f = |x: f64| {
        let h = (x.abs().max(1.0) * EPS).max(1e-12);
        let v0 = d2f(x - 2.0 * h);
        let v1 = d2f(x - h);
        let v2 = d2f(x + h);
        let v3 = d2f(x + 2.0 * h);
        if [v0, v1, v2, v3].iter().all(|v| v.is_finite()) {
            (v0 - 8.0 * v1 + 8.0 * v2 - v3) / (12.0 * h)
        } else {
            f64::NAN
        }
    };

    for i in 1..xs.len() {
        let x0 = xs[i - 1];
        let x1 = xs[i];
        let d0 = d2f(x0);
        let d1 = d2f(x1);

        if d0.is_finite() && d1.is_finite() && d0 * d1 <= 0.0 && (d0 != 0.0 || d1 != 0.0) {
            let root = newton_refine(x1, d2f, d3f, DEFAULT_REFINE_ITER)
                .or_else(|| bisect(d2f, x0, x1, DEFAULT_REFINE_ITER))
                .unwrap_or((x0 + x1) * 0.5);
            if let Ok(y) = eval_function_with_vars(expr, root, vars) {
                if y.is_finite() {
                    inflections.push(AnalysisResult {
                        feature: AnalysisFeature::Inflection,
                        point: Point2::new(root, y),
                        value: Some(y),
                        secondary: None,
                        label: format!("Inflexión: ({:.4}, {:.4})", root, y),
                    });
                }
            }
        }
    }

    inflections.sort_by(|a, b| {
        a.point
            .x
            .partial_cmp(&b.point.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    inflections.dedup_by(|a, b| (a.point.x - b.point.x).abs() < 1e-5);
    inflections
}

fn find_asymptotes(
    expr: &str,
    vars: &HashMap<String, f64>,
    opts: &AnalysisOptions,
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();

    // Asíntotas verticales: buscar discontinuidades infinitas o NaN.
    let samples = opts.samples.max(4);
    let mut prev_x = opts.domain_min;
    let mut prev_finite: Option<f64> = None;
    for i in 0..=samples {
        let x = opts.domain_min + (i as f64 / samples as f64) * (opts.domain_max - opts.domain_min);
        let y = f64_or_nan(expr, x, vars);

        let is_bad = y.is_nan() || y.is_infinite();
        if is_bad {
            // Refinar la ubicación exacta de la asíntota entre prev_x y x.
            let mut lo = prev_x;
            let mut hi = x;
            for _ in 0..40 {
                let mid = (lo + hi) * 0.5;
                let vm = f64_or_nan(expr, mid, vars);
                if vm.is_nan() || vm.is_infinite() {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let asymptote_x = (lo + hi) * 0.5;
            results.push(AnalysisResult {
                feature: AnalysisFeature::VerticalAsymptote,
                point: Point2::new(asymptote_x, 0.0),
                value: None,
                secondary: None,
                label: format!("Asíntota vertical: x = {:.4}", asymptote_x),
            });
            prev_finite = None;
        } else if prev_finite.is_none() && prev_x < x {
            // Saltamos de una región inválida a una válida: la discontinuidad está justo antes.
            let asymptote_x = (prev_x + x) * 0.5;
            results.push(AnalysisResult {
                feature: AnalysisFeature::VerticalAsymptote,
                point: Point2::new(asymptote_x, 0.0),
                value: None,
                secondary: None,
                label: format!("Asíntota vertical: x = {:.4}", asymptote_x),
            });
        }

        if y.is_finite() {
            prev_finite = Some(y);
            prev_x = x;
        }
    }

    // Asíntotas horizontales: límites en ±∞.
    let large = 1e6_f64;
    let y_pos = f64_or_nan(expr, large, vars);
    let y_neg = f64_or_nan(expr, -large, vars);
    if y_pos.is_finite() {
        results.push(AnalysisResult {
            feature: AnalysisFeature::HorizontalAsymptote,
            point: Point2::new(0.0, y_pos),
            value: Some(y_pos),
            secondary: Some(Point2::new(1.0, y_pos)),
            label: format!("Asíntota horizontal: y = {:.4}", y_pos),
        });
    }
    if y_neg.is_finite() && (y_neg - y_pos).abs() > 1e-3 {
        results.push(AnalysisResult {
            feature: AnalysisFeature::HorizontalAsymptote,
            point: Point2::new(0.0, y_neg),
            value: Some(y_neg),
            secondary: Some(Point2::new(1.0, y_neg)),
            label: format!("Asíntota horizontal: y = {:.4}", y_neg),
        });
    }

    results
}

/// Encuentra raíces de una función escalar `g(t)` a partir de muestras uniformes,
/// refinando con Newton/bisección.
fn find_scalar_roots<G, DG>(ts: &[f64], gs: &[Option<f64>], g: G, dg: DG) -> Vec<f64>
where
    G: Fn(f64) -> f64,
    DG: Fn(f64) -> f64,
{
    let mut roots = Vec::new();
    for i in 1..ts.len() {
        let t0 = ts[i - 1];
        let t1 = ts[i];
        let g0 = finite_or_nan(gs[i - 1]);
        let g1 = finite_or_nan(gs[i]);
        if g1 == 0.0 && g1.is_finite() {
            roots.push(t1);
        } else if g0.is_finite() && g1.is_finite() && g0 * g1 <= 0.0 && (g0 != 0.0 || g1 != 0.0) {
            let root = newton_refine(t1, &g, &dg, DEFAULT_REFINE_ITER)
                .or_else(|| bisect(&g, t0, t1, DEFAULT_REFINE_ITER))
                .unwrap_or(t1);
            roots.push(root);
        }
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    roots
}

/// Analiza una curva paramétrica 2D `(x(t), y(t))` en el intervalo `[t_min, t_max]`.
pub fn analyze_parametric_curve2d(
    expr_x: &str,
    expr_y: &str,
    t_min: f64,
    t_max: f64,
    vars: &HashMap<String, f64>,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if t_min >= t_max {
        return results;
    }

    let samples = DEFAULT_SAMPLES;
    let mut ts = Vec::with_capacity(samples + 1);
    for i in 0..=samples {
        let t = t_min + (i as f64 / samples as f64) * (t_max - t_min);
        ts.push(t);
    }

    let xs = eval_batch_1d(expr_x, "t", ts.iter().copied(), vars)
        .unwrap_or_else(|_| vec![None; samples + 1]);
    let ys = eval_batch_1d(expr_y, "t", ts.iter().copied(), vars)
        .unwrap_or_else(|_| vec![None; samples + 1]);

    let eval_x = |t: f64| f64_or_nan_var(expr_x, "t", t, vars);
    let eval_y = |t: f64| f64_or_nan_var(expr_y, "t", t, vars);
    let dx_dt = |t: f64| derivative_var(expr_x, "t", t, vars);
    let dy_dt = |t: f64| derivative_var(expr_y, "t", t, vars);
    let d2x_dt2 = |t: f64| second_derivative_var(expr_x, "t", t, vars);
    let d2y_dt2 = |t: f64| second_derivative_var(expr_y, "t", t, vars);

    if features.contains(&AnalysisFeature::Root) {
        let roots = find_scalar_roots(&ts, &ys, eval_y, dy_dt);
        for t in roots {
            let x = eval_x(t);
            if x.is_finite() {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::Root,
                    point: Point2::new(x, 0.0),
                    value: Some(x),
                    secondary: None,
                    label: format!("Raíz: ({:.4}, 0.00)", x),
                });
            }
        }
    }

    if features.contains(&AnalysisFeature::YIntercept) {
        let roots = find_scalar_roots(&ts, &xs, eval_x, dx_dt);
        for t in roots {
            let y = eval_y(t);
            if y.is_finite() {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::YIntercept,
                    point: Point2::new(0.0, y),
                    value: Some(y),
                    secondary: None,
                    label: format!("Intersección Y: (0.00, {:.4})", y),
                });
            }
        }
    }

    let find_extrema = |expr: &str,
                        coord: &str,
                        coord_name: &str,
                        get_point: &dyn Fn(f64, f64) -> Point2,
                        get_other: &dyn Fn(f64) -> f64| {
        let mut out = Vec::new();
        let d = |t: f64| derivative_var(expr, coord, t, vars);
        let d2 = |t: f64| second_derivative_var(expr, coord, t, vars);
        let values: Vec<Option<f64>> = ts
            .iter()
            .map(|&t| {
                let v = d(t);
                if v.is_finite() {
                    Some(v)
                } else {
                    None
                }
            })
            .collect();
        let roots = find_scalar_roots(&ts, &values, d, d2);
        for t in roots {
            let c = f64_or_nan_var(expr, coord, t, vars);
            let other = get_other(t);
            if c.is_finite() && other.is_finite() {
                let d2v = d2(t);
                let feature = if d2v < 0.0 {
                    AnalysisFeature::LocalMaximum
                } else {
                    AnalysisFeature::LocalMinimum
                };
                out.push(AnalysisResult {
                    feature,
                    point: get_point(c, other),
                    value: Some(c),
                    secondary: None,
                    label: format!(
                        "{} {}: ({:.4}, {:.4})",
                        if feature == AnalysisFeature::LocalMaximum {
                            "Máximo"
                        } else {
                            "Mínimo"
                        },
                        coord_name,
                        get_point(c, other).x,
                        get_point(c, other).y
                    ),
                });
            }
        }
        out
    };

    if features.contains(&AnalysisFeature::LocalMaximum)
        || features.contains(&AnalysisFeature::LocalMinimum)
    {
        results.extend(find_extrema(
            expr_x,
            "t",
            "X",
            &|x, y| Point2::new(x, y),
            &|t| eval_y(t),
        ));
        results.extend(find_extrema(
            expr_y,
            "t",
            "Y",
            &|y, x| Point2::new(x, y),
            &|t| eval_x(t),
        ));
    }

    if features.contains(&AnalysisFeature::Inflection) {
        // Curvatura simplificada: buscamos raíces de d/dt(x' y'' - y' x'').
        let curvature_numerator = |t: f64| {
            let dx = dx_dt(t);
            let dy = dy_dt(t);
            let d2x = d2x_dt2(t);
            let d2y = d2y_dt2(t);
            if [dx, dy, d2x, d2y].iter().all(|v| v.is_finite()) {
                dx * d2y - dy * d2x
            } else {
                f64::NAN
            }
        };
        let dk = |t: f64| {
            let h = (t.abs().max(1.0) * EPS).max(1e-12);
            let v0 = curvature_numerator(t - h);
            let v1 = curvature_numerator(t + h);
            if v0.is_finite() && v1.is_finite() {
                (v1 - v0) / (2.0 * h)
            } else {
                f64::NAN
            }
        };
        let values: Vec<Option<f64>> = ts
            .iter()
            .map(|&t| {
                let v = curvature_numerator(t);
                if v.is_finite() {
                    Some(v)
                } else {
                    None
                }
            })
            .collect();
        let roots = find_scalar_roots(&ts, &values, curvature_numerator, dk);
        for t in roots {
            let x = eval_x(t);
            let y = eval_y(t);
            if x.is_finite() && y.is_finite() {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::Inflection,
                    point: Point2::new(x, y),
                    value: None,
                    secondary: None,
                    label: format!("Inflexión: ({:.4}, {:.4})", x, y),
                });
            }
        }
    }

    results.sort_by(|a, b| {
        a.point
            .x
            .partial_cmp(&b.point.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.dedup_by(|a, b| {
        (a.point.x - b.point.x).abs() < 1e-5 && (a.point.y - b.point.y).abs() < 1e-5
    });
    results
}

/// Analiza una curva polar `r = f(t)` en el intervalo `[t_min, t_max]`.
pub fn analyze_polar_curve(
    expr_r: &str,
    t_min: f64,
    t_max: f64,
    vars: &HashMap<String, f64>,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if t_min >= t_max {
        return results;
    }

    let eval_r = |t: f64| f64_or_nan_var(expr_r, "t", t, vars);
    let dr = |t: f64| derivative_var(expr_r, "t", t, vars);

    let samples = DEFAULT_SAMPLES;
    let mut ts = Vec::with_capacity(samples + 1);
    for i in 0..=samples {
        let t = t_min + (i as f64 / samples as f64) * (t_max - t_min);
        ts.push(t);
    }

    let rs = eval_batch_1d(expr_r, "t", ts.iter().copied(), vars)
        .unwrap_or_else(|_| vec![None; samples + 1]);

    if features.contains(&AnalysisFeature::Root) {
        let roots = find_scalar_roots(&ts, &rs, eval_r, dr);
        for t in roots {
            results.push(AnalysisResult {
                feature: AnalysisFeature::Root,
                point: Point2::new(0.0, 0.0),
                value: Some(t),
                secondary: None,
                label: format!("Raíz polar: t = {:.4}", t),
            });
        }
    }

    if features.contains(&AnalysisFeature::LocalMaximum)
        || features.contains(&AnalysisFeature::LocalMinimum)
    {
        let d_values: Vec<Option<f64>> = ts
            .iter()
            .map(|&t| {
                let v = dr(t);
                if v.is_finite() {
                    Some(v)
                } else {
                    None
                }
            })
            .collect();
        let d2r = |t: f64| second_derivative_var(expr_r, "t", t, vars);
        let roots = find_scalar_roots(&ts, &d_values, dr, d2r);
        for t in roots {
            let r = eval_r(t);
            if r.is_finite() {
                let d2 = d2r(t);
                let feature = if d2 < 0.0 {
                    AnalysisFeature::LocalMaximum
                } else {
                    AnalysisFeature::LocalMinimum
                };
                let x = r * t.cos();
                let y = r * t.sin();
                results.push(AnalysisResult {
                    feature,
                    point: Point2::new(x, y),
                    value: Some(r),
                    secondary: None,
                    label: format!(
                        "{} polar: ({:.4}, {:.4})",
                        if feature == AnalysisFeature::LocalMaximum {
                            "Máximo"
                        } else {
                            "Mínimo"
                        },
                        x,
                        y
                    ),
                });
            }
        }
    }

    results
}

/// Analiza una curva implícita `lhs op rhs` (se evalúa `lhs - rhs`).
pub fn analyze_implicit_curve(
    lhs: &str,
    rhs: &str,
    view_bounds: (f64, f64, f64, f64),
    vars: &HashMap<String, f64>,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    let (xmin, xmax, ymin, ymax) = view_bounds;
    if xmin >= xmax || ymin >= ymax {
        return results;
    }

    let f = format!("({}) - ({})", lhs, rhs);
    let samples = 400;

    if features.contains(&AnalysisFeature::Root) {
        let xs: Vec<f64> = (0..=samples)
            .map(|i| xmin + (i as f64 / samples as f64) * (xmax - xmin))
            .collect();
        let fs =
            eval_batch_2d(&f, "x", "y", xs.iter().map(|&x| (x, 0.0)), vars).unwrap_or_default();
        let g = |x: f64| {
            eval_batch_2d(&f, "x", "y", std::iter::once((x, 0.0)), vars)
                .ok()
                .and_then(|mut v| v.pop().flatten())
                .unwrap_or(f64::NAN)
        };
        let dg = |x: f64| derivative_var(&f, "x", x, vars);
        let roots = find_scalar_roots(&xs, &fs, g, dg);
        for x in roots {
            results.push(AnalysisResult {
                feature: AnalysisFeature::Root,
                point: Point2::new(x, 0.0),
                value: None,
                secondary: None,
                label: format!("Raíz: ({:.4}, 0.00)", x),
            });
        }
    }

    if features.contains(&AnalysisFeature::YIntercept) {
        let ys: Vec<f64> = (0..=samples)
            .map(|i| ymin + (i as f64 / samples as f64) * (ymax - ymin))
            .collect();
        let fs =
            eval_batch_2d(&f, "x", "y", ys.iter().map(|&y| (0.0, y)), vars).unwrap_or_default();
        let g = |y: f64| {
            eval_batch_2d(&f, "x", "y", std::iter::once((0.0, y)), vars)
                .ok()
                .and_then(|mut v| v.pop().flatten())
                .unwrap_or(f64::NAN)
        };
        let dg = |y: f64| derivative_var(&f, "y", y, vars);
        let roots = find_scalar_roots(&ys, &fs, g, dg);
        for y in roots {
            results.push(AnalysisResult {
                feature: AnalysisFeature::YIntercept,
                point: Point2::new(0.0, y),
                value: None,
                secondary: None,
                label: format!("Intersección Y: (0.00, {:.4})", y),
            });
        }
    }

    results
}

/// Analiza un campo vectorial 2D `(u(x,y), v(x,y))` buscando puntos de equilibrio.
pub fn analyze_vector_field2d(
    expr_u: &str,
    expr_v: &str,
    view_bounds: (f64, f64, f64, f64),
    vars: &HashMap<String, f64>,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if !features.contains(&AnalysisFeature::Root) {
        return results;
    }
    let (xmin, xmax, ymin, ymax) = view_bounds;
    if xmin >= xmax || ymin >= ymax {
        return results;
    }

    let grid = 80usize;
    let mut us = vec![vec![f64::NAN; grid + 1]; grid + 1];
    let mut vs = vec![vec![f64::NAN; grid + 1]; grid + 1];
    let mut pts = Vec::with_capacity((grid + 1) * (grid + 1));
    for i in 0..=grid {
        for j in 0..=grid {
            let x = xmin + (i as f64 / grid as f64) * (xmax - xmin);
            let y = ymin + (j as f64 / grid as f64) * (ymax - ymin);
            pts.push((x, y));
        }
    }
    let u_vals = eval_batch_2d(expr_u, "x", "y", pts.clone().into_iter(), vars).unwrap_or_default();
    let v_vals = eval_batch_2d(expr_v, "x", "y", pts.clone().into_iter(), vars).unwrap_or_default();
    for (idx, (_, (u, v))) in pts.iter().zip(u_vals.into_iter().zip(v_vals)).enumerate() {
        if let (Some(u), Some(v)) = (u, v) {
            us[idx / (grid + 1)][idx % (grid + 1)] = u;
            vs[idx / (grid + 1)][idx % (grid + 1)] = v;
        }
    }

    let eval_uv = |x: f64, y: f64| -> (f64, f64) {
        let u = eval_batch_2d(expr_u, "x", "y", std::iter::once((x, y)), vars)
            .ok()
            .and_then(|mut v| v.pop().flatten())
            .unwrap_or(f64::NAN);
        let v = eval_batch_2d(expr_v, "x", "y", std::iter::once((x, y)), vars)
            .ok()
            .and_then(|mut v| v.pop().flatten())
            .unwrap_or(f64::NAN);
        (u, v)
    };

    for i in 0..grid {
        for j in 0..grid {
            let u00 = us[i][j];
            let u10 = us[i + 1][j];
            let u01 = us[i][j + 1];
            let u11 = us[i + 1][j + 1];
            let v00 = vs[i][j];
            let v10 = vs[i + 1][j];
            let v01 = vs[i][j + 1];
            let v11 = vs[i + 1][j + 1];
            if [u00, u10, u01, u11, v00, v10, v01, v11]
                .iter()
                .any(|v| !v.is_finite())
            {
                continue;
            }
            let u_min = u00.min(u10).min(u01).min(u11);
            let u_max = u00.max(u10).max(u01).max(u11);
            let v_min = v00.min(v10).min(v01).min(v11);
            let v_max = v00.max(v10).max(v01).max(v11);
            if u_min * u_max > 0.0 || v_min * v_max > 0.0 {
                continue;
            }
            // Refinamiento simple por bisección en la celda.
            let mut lo_x = xmin + (i as f64 / grid as f64) * (xmax - xmin);
            let mut hi_x = xmin + ((i + 1) as f64 / grid as f64) * (xmax - xmin);
            let mut lo_y = ymin + (j as f64 / grid as f64) * (ymax - ymin);
            let mut hi_y = ymin + ((j + 1) as f64 / grid as f64) * (ymax - ymin);
            let mut best = ((lo_x + hi_x) * 0.5, (lo_y + hi_y) * 0.5, f64::INFINITY);
            for _ in 0..20 {
                let mid_x = (lo_x + hi_x) * 0.5;
                let mid_y = (lo_y + hi_y) * 0.5;
                let (u, v) = eval_uv(mid_x, mid_y);
                let err = u.abs() + v.abs();
                if err < best.2 {
                    best = (mid_x, mid_y, err);
                }
                let (u_left, _) = eval_uv(lo_x, mid_y);
                let (u_right, _) = eval_uv(hi_x, mid_y);
                if u_left.is_finite() && u_right.is_finite() && (u_right - u_left).abs() > 1e-15 {
                    if u_left * u > 0.0 {
                        lo_x = mid_x;
                    } else {
                        hi_x = mid_x;
                    }
                }
                let (_, v_bottom) = eval_uv(mid_x, lo_y);
                let (_, v_top) = eval_uv(mid_x, hi_y);
                if v_bottom.is_finite() && v_top.is_finite() && (v_top - v_bottom).abs() > 1e-15 {
                    if v_bottom * v > 0.0 {
                        lo_y = mid_y;
                    } else {
                        hi_y = mid_y;
                    }
                }
            }
            if best.2 < 1e-2 {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::Root,
                    point: Point2::new(best.0, best.1),
                    value: None,
                    secondary: None,
                    label: format!("Equilibrio: ({:.4}, {:.4})", best.0, best.1),
                });
            }
        }
    }

    results.sort_by(|a, b| {
        a.point
            .x
            .partial_cmp(&b.point.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.dedup_by(|a, b| {
        (a.point.x - b.point.x).abs() < 1e-3 && (a.point.y - b.point.y).abs() < 1e-3
    });
    results
}

/// Calcula los coeficientes del polinomio de Taylor de orden `n` alrededor de `center`.
pub fn taylor_coefficients(
    expr: &str,
    vars: &HashMap<String, f64>,
    center: f64,
    order: usize,
) -> Result<Vec<f64>, String> {
    let mut coeffs = Vec::with_capacity(order + 1);
    let mut f_expr = expr.to_string();
    for k in 0..=order {
        let val = eval_function_with_vars(&f_expr, center, vars)?;
        let fact = (1..=k).fold(1.0, |acc, n| acc * n as f64);
        coeffs.push(val / fact);
        // derivada numérica de la expresión actual
        f_expr = format!("deriv({})", f_expr);
    }
    Ok(coeffs)
}

/// Genera una cadena legible para el polinomio de Taylor.
pub fn taylor_series_string(
    expr: &str,
    vars: &HashMap<String, f64>,
    center: f64,
    order: usize,
) -> Result<String, String> {
    let coeffs = taylor_coefficients(expr, vars, center, order)?;
    let mut terms = Vec::new();
    for (k, c) in coeffs.iter().enumerate() {
        if c.abs() < 1e-12 {
            continue;
        }
        let term = if k == 0 {
            format!("{:.4}", c)
        } else if center == 0.0 {
            format!("{:.4}*x^{}", c, k)
        } else {
            format!("{:.4}*(x-{:.4})^{}", c, center, k)
        };
        terms.push(term);
    }
    if terms.is_empty() {
        return Ok("0".to_string());
    }
    Ok(terms.join(" + "))
}

// ============================================================================
// Análisis polimórfico para tipos de objeto no-Function.
// ============================================================================
//
// Grafito representa cónicas, polígonos, curvas paramétricas, polares,
// implícitas y campos vectoriales con sus propios structs. Las siguientes
// funciones producen `AnalysisResult` directamente desde esos structs sin
// pasar por la representación paramétrica universal. Cada función aplica el
// discriminante analítico de la cónica o un muestreo denso + bisección sobre
// el dominio visible y registra los hallazgos en las features solicitadas.

/// Línea: intersección con los ejes cartesianos.
pub fn analyze_line(
    start: Point2,
    end: Point2,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if dx.abs() < 1e-15 && dy.abs() < 1e-15 {
        return results;
    }

    if features.contains(&AnalysisFeature::YIntercept) {
        // y = 0 => start.y + t*dy = 0
        if dy.abs() > 1e-15 {
            let t = -start.y / dy;
            if (0.0..=1.0).contains(&t) {
                results.push(AnalysisResult {
                    feature: AnalysisFeature::YIntercept,
                    point: Point2::new(0.0, start.y + t * dy),
                    value: Some(0.0),
                    secondary: None,
                    label: format!("Intersección Y: (0.00, {:.4})", start.y + t * dy),
                });
            }
        }
    }

    if (features.contains(&AnalysisFeature::XIntercept)
        || features.contains(&AnalysisFeature::Root))
        && dx.abs() > 1e-15
    {
        let t = -start.x / dx;
        if (0.0..=1.0).contains(&t) {
            let y = start.y + t * dy;
            results.push(AnalysisResult {
                feature: AnalysisFeature::XIntercept,
                point: Point2::new(start.x + t * dx, 0.0),
                value: Some(y),
                secondary: None,
                label: format!("Intersección X: ({:.4}, 0.00)", start.x + t * dx),
            });
        }
    }

    results
}

/// Círculo: cortes con ejes cartesianos (resolviendo `(x-cx)² + (y-cy)² = r²`).
pub fn analyze_circle(
    center: Point2,
    radius: f64,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if radius <= 0.0 {
        return results;
    }
    let r2 = radius * radius;
    let dx2 = r2 - center.y * center.y;
    let dy2 = r2 - center.x * center.x;

    if (features.contains(&AnalysisFeature::XIntercept)
        || features.contains(&AnalysisFeature::Root))
        && dx2 >= 0.0
    {
        let dx = dx2.sqrt();
        results.push(AnalysisResult {
            feature: AnalysisFeature::XIntercept,
            point: Point2::new(center.x - dx, 0.0),
            value: Some(0.0),
            secondary: None,
            label: format!("Intersección X: ({:.4}, 0.00)", center.x - dx),
        });
        if dx > 1e-12 {
            results.push(AnalysisResult {
                feature: AnalysisFeature::XIntercept,
                point: Point2::new(center.x + dx, 0.0),
                value: Some(0.0),
                secondary: None,
                label: format!("Intersección X: ({:.4}, 0.00)", center.x + dx),
            });
        }
    }
    if features.contains(&AnalysisFeature::YIntercept) && dy2 >= 0.0 {
        let dy = dy2.sqrt();
        results.push(AnalysisResult {
            feature: AnalysisFeature::YIntercept,
            point: Point2::new(0.0, center.y - dy),
            value: Some(center.y - dy),
            secondary: None,
            label: format!("Intersección Y: (0.00, {:.4})", center.y - dy),
        });
        if dy > 1e-12 {
            results.push(AnalysisResult {
                feature: AnalysisFeature::YIntercept,
                point: Point2::new(0.0, center.y + dy),
                value: Some(center.y + dy),
                secondary: None,
                label: format!("Intersección Y: (0.00, {:.4})", center.y + dy),
            });
        }
    }

    // Extremos locales del borde: x máximo/mínimo, y máximo/mínimo.
    if features.contains(&AnalysisFeature::LocalMaximum) {
        results.push(AnalysisResult {
            feature: AnalysisFeature::LocalMaximum,
            point: Point2::new(center.x, center.y + radius),
            value: Some(center.y + radius),
            secondary: None,
            label: format!("Máximo Y: ({:.4}, {:.4})", center.x, center.y + radius),
        });
    }
    if features.contains(&AnalysisFeature::LocalMinimum) {
        results.push(AnalysisResult {
            feature: AnalysisFeature::LocalMinimum,
            point: Point2::new(center.x, center.y - radius),
            value: Some(center.y - radius),
            secondary: None,
            label: format!("Mínimo Y: ({:.4}, {:.4})", center.x, center.y - radius),
        });
    }

    results
}

/// Elipse rotada: vértices y cortes con el eje Y. Los cortes exactos con
/// el eje X para una elipse rotada por `angle` requieren resolver la
/// cuadrática implícita; aquí emitimos los vértices canónicos y los cortes
/// cuando la elipse no está rotada, suficientes para los snapshots de UI.
pub fn analyze_ellipse(
    center: Point2,
    rx: f64,
    ry: f64,
    angle: f64,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if rx <= 0.0 || ry <= 0.0 {
        return results;
    }
    let (s, c) = angle.sin_cos();
    let rotate = |p: Point2| Point2::new(p.x * c + p.y * s, -p.x * s + p.y * c);

    if features.contains(&AnalysisFeature::LocalMaximum)
        || features.contains(&AnalysisFeature::LocalMinimum)
    {
        for p in [Point2::new(rx, 0.0), Point2::new(-rx, 0.0)] {
            let r = rotate(p);
            results.push(AnalysisResult {
                feature: AnalysisFeature::LocalMaximum,
                point: Point2::new(center.x + r.x, center.y + r.y),
                value: None,
                secondary: None,
                label: format!("Vértice: ({:.4}, {:.4})", center.x + r.x, center.y + r.y),
            });
        }
        for p in [Point2::new(0.0, ry), Point2::new(0.0, -ry)] {
            let r = rotate(p);
            results.push(AnalysisResult {
                feature: AnalysisFeature::LocalMinimum,
                point: Point2::new(center.x + r.x, center.y + r.y),
                value: None,
                secondary: None,
                label: format!("Vértice: ({:.4}, {:.4})", center.x + r.x, center.y + r.y),
            });
        }
    }

    if features.contains(&AnalysisFeature::YIntercept) && angle.abs() < 1e-9 {
        results.push(AnalysisResult {
            feature: AnalysisFeature::YIntercept,
            point: Point2::new(0.0, center.y + ry),
            value: Some(center.y + ry),
            secondary: None,
            label: format!("Intersección Y: (0.00, {:.4})", center.y + ry),
        });
        results.push(AnalysisResult {
            feature: AnalysisFeature::YIntercept,
            point: Point2::new(0.0, center.y - ry),
            value: Some(center.y - ry),
            secondary: None,
            label: format!("Intersección Y: (0.00, {:.4})", center.y - ry),
        });
    }

    if (features.contains(&AnalysisFeature::XIntercept)
        || features.contains(&AnalysisFeature::Root))
        && angle.abs() < 1e-9
    {
        results.push(AnalysisResult {
            feature: AnalysisFeature::XIntercept,
            point: Point2::new(center.x + rx, 0.0),
            value: Some(0.0),
            secondary: None,
            label: format!("Intersección X: ({:.4}, 0.00)", center.x + rx),
        });
        results.push(AnalysisResult {
            feature: AnalysisFeature::XIntercept,
            point: Point2::new(center.x - rx, 0.0),
            value: Some(0.0),
            secondary: None,
            label: format!("Intersección X: ({:.4}, 0.00)", center.x - rx),
        });
    }

    results
}

/// Parábola: cortes con ejes cartesianos.
pub fn analyze_parabola(
    vertex: Point2,
    p: f64,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if p.abs() < 1e-15 {
        return results;
    }
    // Parábola vertical y^2 = 4p(x - vx) + vy^2 ... simplificamos al caso
    // canónico (vértice en origen, abre hacia arriba): y = x^2 / (4p).
    if features.contains(&AnalysisFeature::XIntercept) || features.contains(&AnalysisFeature::Root)
    {
        // y = 0 => x = 0 en sistema alineado
        if vertex.y.abs() < 1e-12 {
            results.push(AnalysisResult {
                feature: AnalysisFeature::XIntercept,
                point: Point2::new(vertex.x, 0.0),
                value: Some(0.0),
                secondary: None,
                label: format!("Intersección X: ({:.4}, 0.00)", vertex.x),
            });
        }
    }
    if features.contains(&AnalysisFeature::YIntercept) {
        // x = 0 => y = vertex.y
        results.push(AnalysisResult {
            feature: AnalysisFeature::YIntercept,
            point: Point2::new(0.0, vertex.y),
            value: Some(vertex.y),
            secondary: None,
            label: format!("Intersección Y: (0.00, {:.4})", vertex.y),
        });
    }
    if features.contains(&AnalysisFeature::LocalMinimum) && p > 0.0 {
        results.push(AnalysisResult {
            feature: AnalysisFeature::LocalMinimum,
            point: vertex,
            value: Some(vertex.y),
            secondary: None,
            label: format!("Vértice: ({:.4}, {:.4})", vertex.x, vertex.y),
        });
    }
    if features.contains(&AnalysisFeature::LocalMaximum) && p < 0.0 {
        results.push(AnalysisResult {
            feature: AnalysisFeature::LocalMaximum,
            point: vertex,
            value: Some(vertex.y),
            secondary: None,
            label: format!("Vértice: ({:.4}, {:.4})", vertex.x, vertex.y),
        });
    }
    results
}

/// Hipérbola: cortes con ejes cartesianos (asíntotas como oblicuas).
pub fn analyze_hyperbola(
    center: Point2,
    a: f64,
    b: f64,
    horizontal: bool,
    features: &[AnalysisFeature],
) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if a <= 0.0 || b <= 0.0 {
        return results;
    }
    if horizontal {
        // (x-cx)^2/a^2 - (y-cy)^2/b^2 = 1
        // y=0 => (x-cx)^2 = a^2 => x = cx ± a
        if features.contains(&AnalysisFeature::XIntercept)
            || features.contains(&AnalysisFeature::Root)
        {
            results.push(AnalysisResult {
                feature: AnalysisFeature::XIntercept,
                point: Point2::new(center.x + a, 0.0),
                value: Some(0.0),
                secondary: None,
                label: format!("Intersección X: ({:.4}, 0.00)", center.x + a),
            });
            results.push(AnalysisResult {
                feature: AnalysisFeature::XIntercept,
                point: Point2::new(center.x - a, 0.0),
                value: Some(0.0),
                secondary: None,
                label: format!("Intersección X: ({:.4}, 0.00)", center.x - a),
            });
        }
        if features.contains(&AnalysisFeature::YIntercept) {
            // x=0 => -((cy)^2)/b^2 = 1 => sin solución salvo cy imaginario
        }
        if features.contains(&AnalysisFeature::ObliqueAsymptote) {
            // y = ±(b/a) * (x - cx) + cy
            let m = b / a;
            results.push(AnalysisResult {
                feature: AnalysisFeature::ObliqueAsymptote,
                point: Point2::new(center.x, center.y),
                value: Some(m),
                secondary: Some(Point2::new(1.0, m)),
                label: format!(
                    "Asíntota: y = {:.4}·(x - {:.4}) + {:.4}",
                    m, center.x, center.y
                ),
            });
            results.push(AnalysisResult {
                feature: AnalysisFeature::ObliqueAsymptote,
                point: Point2::new(center.x, center.y),
                value: Some(-m),
                secondary: Some(Point2::new(1.0, -m)),
                label: format!(
                    "Asíntota: y = {:.4}·(x - {:.4}) + {:.4}",
                    -m, center.x, center.y
                ),
            });
        }
    } else {
        // (y-cy)^2/a^2 - (x-cx)^2/b^2 = 1
        if features.contains(&AnalysisFeature::YIntercept) {
            results.push(AnalysisResult {
                feature: AnalysisFeature::YIntercept,
                point: Point2::new(0.0, center.y + a),
                value: Some(center.y + a),
                secondary: None,
                label: format!("Intersección Y: (0.00, {:.4})", center.y + a),
            });
            results.push(AnalysisResult {
                feature: AnalysisFeature::YIntercept,
                point: Point2::new(0.0, center.y - a),
                value: Some(center.y - a),
                secondary: None,
                label: format!("Intersección Y: (0.00, {:.4})", center.y - a),
            });
        }
    }
    results
}

/// Polígono: centroide, área y, si la feature X/Y-Intercept está activa, los
/// cruces con los ejes cartesianos (resueltos segmento a segmento).
pub fn analyze_polygon(vertices: &[Point2], features: &[AnalysisFeature]) -> Vec<AnalysisResult> {
    let mut results = Vec::new();
    if vertices.len() < 3 {
        return results;
    }
    let n = vertices.len() as f64;

    if features.contains(&AnalysisFeature::Centroid) {
        let cx = vertices.iter().map(|v| v.x).sum::<f64>() / n;
        let cy = vertices.iter().map(|v| v.y).sum::<f64>() / n;
        // Área firmada para asociarla al centroide.
        let mut area2 = 0.0;
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            area2 += a.x * b.y - b.x * a.y;
        }
        let area = area2.abs() * 0.5;
        results.push(AnalysisResult {
            feature: AnalysisFeature::Centroid,
            point: Point2::new(cx, cy),
            value: Some(area),
            secondary: None,
            label: format!("Centroide: ({:.4}, {:.4})  A = {:.4}", cx, cy, area),
        });
    }

    if features.contains(&AnalysisFeature::XIntercept) || features.contains(&AnalysisFeature::Root)
    {
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            if ((a.y >= 0.0 && b.y >= 0.0) || (a.y <= 0.0 && b.y <= 0.0))
                && (a.y != 0.0 || b.y != 0.0)
            {
                continue;
            }
            let t = -a.y / (b.y - a.y);
            if (0.0..=1.0).contains(&t) {
                let x = a.x + t * (b.x - a.x);
                results.push(AnalysisResult {
                    feature: AnalysisFeature::XIntercept,
                    point: Point2::new(x, 0.0),
                    value: None,
                    secondary: None,
                    label: format!("Intersección X: ({:.4}, 0.00)", x),
                });
            }
        }
    }

    if features.contains(&AnalysisFeature::YIntercept) {
        for i in 0..vertices.len() {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            if ((a.x >= 0.0 && b.x >= 0.0) || (a.x <= 0.0 && b.x <= 0.0))
                && (a.x != 0.0 || b.x != 0.0)
            {
                continue;
            }
            let t = -a.x / (b.x - a.x);
            if (0.0..=1.0).contains(&t) {
                let y = a.y + t * (b.y - a.y);
                results.push(AnalysisResult {
                    feature: AnalysisFeature::YIntercept,
                    point: Point2::new(0.0, y),
                    value: Some(y),
                    secondary: None,
                    label: format!("Intersección Y: (0.00, {:.4})", y),
                });
            }
        }
    }

    results
}

/// Intersecciones entre dos objetos cualesquiera, despachadas por tipo.
///
/// Devuelve los puntos de cruce 2D. Para `Function × Function` reusamos
/// bisección + Newton sobre `f1(x) - f2(x)`. Para parejas con discriminante
/// analítico (línea, cónica) resolvemos el sistema. Para el resto se hace
/// un barrido denso.
pub fn analyze_intersection(
    a: &IntersectionCurve<'_>,
    b: &IntersectionCurve<'_>,
    view_bounds: (f64, f64, f64, f64),
    vars: &HashMap<String, f64>,
) -> Vec<Point2> {
    match (a, b) {
        (IntersectionCurve::Line { s: s1, e: e1 }, IntersectionCurve::Line { s: s2, e: e2 }) => {
            line_line_intersection(*s1, *e1, *s2, *e2)
        }
        (IntersectionCurve::Line { s, e }, IntersectionCurve::Circle { center, radius })
        | (IntersectionCurve::Circle { center, radius }, IntersectionCurve::Line { s, e }) => {
            line_circle_intersection(*s, *e, *center, *radius)
        }
        (
            IntersectionCurve::Circle {
                center: c1,
                radius: r1,
            },
            IntersectionCurve::Circle {
                center: c2,
                radius: r2,
            },
        ) => circle_circle_intersection(*c1, *r1, *c2, *r2),
        (IntersectionCurve::Function { expr }, IntersectionCurve::Line { s, e })
        | (IntersectionCurve::Line { s, e }, IntersectionCurve::Function { expr }) => {
            function_line_intersection(expr, *s, *e, view_bounds, vars)
        }
        (IntersectionCurve::Function { expr: e1 }, IntersectionCurve::Function { expr: e2 }) => {
            function_function_intersection(e1, e2, view_bounds, vars)
        }
        _ => Vec::new(),
    }
}

/// Curva representable por una de las fórmulas usadas en `analyze_intersection`.
/// Se mantiene minimal: solo los tipos con discriminante analítico.
#[derive(Debug, Clone)]
pub enum IntersectionCurve<'a> {
    Line { s: Point2, e: Point2 },
    Circle { center: Point2, radius: f64 },
    Function { expr: &'a str },
}

fn line_line_intersection(s1: Point2, e1: Point2, s2: Point2, e2: Point2) -> Vec<Point2> {
    let d1x = e1.x - s1.x;
    let d1y = e1.y - s1.y;
    let d2x = e2.x - s2.x;
    let d2y = e2.y - s2.y;
    let denom = d1x * d2y - d1y * d2x;
    if denom.abs() < 1e-15 {
        return Vec::new();
    }
    let dx = s2.x - s1.x;
    let dy = s2.y - s1.y;
    let t = (dx * d2y - dy * d2x) / denom;
    vec![Point2::new(s1.x + t * d1x, s1.y + t * d1y)]
}

fn line_circle_intersection(s: Point2, e: Point2, center: Point2, radius: f64) -> Vec<Point2> {
    let dx = e.x - s.x;
    let dy = e.y - s.y;
    let fx = s.x - center.x;
    let fy = s.y - center.y;
    let a = dx * dx + dy * dy;
    let b = 2.0 * (fx * dx + fy * dy);
    let c = fx * fx + fy * fy - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 || a.abs() < 1e-15 {
        return Vec::new();
    }
    let sd = disc.sqrt();
    let t1 = (-b - sd) / (2.0 * a);
    let t2 = (-b + sd) / (2.0 * a);
    let mut out = Vec::new();
    for t in [t1, t2] {
        if t.is_finite() {
            out.push(Point2::new(s.x + t * dx, s.y + t * dy));
        }
    }
    out
}

fn circle_circle_intersection(c1: Point2, r1: f64, c2: Point2, r2: f64) -> Vec<Point2> {
    let d = c1.distance(&c2);
    if d < 1e-15 || d > r1 + r2 || d < (r1 - r2).abs() {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h2 = r1 * r1 - a * a;
    if h2 < 0.0 {
        return Vec::new();
    }
    let h = h2.sqrt();
    let px = c1.x + a * (c2.x - c1.x) / d;
    let py = c1.y + a * (c2.y - c1.y) / d;
    if h < 1e-12 {
        return vec![Point2::new(px, py)];
    }
    vec![
        Point2::new(px + h * (c2.y - c1.y) / d, py - h * (c2.x - c1.x) / d),
        Point2::new(px - h * (c2.y - c1.y) / d, py + h * (c2.x - c1.x) / d),
    ]
}

fn function_function_intersection(
    expr_a: &str,
    expr_b: &str,
    view_bounds: (f64, f64, f64, f64),
    vars: &HashMap<String, f64>,
) -> Vec<Point2> {
    let (xmin, xmax, _, _) = view_bounds;
    let f = |x: f64| -> f64 {
        let ya = eval_function_with_vars(expr_a, x, vars).unwrap_or(f64::NAN);
        let yb = eval_function_with_vars(expr_b, x, vars).unwrap_or(f64::NAN);
        if ya.is_finite() && yb.is_finite() {
            ya - yb
        } else {
            f64::NAN
        }
    };
    let df = |x: f64| -> f64 {
        let h = (x.abs().max(1.0) * EPS).max(1e-12);
        (f(x + h) - f(x - h)) / (2.0 * h)
    };
    let n = 400;
    let xs: Vec<f64> = (0..=n)
        .map(|i| xmin + (i as f64 / n as f64) * (xmax - xmin))
        .collect();
    let yas =
        eval_batch_1d(expr_a, "x", xs.iter().copied(), vars).unwrap_or_else(|_| vec![None; n + 1]);
    let ybs =
        eval_batch_1d(expr_b, "x", xs.iter().copied(), vars).unwrap_or_else(|_| vec![None; n + 1]);

    let get_f = |i: usize| -> f64 {
        if let (Some(a), Some(b)) = (yas[i], ybs[i]) {
            if a.is_finite() && b.is_finite() {
                a - b
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        }
    };

    let mut out = Vec::new();
    let mut prev_x = xs[0];
    let mut prev_f = get_f(0);
    for (i, x) in xs.iter().enumerate().take(n + 1).skip(1) {
        let x = *x;
        let fx = get_f(i);
        if !fx.is_finite() {
            prev_x = x;
            prev_f = fx;
            continue;
        }
        if prev_f.is_finite() && prev_f * fx <= 0.0 && (prev_f != 0.0 || fx != 0.0) {
            let root = newton_refine(x, f, df, DEFAULT_REFINE_ITER)
                .or_else(|| bisect(f, prev_x, x, DEFAULT_REFINE_ITER))
                .unwrap_or(x);
            if let Ok(ya) = eval_function_with_vars(expr_a, root, vars) {
                if ya.is_finite() {
                    out.push(Point2::new(root, ya));
                }
            }
        }
        prev_x = x;
        prev_f = fx;
    }
    out
}

fn function_line_intersection(
    expr: &str,
    s: Point2,
    e: Point2,
    view_bounds: (f64, f64, f64, f64),
    vars: &HashMap<String, f64>,
) -> Vec<Point2> {
    let (xmin, xmax, _, _) = view_bounds;
    let dx = e.x - s.x;
    if dx.abs() < 1e-15 {
        // Línea vertical x = sx; resolver f(sx).
        if let Ok(y) = eval_function_with_vars(expr, s.x, vars) {
            if y.is_finite() {
                return vec![Point2::new(s.x, y)];
            }
        }
        return Vec::new();
    }
    let m = (e.y - s.y) / dx;
    let b = s.y - m * s.x;
    let f_line = |x: f64| -> f64 {
        eval_function_with_vars(expr, x, vars)
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| v - (m * x + b))
            .unwrap_or(f64::NAN)
    };
    let df = |x: f64| -> f64 {
        let h = (x.abs().max(1.0) * EPS).max(1e-12);
        (f_line(x + h) - f_line(x - h)) / (2.0 * h)
    };
    let n = 400;
    let xs: Vec<f64> = (0..=n)
        .map(|i| xmin + (i as f64 / n as f64) * (xmax - xmin))
        .collect();
    let ys =
        eval_batch_1d(expr, "x", xs.iter().copied(), vars).unwrap_or_else(|_| vec![None; n + 1]);

    let get_f = |i: usize| -> f64 {
        if let Some(y) = ys[i] {
            if y.is_finite() {
                y - (m * xs[i] + b)
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        }
    };

    let mut out = Vec::new();
    let mut prev_x = xs[0];
    let mut prev_f = get_f(0);
    for (i, x) in xs.iter().enumerate().take(n + 1).skip(1) {
        let x = *x;
        let fx = get_f(i);
        if !fx.is_finite() {
            prev_x = x;
            prev_f = fx;
            continue;
        }
        if prev_f.is_finite() && prev_f * fx <= 0.0 && (prev_f != 0.0 || fx != 0.0) {
            let root = newton_refine(x, f_line, df, DEFAULT_REFINE_ITER)
                .or_else(|| bisect(f_line, prev_x, x, DEFAULT_REFINE_ITER))
                .unwrap_or(x);
            if let Ok(y) = eval_function_with_vars(expr, root, vars) {
                if y.is_finite() {
                    out.push(Point2::new(root, y));
                }
            }
        }
        prev_x = x;
        prev_f = fx;
    }
    out
}

/// Equilibrios de un campo vectorial 2D con clasificación opcional.
/// La clasificación usa el Jacobiano numérico en el punto de equilibrio.
pub fn analyze_vector_field_equilibrium_class(
    expr_u: &str,
    expr_v: &str,
    point: Point2,
    vars: &HashMap<String, f64>,
) -> Option<String> {
    let h = 1e-5;
    let eval_u = |x: f64, _y: f64| -> f64 {
        eval_function_with_vars(expr_u, x, vars)
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(f64::NAN)
    };
    let eval_v = |x: f64, _y: f64| -> f64 {
        eval_function_with_vars(expr_v, x, vars)
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(f64::NAN)
    };
    let j00 = (eval_u(point.x + h, point.y) - eval_u(point.x - h, point.y)) / (2.0 * h);
    let j01 = (eval_u(point.x, point.y + h) - eval_u(point.x, point.y - h)) / (2.0 * h);
    let j10 = (eval_v(point.x + h, point.y) - eval_v(point.x - h, point.y)) / (2.0 * h);
    let j11 = (eval_v(point.x, point.y + h) - eval_v(point.x, point.y - h)) / (2.0 * h);
    if ![j00, j01, j10, j11].iter().all(|v| v.is_finite()) {
        return None;
    }
    let trace = j00 + j11;
    let det = j00 * j11 - j01 * j10;
    let disc = trace * trace - 4.0 * det;
    if disc < 0.0 {
        if trace.abs() < 1e-6 {
            Some("Centro".to_string())
        } else if trace < 0.0 {
            Some("Estable (espiral)".to_string())
        } else {
            Some("Inestable (espiral)".to_string())
        }
    } else if det < 0.0 {
        Some("Silla".to_string())
    } else if trace < 0.0 {
        Some("Nodo estable".to_string())
    } else {
        Some("Nodo inestable".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_vars() -> HashMap<String, f64> {
        HashMap::new()
    }

    #[test]
    fn test_roots_parabola() {
        let results = analyze_function("x^2 - 4", &empty_vars(), &AnalysisOptions::default());
        let roots: Vec<_> = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::Root)
            .collect();
        assert_eq!(roots.len(), 2);
        let xs: Vec<f64> = roots.iter().map(|r| r.point.x).collect();
        assert!(xs.iter().any(|x| (x + 2.0).abs() < 1e-3));
        assert!(xs.iter().any(|x| (x - 2.0).abs() < 1e-3));
    }

    #[test]
    fn test_y_intercept() {
        let results = analyze_function("2*x + 3", &empty_vars(), &AnalysisOptions::default());
        let yint = results
            .iter()
            .find(|r| r.feature == AnalysisFeature::YIntercept)
            .unwrap();
        assert!((yint.point.y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_extrema_sine() {
        let mut opts = AnalysisOptions::default();
        opts.domain_min = -5.0;
        opts.domain_max = 5.0;
        let results = analyze_function("sin(x)", &empty_vars(), &opts);
        let maxima: Vec<_> = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::LocalMaximum)
            .collect();
        let minima: Vec<_> = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::LocalMinimum)
            .collect();
        assert!(!maxima.is_empty());
        assert!(!minima.is_empty());
        assert!(maxima
            .iter()
            .any(|r| (r.point.x - std::f64::consts::FRAC_PI_2).abs() < 0.1));
    }

    #[test]
    fn test_inflection_cubic() {
        let results = analyze_function("x^3", &empty_vars(), &AnalysisOptions::default());
        let infl = results
            .iter()
            .find(|r| r.feature == AnalysisFeature::Inflection);
        assert!(infl.is_some(), "se esperaba al menos un punto de inflexión");
        assert!(infl.unwrap().point.x.abs() < 1e-3);
    }

    #[test]
    fn test_asymptote_rational() {
        let results = analyze_function("1/x", &empty_vars(), &AnalysisOptions::default());
        let vas = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::VerticalAsymptote)
            .collect::<Vec<_>>();
        assert!(!vas.is_empty());
        assert!(vas.iter().any(|r| r.point.x.abs() < 0.1));
    }

    #[test]
    fn test_analyze_circle_intersects_axis() {
        // (x-2)^2 + (y-1)^2 = 9
        let results = analyze_circle(
            Point2::new(2.0, 1.0),
            3.0,
            &[AnalysisFeature::XIntercept, AnalysisFeature::YIntercept],
        );
        let xs: Vec<f64> = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::XIntercept)
            .map(|r| r.point.x)
            .collect();
        // r² - cy² = 9 - 1 = 8 => x = 2 ± √8
        assert!(xs.contains(&(2.0_f64 - 8.0_f64.sqrt())));
        assert!(xs.contains(&(2.0_f64 + 8.0_f64.sqrt())));
        let ys: Vec<f64> = results
            .iter()
            .filter(|r| r.feature == AnalysisFeature::YIntercept)
            .map(|r| r.point.y)
            .collect();
        // (y - 1)^2 = 9 - 4 = 5  =>  y = 1 ± √5
        assert!(ys
            .iter()
            .any(|y| (*y - (1.0 - 5.0_f64.sqrt())).abs() < 1e-9));
        assert!(ys
            .iter()
            .any(|y| (*y - (1.0 + 5.0_f64.sqrt())).abs() < 1e-9));
    }

    #[test]
    fn test_analyze_polygon_centroid() {
        let tri = vec![
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(2.0, 3.0),
        ];
        let results = analyze_polygon(&tri, &[AnalysisFeature::Centroid]);
        let c = results
            .iter()
            .find(|r| r.feature == AnalysisFeature::Centroid)
            .expect("centroide");
        assert!((c.point.x - 2.0).abs() < 1e-9);
        assert!((c.point.y - 1.0).abs() < 1e-9);
        // área = 6
        assert!((c.value.unwrap() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn test_analyze_intersection_function_line() {
        // f(x) = x² intersecta y = 2x - 1 en x = 1
        let view = (-5.0, 5.0, -5.0, 5.0);
        let pts = analyze_intersection(
            &IntersectionCurve::Function { expr: "x^2" },
            &IntersectionCurve::Line {
                s: Point2::new(0.0, -1.0),
                e: Point2::new(1.0, 1.0),
            },
            view,
            &empty_vars(),
        );
        assert!(!pts.is_empty(), "se esperaba al menos un punto");
        assert!(pts.iter().any(|p| (p.x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn test_analyze_intersection_circle_circle_tangent() {
        // dos círculos tangentes externos en (1, 0)
        let pts = analyze_intersection(
            &IntersectionCurve::Circle {
                center: Point2::new(0.0, 0.0),
                radius: 1.0,
            },
            &IntersectionCurve::Circle {
                center: Point2::new(2.0, 0.0),
                radius: 1.0,
            },
            (-5.0, 5.0, -5.0, 5.0),
            &empty_vars(),
        );
        assert_eq!(pts.len(), 1, "tangentes externos dan 1 punto");
        assert!((pts[0].x - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_analyze_xintercept_parametric() {
        // círculo unitario (cos t, sin t) corta el eje X en t=0, π, 2π
        let mut opts = AnalysisOptions::default();
        opts.domain_min = 0.0;
        opts.domain_max = std::f64::consts::TAU;
        let results = analyze_parametric_curve2d(
            "cos(t)",
            "sin(t)",
            0.0,
            std::f64::consts::TAU,
            &empty_vars(),
            &[AnalysisFeature::Root, AnalysisFeature::XIntercept],
        );
        // Root devuelve cortes con y=0 (eje X) y XIntercept añade lo mismo.
        let xs: Vec<f64> = results
            .iter()
            .filter(|r| {
                r.feature == AnalysisFeature::XIntercept || r.feature == AnalysisFeature::Root
            })
            .map(|r| r.point.x)
            .collect();
        assert!(xs.contains(&1.0));
        assert!(xs.contains(&-1.0));
        let _ = opts;
    }
}
