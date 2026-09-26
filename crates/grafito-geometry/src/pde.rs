//! Motores EDP para `HeatEquation`, `WaveEquation` y `Laplace2D`.
//!
//! Los tres resuelven con diferencias finitas sobre grillas acotadas y
//! devuelven la solución muestreada como texto (mismo estilo de resumen que
//! los `geo_*` vecinos de `grafito-command`: valores con 6 decimales más
//! metadatos del esquema). La condición inicial / datos de borde se parsean
//! con el parser existente del crate (`parse_ast` + `CompiledExpr`); los
//! presupuestos del repo aplican (`MAX_EXPR_LENGTH` = 2000 vía
//! `CompiledExpr::new`, grillas y pasos acotados acá).
//!
//! # Convenciones (diseño, no física arbitraria)
//!
//! - Calor 1D (`u_t = u_xx`) y ondas 1D (`u_tt = u_xx`, `c = 1`): intervalo
//!   `[0, 1]` con borde Dirichlet homogéneo `u = 0` en ambos extremos. La
//!   condición inicial es `u(x, t0) = expr`; en ondas la velocidad inicial es
//!   `u_t(x, t0) = 0`.
//! - Laplace 2D (`u_xx + u_yy = 0`): rectángulo `[xmin, xmax] × [ymin, ymax]`
//!   con Dirichlet dado por 4 expresiones que pueden usar `x` y/o `y`:
//!   `g_inf`/`g_sup` se evalúan en `(x_i, ymin)`/`(x_i, ymax)` y
//!   `g_izq`/`g_der` en `(xmin, y_j)`/`(xmax, y_j)`. Cada esquina es el
//!   promedio de los dos bordes que la tocan.
//! - Estabilidad: FTCS exige `r = dt/dx² ≤ 1/2`; ondas exige CFL
//!   `C = dt/dx ≤ 1`. La grilla se elige para cumplirlo y el chequeo queda
//!   como guarda honesta: si se viola, error en lugar de números inventados.

use crate::ast::parse_ast;
use crate::expr::{preprocess_expr, CompiledExpr, MAX_EXPR_LENGTH};
use std::collections::{BTreeMap, HashSet};

/// Nodos espaciales del calor 1D en `[0, 1]` (`dx = 0.01`).
pub const HEAT_NODES: usize = 101;
/// Número de Courant objetivo del FTCS (`r = dt/dx²`, estable si `≤ 1/2`).
pub const HEAT_R: f64 = 0.4;
/// Nodos espaciales de ondas 1D en `[0, 1]` (`dx = 0.005`).
pub const WAVE_NODES: usize = 201;
/// Tope de pasos temporales (calor y ondas). Pasarlo es error honesto.
pub const MAX_TIME_STEPS: usize = 200_000;
/// Nodos por eje de Laplace 2D (grilla `41 × 41`).
pub const LAPLACE_N: usize = 41;
/// Tope de barridos Gauss-Seidel. No converger es error honesto.
pub const LAPLACE_MAX_SWEEPS: usize = 10_000;
/// Corte de convergencia: cambio máximo por barrido menor que esto.
pub const LAPLACE_TOL: f64 = 1e-9;
/// Muestras del perfil 1D en el texto de salida.
const PROFILE_SAMPLES: usize = 11;
/// Muestras por corte en el texto de salida de Laplace.
const LAPLACE_SAMPLES: usize = 9;

/// Solución 1D muestreada `u(x, t_end)` sobre `[0, 1]`.
#[derive(Clone, Debug)]
pub(crate) struct Profile1D {
    /// Coordenadas de la malla.
    pub(crate) xs: Vec<f64>,
    /// Valores `u(x_i, t_end)`.
    pub(crate) values: Vec<f64>,
    /// Tiempo final pedido.
    pub(crate) t_end: f64,
    /// Nodos espaciales usados.
    pub(crate) nodes: usize,
    /// Pasos temporales dados.
    pub(crate) steps: usize,
    /// Número de estabilidad (`r` en calor, `C` en ondas).
    pub(crate) stability: f64,
}

