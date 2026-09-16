//! Optimización 1D y EDO numérica: `Minimize`/`Maximize` y `NSolveODE`.
//!
//! Todo acotado y determinista: la grilla es fija, el refinamiento es
//! sección áurea y el RK45 es Dormand–Prince con paso adaptativo y techo
//! de evaluaciones. El mínimo reportado es el del intervalo (global no
//! garantizado: se declara en el mensaje, no se inventa).

use crate::ast::{parse_ast, Expr};

/// Puntos de la grilla gruesa de minimización.
pub const MAX_MINIMIZE_GRID: usize = 4096;
/// Iteraciones de sección áurea por candidato.
pub const MAX_GOLDEN_ITERS: usize = 64;
/// Puntos máximos de una tabla `NSolveODE`.
pub const MAX_ODE_POINTS: usize = 1000;
/// Evaluaciones máximas del integrador RK45.
pub const MAX_ODE_EVALS: usize = 100_000;

/// Mínimo/máximo local refinado.
#[derive(Debug, Clone, PartialEq)]
pub struct Extremum {
    /// Abscisa del extremo.
    pub x: f64,
    /// Valor de la función.
    pub value: f64,
}

/// Evalúa `expr` (una variable) en `at`; `None` si no evalúa finito.
fn eval_1d(expr: &Expr, var: &str, at: f64) -> Option<f64> {
    let value = expr.eval_at(var, at);
    value.is_finite().then_some(value)
}

/// Extremo global-en-grilla + refinamiento áureo de `expr` en `[lo, hi]`.
///
/// `find_max` invierte el sentido. Devuelve el mejor candidato con su
/// valor; si nada evalúa finito, error honesto.
pub fn minimize_1d(
    expr: &str,
    var: &str,
    lo: f64,
    hi: f64,
    find_max: bool,
) -> Result<Extremum, String> {
    if !lo.is_finite() || !hi.is_finite() || lo >= hi {
        return Err("el intervalo debe ser finito con a < b".to_string());
    }
    let ast = parse_ast(&expr.replace(' ', ""))
        .map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let better = |a: f64, b: f64| {
        if find_max {
            a > b
        } else {
            a < b
        }
    };
    // Grilla gruesa determinista.
    let mut best: Option<(f64, f64)> = None;
    for i in 0..=MAX_MINIMIZE_GRID {
        let x = lo + (hi - lo) * (i as f64) / (MAX_MINIMIZE_GRID as f64);
        if let Some(v) = eval_1d(&ast, var, x) {
            match best {
                Some((_, bv)) if !better(v, bv) => {}
                _ => best = Some((x, v)),
            }
        }
    }
    let (mut bx, mut bv) = best.ok_or_else(|| "f no evalúa finito en el intervalo".to_string())?;
    // Refinamiento áureo alrededor del mejor punto de grilla.
    let step = (hi - lo) / (MAX_MINIMIZE_GRID as f64);
    let (mut a, mut b) = ((bx - step).max(lo), (bx + step).min(hi));
    const INV_PHI: f64 = 0.618_033_988_749_894_9;
    for _ in 0..MAX_GOLDEN_ITERS {
        if (b - a).abs() < 1e-13 * (1.0 + a.abs().max(b.abs())) {
            break;
        }
        let c = b - INV_PHI * (b - a);
        let d = a + INV_PHI * (b - a);
        match (eval_1d(&ast, var, c), eval_1d(&ast, var, d)) {
            (Some(fc), Some(fd)) => {
                let take_left = if find_max { fc > fd } else { fc < fd };
                if take_left {
                    b = d;
                } else {
                    a = c;
                }
            }
            _ => break,
        }
    }
    for x in [a, b, (a + b) * 0.5] {
        if let Some(v) = eval_1d(&ast, var, x) {
            if better(v, bv) {
                bx = x;
                bv = v;
            }
        }
    }
    Ok(Extremum { x: bx, value: bv })
}