/// Solución de Laplace 2D en el rectángulo.
#[derive(Clone, Debug)]
pub(crate) struct LaplaceGrid {
    /// Valores en orden de filas (`valores[j * nx + i]`).
    pub(crate) values: Vec<f64>,
    /// Nodos en `x`.
    pub(crate) nx: usize,
    /// Nodos en `y`.
    pub(crate) ny: usize,
    /// Dominio pedido.
    pub(crate) xmin: f64,
    /// Dominio pedido.
    pub(crate) xmax: f64,
    /// Dominio pedido.
    pub(crate) ymin: f64,
    /// Dominio pedido.
    pub(crate) ymax: f64,
    /// Barridos Gauss-Seidel ejecutados.
    pub(crate) sweeps: usize,
    /// Cambio máximo del último barrido.
    pub(crate) max_update: f64,
}

// ---------------------------------------------------------------------------
// Validación compartida
// ---------------------------------------------------------------------------

/// Variable de barrido saneada: no vacía.
fn check_var(var: &str) -> Result<String, String> {
    let v = var.trim();
    if v.is_empty() {
        return Err("variable de barrido vacía".to_string());
    }
    Ok(v.to_string())
}

/// Horizonte temporal `[t0, t_end]`: finito y no hacia atrás.
fn check_horizon(t0: f64, t_end: f64) -> Result<f64, String> {
    if !t0.is_finite() || !t_end.is_finite() {
        return Err(format!("tiempos no finitos: t0 = {t0}, t_end = {t_end}"));
    }
    if t_end < t0 {
        return Err(format!(
            "t_end = {t_end} menor que t0 = {t0}: no se integra hacia atrás en el tiempo"
        ));
    }
    Ok(t_end - t0)
}

/// Rechaza variables ajenas a `allowed` (mismo criterio que
/// `fourier::parse_univariate`). `pi`/`e` no aparecen: el parser los
/// sustituye por literales.
fn check_vars(expr: &str, allowed: &[&str]) -> Result<(), String> {
    if expr.trim().is_empty() {
        return Err("expresión vacía".to_string());
    }
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "la expresión excede {MAX_EXPR_LENGTH} bytes (tiene {})",
            expr.len()
        ));
    }
    let clean = preprocess_expr(expr);
    let ast = parse_ast(&clean).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let mut vars = HashSet::new();
    ast.get_variables(&mut vars);
    let mut bad: Vec<&String> = vars
        .iter()
        .filter(|v| !allowed.contains(&v.as_str()))
        .collect();
    bad.sort();
    if let Some(first) = bad.first() {
        return Err(format!(
            "la expresión '{expr}' trae la variable ajena '{first}' (se esperaban solo {})",
            allowed.join(", ")
        ));
    }
    Ok(())
}

/// Compila la condición inicial 1D como función de `var`.
fn compile_initial(expr: &str, var: &str) -> Result<CompiledExpr, String> {
    check_vars(expr, &[var])?;
    CompiledExpr::new(expr, &BTreeMap::new())
        .map_err(|e| format!("condición inicial inválida '{expr}': {e}"))
}

/// Compila un dato de borde 2D (puede usar `x` y/o `y`).
fn compile_edge(expr: &str, edge: &str) -> Result<CompiledExpr, String> {
    check_vars(expr, &["x", "y"])?;
    CompiledExpr::new(expr, &BTreeMap::new())
        .map_err(|e| format!("borde {edge} inválido '{expr}': {e}"))
}

/// Evalúa `f` en un punto exigiendo resultado finito (nunca `Inf`/`NaN`
/// silencioso).
fn eval_finite(f: &CompiledExpr, vars: &[(String, f64)], where_: &str) -> Result<f64, String> {
    let v = f
        .eval(vars)
        .map_err(|e| format!("no se pudo evaluar en {where_}: {e}"))?;
    if !v.is_finite() {
        return Err(format!("la expresión no es finita en {where_}"));
    }
    Ok(v)
}

/// Formato escalar vecino de `fmt_scalar` de `commands.rs`: `0` si es
/// despreciable, entero pelado si lo es, si no 6 decimales.
fn fmt_num(x: f64) -> String {
    if x.abs() < 5e-11 {
        "0".to_string()
    } else if (x - x.round()).abs() < 5e-10 {
        format!("{:.0}", x.round())
    } else {
        format!("{:.6}", x)
    }
}

/// `count` muestras equiespaciadas de `values` (siempre incluye extremos).
fn sampled_values(values: &[f64], count: usize) -> String {
    let n = values.len();
    let mut parts = Vec::with_capacity(count);
    for k in 0..count {
        let i = if count == 1 {
            0
        } else {
            k * (n - 1) / (count - 1)
        };
        match values.get(i) {
            Some(v) => parts.push(fmt_num(*v)),
            None => parts.push("?".to_string()),
        }
    }
    parts.join(", ")
}

/// Coordenadas reales de las muestras de arriba.
fn sampled_xs(xs: &[f64], count: usize) -> String {
    let n = xs.len();
    let mut parts = Vec::with_capacity(count);
    for k in 0..count {
        let i = if count == 1 {
            0
        } else {
            k * (n - 1) / (count - 1)
        };
        match xs.get(i) {
            Some(v) => parts.push(fmt_num(*v)),
            None => parts.push("?".to_string()),
        }
    }
    parts.join(", ")
}

/// Coordenadas de las muestras de arriba.
fn sampled_coords(a: f64, b: f64, count: usize) -> String {
    let mut parts = Vec::with_capacity(count);
    for k in 0..count {
        let v = if count == 1 {
            a
        } else {
            a + (b - a) * k as f64 / (count - 1) as f64
        };
        parts.push(fmt_num(v));
    }
    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Calor 1D: u_t = u_xx, FTCS explícito
// ---------------------------------------------------------------------------

/// Perfil `u(x, t_end)` del calor 1D con dato inicial `expr`.
///
/// FTCS: `u_i^{n+1} = u_i^n + r·(u_{i+1}^n − 2·u_i^n + u_{i−1}^n)` con
/// `r = dt/dx² = HEAT_R = 0.4 ≤ 1/2`. Si `t_end == t0` no se integra y se
/// devuelve el dato inicial (con bordes en cero).
pub(crate) fn heat_profile(
    expr: &str,
    var: &str,
    t0: f64,
    t_end: f64,
) -> Result<Profile1D, String> {
    let var = check_var(var)?;
    let horizon = check_horizon(t0, t_end)?;
    let f = compile_initial(expr, &var)?;
    let n = HEAT_NODES;
    let dx = 1.0 / (n as f64 - 1.0);
    let xs: Vec<f64> = (0..n).map(|i| i as f64 * dx).collect();
    let mut u = Vec::with_capacity(n);
    for x in &xs {
        let v = eval_finite(
            &f,
            &[(var.clone(), *x)],
            &format!("x = {x} de la condición inicial"),
        )?;
        u.push(v);
    }
    // Borde Dirichlet homogéneo (documentado en el resumen de salida).
    if let Some(first) = u.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = u.last_mut() {
        *last = 0.0;
    }
    if horizon <= 0.0 {
        return Ok(Profile1D {
            xs,
            values: u,
            t_end,
            nodes: n,
            steps: 0,
            stability: HEAT_R,
        });
    }
    let dt_guess = HEAT_R * dx * dx;
    let mut steps = (horizon / dt_guess).ceil() as usize;
    if steps < 1 {
        steps = 1;
    }
    if steps > MAX_TIME_STEPS {
        return Err(format!(
            "el horizonte t_end − t0 = {horizon} pide {steps} pasos (tope {MAX_TIME_STEPS}): achicá el horizonte o subdividí el tramo"
        ));
    }
    let dt = horizon / steps as f64;
    let r = dt / (dx * dx);
    if r > 0.5 + 1e-12 {
        return Err(format!(
            "FTCS inestable con r = {r:.6} > 0.5 (dt = {dt:.3e}, dx = {dx:.3e}): no se devuelve nada"
        ));
    }
    let mut next = vec![0.0; n];
    for _ in 0..steps {
        for (win, slot) in u.windows(3).zip(next.iter_mut().skip(1)) {
            *slot = win[1] + r * (win[0] - 2.0 * win[1] + win[2]);
        }
        next[0] = 0.0;
        next[n - 1] = 0.0;
        std::mem::swap(&mut u, &mut next);
    }
    if u.iter().any(|v| !v.is_finite()) {
        return Err(
            "el esquema FTCS produjo valores no finitos: dato inicial o horizonte fuera de rango"
                .to_string(),
        );
    }
    Ok(Profile1D {
        xs,
        values: u,
        t_end,
        nodes: n,
        steps,
        stability: r,
    })
}

/// Resume `u(x, t_end)` del calor 1D como texto muestreado.
///
/// Contrato del call site `geo_heat_1d` en `commands.rs`:
/// `pde::solve_heat_1d(expr, x, t0, t_end) -> Result<String, String>`.
pub fn solve_heat_1d(expr: &str, var: &str, t0: f64, t_end: f64) -> Result<String, String> {
    let s = heat_profile(expr, var, t0, t_end)?;
    Ok(format!(
        "calor 1D u_t = u_xx en [0, 1] con borde Dirichlet homogéneo \
        (FTCS explícito, malla {}, {} pasos, r = {:.4}): \
        u(x, t_end = {}) = [{}] en x = [{}]",
        s.nodes,
        s.steps,
        s.stability,
        fmt_num(s.t_end),
        sampled_values(&s.values, PROFILE_SAMPLES),
        sampled_xs(&s.xs, PROFILE_SAMPLES),
    ))
}

// ---------------------------------------------------------------------------
// Ondas 1D: u_tt = c²u_xx con c = 1, esquema explícito de 2do orden
// ---------------------------------------------------------------------------

/// Perfil `u(x, t_end)` de ondas 1D con desplazamiento inicial `expr` y
/// velocidad inicial nula.
///
/// Esquema de 2do orden:
/// `u_i^{n+1} = 2·u_i^n − u_i^{n−1} + C²·(u_{i+1}^n − 2·u_i^n + u_{i−1}^n)`
/// con `C = dt/dx ≤ 1` (CFL). El primer paso sale de Taylor con `u_t = 0`:
/// `u_i^1 = u_i^0 + C²/2·(u_{i+1}^0 − 2·u_i^0 + u_{i−1}^0)`.
pub(crate) fn wave_profile(
    expr: &str,
    var: &str,
    t0: f64,
    t_end: f64,
) -> Result<Profile1D, String> {
    let var = check_var(var)?;
    let horizon = check_horizon(t0, t_end)?;
    let f = compile_initial(expr, &var)?;
    let n = WAVE_NODES;
    let dx = 1.0 / (n as f64 - 1.0);
    let xs: Vec<f64> = (0..n).map(|i| i as f64 * dx).collect();
    let mut prev = Vec::with_capacity(n);
    for x in &xs {
        let v = eval_finite(
            &f,
            &[(var.clone(), *x)],
            &format!("x = {x} de la condición inicial"),
        )?;
        prev.push(v);
    }
    if let Some(first) = prev.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = prev.last_mut() {
        *last = 0.0;
    }
    if horizon <= 0.0 {
        return Ok(Profile1D {
            xs,
            values: prev,
            t_end,
            nodes: n,
            steps: 0,
            stability: 1.0,
        });
    }
    let mut steps = (horizon / dx).ceil() as usize;
    if steps < 1 {
        steps = 1;
    }
    if steps > MAX_TIME_STEPS {
        return Err(format!(
            "el horizonte t_end − t0 = {horizon} pide {steps} pasos (tope {MAX_TIME_STEPS}): achicá el horizonte o subdividí el tramo"
        ));
    }
    let dt = horizon / steps as f64;
    let cfl = dt / dx;
    if cfl > 1.0 + 1e-12 {
        return Err(format!(
            "CFL violada con C = {cfl:.6} > 1 (dt = {dt:.3e}, dx = {dx:.3e}): no se devuelve nada"
        ));
    }
    let c2 = cfl * cfl;
    // Primer paso (velocidad inicial nula).
    let mut curr = vec![0.0; n];
    for (win, slot) in prev.windows(3).zip(curr.iter_mut().skip(1)) {
        *slot = win[1] + 0.5 * c2 * (win[0] - 2.0 * win[1] + win[2]);
    }
    let mut next = vec![0.0; n];
    for _ in 1..steps {
        for ((win, p), slot) in curr
            .windows(3)
            .zip(prev.iter().skip(1))
            .zip(next.iter_mut().skip(1))
        {
            *slot = 2.0 * win[1] - p + c2 * (win[0] - 2.0 * win[1] + win[2]);
        }
        std::mem::swap(&mut prev, &mut curr);
        std::mem::swap(&mut curr, &mut next);
    }
    if curr.iter().any(|v| !v.is_finite()) {
        return Err("el esquema de ondas produjo valores no finitos: dato inicial o horizonte fuera de rango".to_string());
    }
    Ok(Profile1D {
        xs,
        values: curr,
        t_end,
        nodes: n,
        steps,
        stability: cfl,
    })
}

/// Resume `u(x, t_end)` de ondas 1D como texto muestreado.
///
/// Contrato del call site `geo_wave_1d` en `commands.rs`:
/// `pde::solve_wave_1d(expr, x, t0, t_end) -> Result<String, String>`.
pub fn solve_wave_1d(expr: &str, var: &str, t0: f64, t_end: f64) -> Result<String, String> {
    let s = wave_profile(expr, var, t0, t_end)?;
    Ok(format!(
        "ondas 1D u_tt = u_xx en [0, 1] con borde Dirichlet homogéneo \
        (explícito de 2do orden, c = 1, velocidad inicial nula, malla {}, {} pasos, C = {:.4}): \
        u(x, t_end = {}) = [{}] en x = [{}]",
        s.nodes,
        s.steps,
        s.stability,
        fmt_num(s.t_end),
        sampled_values(&s.values, PROFILE_SAMPLES),
        sampled_xs(&s.xs, PROFILE_SAMPLES),
    ))
}

// ---------------------------------------------------------------------------
// Laplace 2D en rectángulo: Gauss-Seidel
// ---------------------------------------------------------------------------

/// Valida el rectángulo `[xmin, xmax] × [ymin, ymax]`.
fn check_rect(xmin: f64, xmax: f64, ymin: f64, ymax: f64) -> Result<(), String> {
    for (nombre, v) in [
        ("xmin", xmin),
        ("xmax", xmax),
        ("ymin", ymin),
        ("ymax", ymax),
    ] {
        if !v.is_finite() {
            return Err(format!("borde {nombre} = {v} no finito"));
        }
    }
    if xmax <= xmin {
        return Err(format!(
            "rectángulo inválido en x: xmax = {xmax} debe ser mayor que xmin = {xmin}"
        ));
    }
    if ymax <= ymin {
        return Err(format!(
            "rectángulo inválido en y: ymax = {ymax} debe ser mayor que ymin = {ymin}"
        ));
    }
    Ok(())
}

/// Resuelve Laplace 2D con Dirichlet por Gauss-Seidel in place.
///
/// Interior inicializado en cero; la actualización anisotrópica es
/// `u = (wx·(u_oeste + u_este) + wy·(u_sur + u_norte)) / (2·wx + 2·wy)`
/// con `wx = 1/dx²`, `wy = 1/dy²`. Converge cuando el cambio máximo por
/// barrido cae bajo `LAPLACE_TOL`; si no, error honesto.
#[allow(clippy::too_many_arguments)] // 4 fronteras + dominio: inseparables
pub(crate) fn laplace_solve(
    g_sup: &str,
    g_inf: &str,
    g_izq: &str,
    g_der: &str,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
) -> Result<LaplaceGrid, String> {
    check_rect(xmin, xmax, ymin, ymax)?;
    let f_sup = compile_edge(g_sup, "superior")?;
    let f_inf = compile_edge(g_inf, "inferior")?;
    let f_izq = compile_edge(g_izq, "izquierdo")?;
    let f_der = compile_edge(g_der, "derecho")?;
    let nx = LAPLACE_N;
    let ny = LAPLACE_N;
    let dx = (xmax - xmin) / (nx as f64 - 1.0);
    let dy = (ymax - ymin) / (ny as f64 - 1.0);
    let x_at = |i: usize| xmin + i as f64 * dx;
    let y_at = |j: usize| ymin + j as f64 * dy;
    let mut u = vec![0.0; nx * ny];

    // Bordes inferior/superior (filas j = 0 y j = ny − 1).
    for i in 0..nx {
        let x = x_at(i);
        let v_inf = eval_finite(
            &f_inf,
            &[("x".to_string(), x), ("y".to_string(), ymin)],
            &format!("borde inferior en x = {x}"),
        )?;
        let v_sup = eval_finite(
            &f_sup,
            &[("x".to_string(), x), ("y".to_string(), ymax)],
            &format!("borde superior en x = {x}"),
        )?;
        u[i] = v_inf;
        u[(ny - 1) * nx + i] = v_sup;
    }
    // Bordes izquierdo/derecho (columnas i = 0 e i = nx − 1, sin esquinas).
    for j in 1..ny - 1 {
        let y = y_at(j);
        let v_izq = eval_finite(
            &f_izq,
            &[("x".to_string(), xmin), ("y".to_string(), y)],
            &format!("borde izquierdo en y = {y}"),
        )?;
        let v_der = eval_finite(
            &f_der,
            &[("x".to_string(), xmax), ("y".to_string(), y)],
            &format!("borde derecho en y = {y}"),
        )?;
        u[j * nx] = v_izq;
        u[j * nx + nx - 1] = v_der;
    }
    // Esquinas: promedio de los dos bordes que se tocan.
    for (k, edge_a, edge_b) in [
        (0, &f_inf, &f_izq),
        (nx - 1, &f_inf, &f_der),
        ((ny - 1) * nx, &f_sup, &f_izq),
        ((ny - 1) * nx + nx - 1, &f_sup, &f_der),
    ] {
        let (cx, cy) = (x_at(k % nx), y_at(k / nx));
        let va = eval_finite(
            edge_a,
            &[("x".to_string(), cx), ("y".to_string(), cy)],
            &format!("esquina en ({cx}, {cy})"),
        )?;
        let vb = eval_finite(
            edge_b,
            &[("x".to_string(), cx), ("y".to_string(), cy)],
            &format!("esquina en ({cx}, {cy})"),
        )?;
        if let Some(slot) = u.get_mut(k) {
            *slot = 0.5 * (va + vb);
        }
    }

    let wx = 1.0 / (dx * dx);
    let wy = 1.0 / (dy * dy);
    let denom = 2.0 * wx + 2.0 * wy;
    let mut sweeps = 0;
    let mut max_update = f64::INFINITY;
    while sweeps < LAPLACE_MAX_SWEEPS {
        sweeps += 1;
        let mut biggest = 0.0;
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                let k = j * nx + i;
                let v = (wx * (u[k - 1] + u[k + 1]) + wy * (u[k - nx] + u[k + nx])) / denom;
                if !v.is_finite() {
                    return Err(format!(
                        "Gauss-Seidel produjo un valor no finito en el nodo ({i}, {j}): revisá los datos de borde"
                    ));
                }
                let d = (v - u[k]).abs();
                if d > biggest {
                    biggest = d;
                }
                u[k] = v;
            }
        }
        max_update = biggest;
        if max_update < LAPLACE_TOL {
            break;
        }
    }
    if max_update >= LAPLACE_TOL {
        return Err(format!(
            "Laplace 2D no convergió en {LAPLACE_MAX_SWEEPS} barridos (último cambio máx {max_update:.3e} ≥ tol {LAPLACE_TOL:.1e}): grilla o datos de borde incompatibles"
        ));
    }
    Ok(LaplaceGrid {
        values: u,
        nx,
        ny,
        xmin,
        xmax,
        ymin,
        ymax,
        sweeps,
        max_update,
    })
}