/// Tabla `(x, y)` de `y' = f(x, y)`, `y(x0) = y0` hasta `x1` (RK45).
///
/// `n` puntos de salida (2..=`MAX_ODE_POINTS`). Paso adaptativo con
/// tolerancia 1e-9 y techo `MAX_ODE_EVALS`; más allá, error honesto.
pub fn nsolve_ode_table(
    expr: &str,
    x0: f64,
    y0: f64,
    x1: f64,
    n: usize,
) -> Result<Vec<(f64, f64)>, String> {
    if ![x0, y0, x1].iter().all(|v| v.is_finite()) {
        return Err("condiciones inicial y final deben ser finitas".to_string());
    }
    if x1 == x0 {
        return Err("x1 debe diferir de x0".to_string());
    }
    if !(2..=MAX_ODE_POINTS).contains(&n) {
        return Err(format!("n debe estar entre 2 y {MAX_ODE_POINTS}"));
    }
    let ast = parse_ast(&expr.replace(' ', ""))
        .map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let field = |x: f64, y: f64| -> Option<f64> {
        // Sustituye x e y por constantes y evalúa.
        let sx = crate::ast::Expr::Const(x);
        let sy = crate::ast::Expr::Const(y);
        let mut with_x = substitute_var(&ast, "x", &sx);
        with_x = substitute_var(&with_x, "y", &sy);
        let value = eval_closed(&with_x)?;
        value.is_finite().then_some(value)
    };
    // Dormand–Prince 5(4) con paso adaptativo.
    let direction = (x1 - x0).signum();
    let mut h = (x1 - x0).abs() / (n as f64 * 4.0);
    h = h.clamp(1e-12, (x1 - x0).abs());
    let (mut x, mut y) = (x0, y0);
    let mut evals = 0usize;
    let mut table = vec![(x0, y0)];
    let targets: Vec<f64> = (1..n)
        .map(|i| x0 + (x1 - x0) * (i as f64) / ((n - 1) as f64))
        .collect();
    for &target in &targets {
        let mut guard = 0usize;
        while (target - x) * direction > 0.0 {
            guard += 1;
            if guard > 10_000 {
                return Err("RK45 no avanza: campo rígido o singular".to_string());
            }
            if evals > MAX_ODE_EVALS {
                return Err(format!(
                    "RK45 excede {MAX_ODE_EVALS} evaluaciones; achica el intervalo"
                ));
            }
            let step = h.min((target - x).abs());
            let (y5, y4) = dp45_step(&field, x, y, step * direction)?;
            evals += 6;
            let err = (y5 - y4).abs();
            let tol = 1e-9 * (1.0 + y.abs().max(y5.abs()));
            if err <= tol {
                x += step * direction;
                y = y5;
            }
            h = adapt_step(h, err, tol, (x1 - x0).abs());
            if !y.is_finite() {
                return Err(format!("la solución diverge cerca de x={x}"));
            }
        }
        table.push((target, y));
    }
    Ok(table)
}

fn adapt_step(h: f64, err: f64, tol: f64, span: f64) -> f64 {
    if err == 0.0 {
        return (h * 2.0).min(span);
    }
    let factor = 0.9 * (tol / err).powf(0.2);
    (h * factor.clamp(0.2, 2.0)).clamp(1e-12, span)
}