/// Resume la solución de Laplace 2D como texto muestreado.
///
/// Contrato del call site `geo_laplace_2d_rect` en `commands.rs`:
/// `pde::solve_laplace_2d_rect(g_sup, g_inf, g_izq, g_der, xmin, xmax, ymin, ymax)`.
#[allow(clippy::too_many_arguments)] // espejo del call site: 4 bordes + dominio
pub fn solve_laplace_2d_rect(
    g_sup: &str,
    g_inf: &str,
    g_izq: &str,
    g_der: &str,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
) -> Result<String, String> {
    let g = laplace_solve(g_sup, g_inf, g_izq, g_der, xmin, xmax, ymin, ymax)?;
    let y_mid = 0.5 * (g.ymin + g.ymax);
    let x_mid = 0.5 * (g.xmin + g.xmax);
    let j_mid = (g.ny - 1) / 2;
    let i_mid = (g.nx - 1) / 2;
    let row: Vec<f64> = (0..g.nx).map(|i| g.values[j_mid * g.nx + i]).collect();
    let col: Vec<f64> = (0..g.ny).map(|j| g.values[j * g.nx + i_mid]).collect();
    let center = g.values[j_mid * g.nx + i_mid];
    Ok(format!(
        "Laplace 2D en [{}, {}] × [{}, {}] con Dirichlet dado \
        (Gauss-Seidel {}×{}, {} barridos, cambio máx {:.1e}): \
        u(x, y = {}) = [{}] en x = [{}]; \
        u(x = {}, y) = [{}] en y = [{}]; \
        u(centro) = {}",
        fmt_num(g.xmin),
        fmt_num(g.xmax),
        fmt_num(g.ymin),
        fmt_num(g.ymax),
        g.nx,
        g.ny,
        g.sweeps,
        g.max_update,
        fmt_num(y_mid),
        sampled_values(&row, LAPLACE_SAMPLES),
        sampled_coords(g.xmin, g.xmax, LAPLACE_SAMPLES),
        fmt_num(x_mid),
        sampled_values(&col, LAPLACE_SAMPLES),
        sampled_coords(g.ymin, g.ymax, LAPLACE_SAMPLES),
        fmt_num(center),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Calor con dato `sin(π·x)`: analítica `sin(π·x)·e^{−π²·t}`.
    #[test]
    fn calor_seno_decae_como_la_analitica() {
        let s = heat_profile("sin(pi*x)", "x", 0.0, 0.1).expect("calor seno");
        assert_eq!(s.nodes, HEAT_NODES);
        assert!(s.steps > 1000, "pasos = {}", s.steps);
        assert!(s.stability <= 0.5, "r = {}", s.stability);
        let factor = (-PI * PI * 0.1).exp();
        for (x, v) in s.xs.iter().zip(s.values.iter()) {
            let esperado = (PI * x).sin() * factor;
            assert!((v - esperado).abs() < 0.01, "x = {x}: {v} vs {esperado}");
        }
    }

    /// Ondas con dato `sin(π·x)` y velocidad nula: estacionaria
    /// `sin(π·x)·cos(π·t)`; en `t = 1` vale `−sin(π·x)`.
    #[test]
    fn ondas_seno_en_t1_es_menos_seno() {
        let s = wave_profile("sin(pi*x)", "x", 0.0, 1.0).expect("ondas seno");
        assert!(s.stability <= 1.0, "C = {}", s.stability);
        for (x, v) in s.xs.iter().zip(s.values.iter()) {
            let esperado = -(PI * x).sin();
            assert!((v - esperado).abs() < 1e-6, "x = {x}: {v} vs {esperado}");
        }
    }

    /// Onda viajera: pulso gaussiano se parte en dos medios pulsos que
    /// viajan a `c = 1` (d'Alembert con velocidad inicial nula).
    #[test]
    fn ondas_pulso_viaja_a_velocidad_uno() {
        let s = wave_profile("exp(-((x-0.5)/0.05)^2)", "x", 0.0, 0.1).expect("pulso");
        let at = |x: f64| {
            let i = (x * (s.nodes as f64 - 1.0)).round() as usize;
            s.values[i]
        };
        // Los frentes llegaron a 0.4 y 0.6 con mitad de amplitud; el centro
        // quedó casi en cero.
        assert!((at(0.4) - 0.5).abs() < 0.03, "u(0.4) = {}", at(0.4));
        assert!((at(0.6) - 0.5).abs() < 0.03, "u(0.6) = {}", at(0.6));
        assert!(at(0.5).abs() < 0.05, "u(0.5) = {}", at(0.5));
    }

    /// Laplace con borde lineal: `u = x + y` es armónica, el interior la
    /// tiene que recuperar.
    #[test]
    fn laplace_borde_lineal_recupera_el_plano() {
        let g = laplace_solve("x+1", "x", "y", "1+y", 0.0, 1.0, 0.0, 1.0).expect("laplace lineal");
        assert!(g.sweeps < LAPLACE_MAX_SWEEPS);
        for j in 0..g.ny {
            for i in 0..g.nx {
                let x = g.xmin + i as f64 * (g.xmax - g.xmin) / (g.nx as f64 - 1.0);
                let y = g.ymin + j as f64 * (g.ymax - g.ymin) / (g.ny as f64 - 1.0);
                let v = g.values[j * g.nx + i];
                assert!((v - (x + y)).abs() < 1e-5, "({x}, {y}): {v} vs {}", x + y);
            }
        }
    }

    /// Laplace con borde cuadrático armónico `u = x² − y²`.
    #[test]
    fn laplace_borde_cuadratico_armonico() {
        let g = laplace_solve("x^2-1", "x^2", "0-y^2", "1-y^2", 0.0, 1.0, 0.0, 1.0)
            .expect("laplace cuadrático");
        let i_mid = (g.nx - 1) / 2;
        let j_mid = (g.ny - 1) / 2;
        let centro = g.values[j_mid * g.nx + i_mid];
        // u(0.5, 0.5) = 0.25 − 0.25 = 0.
        assert!(centro.abs() < 1e-4, "centro = {centro}");
    }

    #[test]
    fn horizonte_cero_devuelve_el_dato_inicial() {
        let s = heat_profile("x*(1-x)", "x", 0.5, 0.5).expect("horizonte cero");
        assert_eq!(s.steps, 0);
        let i = s.nodes / 2;
        assert!((s.values[i] - 0.25).abs() < 1e-12, "u = {}", s.values[i]);
        let w = wave_profile("x*(1-x)", "x", 0.5, 0.5).expect("horizonte cero");
        assert_eq!(w.steps, 0);
        let j = w.nodes / 2;
        assert!((w.values[j] - 0.25).abs() < 1e-12, "u = {}", w.values[j]);
    }

    #[test]
    fn errores_honestos_en_validacion() {
        // Hacia atrás no se integra.
        assert!(heat_profile("x", "x", 1.0, 0.5).is_err());
        assert!(wave_profile("x", "x", 1.0, 0.5).is_err());
        // Tiempos no finitos.
        assert!(heat_profile("x", "x", 0.0, f64::NAN).is_err());
        // Variable ajena.
        assert!(heat_profile("x+y", "x", 0.0, 0.1).is_err());
        // Vacías.
        assert!(heat_profile("", "x", 0.0, 0.1).is_err());
        assert!(heat_profile("x", "", 0.0, 0.1).is_err());
        // Presupuesto de expresión.
        assert!(heat_profile(&"x+".repeat(2000), "x", 0.0, 0.1).is_err());
        // Horizonte absurdo: tope de pasos antes de integrar.
        assert!(heat_profile("sin(pi*x)", "x", 0.0, 100.0).is_err());
        assert!(wave_profile("sin(pi*x)", "x", 0.0, 1e9).is_err());
        // Rectángulo degenerado.
        assert!(laplace_solve("1", "1", "1", "1", 1.0, 1.0, 0.0, 1.0).is_err());
        assert!(laplace_solve("1", "1", "1", "1", 0.0, 1.0, 1.0, 0.0).is_err());
        assert!(laplace_solve("1", "1", "1", "1", f64::NAN, 1.0, 0.0, 1.0).is_err());
        // Borde no finito en la malla (`log` de argumento ≤ 0 en todo [0, 1]).
        assert!(laplace_solve("1", "log(x-1)", "1", "1", 0.0, 1.0, 0.0, 1.0).is_err());
        // Variable ajena en el borde.
        assert!(laplace_solve("t", "1", "1", "1", 0.0, 1.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn resumenes_publicos_tienen_formato_muestreado() {
        let h = solve_heat_1d("sin(pi*x)", "x", 0.0, 0.1).expect("resumen calor");
        assert!(h.contains("u(x, t_end = 0.100000) = ["), "{h}");
        assert!(h.contains("r = "), "{h}");
        let w = solve_wave_1d("sin(pi*x)", "x", 0.0, 1.0).expect("resumen ondas");
        assert!(w.contains("u(x, t_end = 1) = ["), "{w}");
        assert!(w.contains("C = "), "{w}");
        let l = solve_laplace_2d_rect("x+1", "x", "y", "1+y", 0.0, 1.0, 0.0, 1.0)
            .expect("resumen laplace");
        assert!(l.contains("u(centro) = 1"), "{l}");
        assert!(l.contains("barridos"), "{l}");
    }
}