/// Un paso Dormand–Prince: devuelve `(orden5, orden4)`.
fn dp45_step(
    field: &impl Fn(f64, f64) -> Option<f64>,
    x: f64,
    y: f64,
    h: f64,
) -> Result<(f64, f64), String> {
    let f = |x: f64, y: f64| field(x, y).ok_or_else(|| "campo no evaluable".to_string());
    let k1 = f(x, y)?;
    let k2 = f(x + h / 5.0, y + h * (k1 / 5.0))?;
    let k3 = f(
        x + h * 3.0 / 10.0,
        y + h * (3.0 * k1 / 40.0 + 9.0 * k2 / 40.0),
    )?;
    let k4 = f(
        x + h * 4.0 / 5.0,
        y + h * (44.0 * k1 / 45.0 - 56.0 * k2 / 15.0 + 32.0 * k3 / 9.0),
    )?;
    let k5 = f(
        x + h * 8.0 / 9.0,
        y + h
            * (19372.0 * k1 / 6561.0 - 25360.0 * k2 / 2187.0 + 64448.0 * k3 / 6561.0
                - 212.0 * k4 / 729.0),
    )?;
    let k6 = f(
        x + h,
        y + h
            * (9017.0 * k1 / 3168.0 - 355.0 * k2 / 33.0
                + 46732.0 * k3 / 5247.0
                + 49.0 * k4 / 176.0
                - 5103.0 * k5 / 18656.0),
    )?;
    let y5 = y + h
        * (35.0 * k1 / 384.0 + 500.0 * k3 / 1113.0 + 125.0 * k4 / 192.0 - 2187.0 * k5 / 6784.0
            + 11.0 * k6 / 84.0);
    let k7 = f(x + h, y5)?;
    let y4 = y + h
        * (5179.0 * k1 / 57600.0 + 7571.0 * k3 / 16695.0 + 393.0 * k4 / 640.0
            - 92097.0 * k5 / 339200.0
            + 187.0 * k6 / 2100.0
            + k7 / 40.0);
    Ok((y5, y4))
}

/// Sustituye `var` por `replacement` (clonando; acotado por el AST dado).
fn substitute_var(expr: &Expr, var: &str, replacement: &Expr) -> Expr {
    use Expr::*;
    match expr {
        Const(_) => expr.clone(),
        Var(v) if v == var => replacement.clone(),
        Var(_) => expr.clone(),
        Neg(u) => Neg(Box::new(substitute_var(u, var, replacement))),
        Add(a, b) => Add(
            Box::new(substitute_var(a, var, replacement)),
            Box::new(substitute_var(b, var, replacement)),
        ),
        Sub(a, b) => Sub(
            Box::new(substitute_var(a, var, replacement)),
            Box::new(substitute_var(b, var, replacement)),
        ),
        Mul(a, b) => Mul(
            Box::new(substitute_var(a, var, replacement)),
            Box::new(substitute_var(b, var, replacement)),
        ),
        Div(a, b) => Div(
            Box::new(substitute_var(a, var, replacement)),
            Box::new(substitute_var(b, var, replacement)),
        ),
        Pow(a, b) => Pow(
            Box::new(substitute_var(a, var, replacement)),
            Box::new(substitute_var(b, var, replacement)),
        ),
        Sin(u) => Sin(sub1(u, var, replacement)),
        Cos(u) => Cos(sub1(u, var, replacement)),
        Tan(u) => Tan(sub1(u, var, replacement)),
        Asin(u) => Asin(sub1(u, var, replacement)),
        Acos(u) => Acos(sub1(u, var, replacement)),
        Atan(u) => Atan(sub1(u, var, replacement)),
        Exp(u) => Exp(sub1(u, var, replacement)),
        Ln(u) => Ln(sub1(u, var, replacement)),
        Log(u) => Log(sub1(u, var, replacement)),
        Sqrt(u) => Sqrt(sub1(u, var, replacement)),
        Abs(u) => Abs(sub1(u, var, replacement)),
        Sinh(u) => Sinh(sub1(u, var, replacement)),
        Cosh(u) => Cosh(sub1(u, var, replacement)),
        Tanh(u) => Tanh(sub1(u, var, replacement)),
        Floor(u) => Floor(sub1(u, var, replacement)),
        Ceil(u) => Ceil(sub1(u, var, replacement)),
        Round(u) => Round(sub1(u, var, replacement)),
        Sec(u) => Sec(sub1(u, var, replacement)),
        Csc(u) => Csc(sub1(u, var, replacement)),
        Cot(u) => Cot(sub1(u, var, replacement)),
        Asinh(u) => Asinh(sub1(u, var, replacement)),
        Acosh(u) => Acosh(sub1(u, var, replacement)),
        Atanh(u) => Atanh(sub1(u, var, replacement)),
        Sign(u) => Sign(sub1(u, var, replacement)),
        Heaviside(u) => Heaviside(sub1(u, var, replacement)),
        Cbrt(u) => Cbrt(sub1(u, var, replacement)),
        Re(u) => Re(sub1(u, var, replacement)),
        Im(u) => Im(sub1(u, var, replacement)),
        Arg(u) => Arg(sub1(u, var, replacement)),
        Conj(u) => Conj(sub1(u, var, replacement)),
        Erf(u) => Erf(sub1(u, var, replacement)),
        Erfc(u) => Erfc(sub1(u, var, replacement)),
        Gamma(u) => Gamma(sub1(u, var, replacement)),
        LnGamma(u) => LnGamma(sub1(u, var, replacement)),
        Digamma(u) => Digamma(sub1(u, var, replacement)),
        Trigamma(u) => Trigamma(sub1(u, var, replacement)),
        _ => expr.clone(),
    }
}

fn sub1(u: &Expr, var: &str, replacement: &Expr) -> Box<Expr> {
    Box::new(substitute_var(u, var, replacement))
}

/// Evalúa un AST cerrado (sin variables libres conocidas).
fn eval_closed(expr: &Expr) -> Option<f64> {
    use Expr::*;
    let num = |u: &Expr| eval_closed(u);
    match expr {
        Const(v) => Some(*v),
        Var(_) => None,
        Neg(u) => num(u).map(|v| -v),
        Add(a, b) => Some(num(a)? + num(b)?),
        Sub(a, b) => Some(num(a)? - num(b)?),
        Mul(a, b) => Some(num(a)? * num(b)?),
        Div(a, b) => {
            let d = num(b)?;
            if d == 0.0 {
                None
            } else {
                Some(num(a)? / d)
            }
        }
        Pow(a, b) => Some(num(a)?.powf(num(b)?)),
        Sin(u) => num(u).map(f64::sin),
        Cos(u) => num(u).map(f64::cos),
        Tan(u) => num(u).map(f64::tan),
        Exp(u) => num(u).map(f64::exp),
        Ln(u) => num(u).filter(|v| *v > 0.0).map(f64::ln),
        Sqrt(u) => num(u).filter(|v| *v >= 0.0).map(f64::sqrt),
        Abs(u) => num(u).map(f64::abs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimize_parabola() {
        let e = minimize_1d("x^2-4*x+1", "x", -10.0, 10.0, false).expect("mínimo");
        assert!((e.x - 2.0).abs() < 1e-6, "got {e:?}");
        assert!((e.value + 3.0).abs() < 1e-6, "got {e:?}");
    }

    #[test]
    fn maximize_sine() {
        let e = minimize_1d("sin(x)", "x", 0.0, std::f64::consts::TAU, true).expect("máximo");
        assert!((e.value - 1.0).abs() < 1e-4, "got {e:?}");
    }

    #[test]
    fn minimize_needs_finite_interval() {
        assert!(minimize_1d("x", "x", 5.0, 5.0, false).is_err());
    }

    #[test]
    fn nsolve_ode_exponential() {
        // y' = y, y(0) = 1 → e^1 ≈ 2.718 en x=1.
        let table = nsolve_ode_table("y", 0.0, 1.0, 1.0, 11).expect("exponencial");
        assert_eq!(table.len(), 11);
        let last = table.last().unwrap();
        assert!((last.1 - std::f64::consts::E).abs() < 1e-6, "got {last:?}");
    }

    #[test]
    fn nsolve_ode_harmonic_half_period() {
        // Sistema decaído a 1er orden vía campo: y' = -y → e^-1.
        let table = nsolve_ode_table("-y", 0.0, 1.0, 1.0, 11).expect("decaimiento");
        let last = table.last().unwrap();
        assert!(
            (last.1 - 1.0 / std::f64::consts::E).abs() < 1e-6,
            "got {last:?}"
        );
    }

    #[test]
    fn nsolve_ode_rejects_bad_input() {
        assert!(nsolve_ode_table("y", 0.0, 1.0, 0.0, 11).is_err());
        assert!(nsolve_ode_table("y", 0.0, 1.0, 1.0, 1).is_err());
    }
}
