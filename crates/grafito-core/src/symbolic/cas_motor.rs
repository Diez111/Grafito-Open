//! Puerta CAS G-A del cerebro (frente G-A) + F3c (2º orden, sistemas, Laplace).
//!
//! Expone el motor de `grafito-geometry` (`cas`, `integral`, `ode`) con
//! errores [`ExchangeError`] honestos: entradas malformadas van a
//! `InvalidData`; subconjuntos fuera de S/M o presupuestos agotados van a
//! `NotImplemented` con la derivación (`Eliminate`, cuadratura, diseño L).
//! Cero `unwrap` en producción; presupuestos heredados del motor
//! (2000 bytes, Taylor 64, Laurent orden 16, S-polinomios 128,
//! parciales grado ≤ 4, RHS 2º orden grado ≤ 8, Laplace n ≤ 20).

use super::exchange::ExchangeError;
use grafito_geometry::cas as geo_cas;
use grafito_geometry::integral as geo_integral;
use grafito_geometry::ode as geo_ode;
use grafito_geometry::solve as geo_solve;

fn invalid(feature: &'static str, detail: String) -> ExchangeError {
    ExchangeError::InvalidData { feature, detail }
}

fn pending(feature: &'static str, hint: String) -> ExchangeError {
    ExchangeError::NotImplemented { feature, hint }
}

fn map_cas(feature: &'static str, err: geo_cas::CasError) -> ExchangeError {
    match err {
        geo_cas::CasError::InputTooLong { provided, maximum } => invalid(
            feature,
            format!("expresión de {provided} bytes excede el máximo {maximum}"),
        ),
        geo_cas::CasError::InvalidVariable { variable } => invalid(
            feature,
            format!("variable '{variable}' no es un identificador válido"),
        ),
        geo_cas::CasError::NonFinitePoint => {
            invalid(feature, "el punto de enfoque debe ser finito".to_string())
        }
        geo_cas::CasError::Parse { reason } => invalid(
            feature,
            format!("no se pudo parsear la expresión: {reason}"),
        ),
        geo_cas::CasError::LimitDoesNotExist { detail } => {
            invalid(feature, format!("el límite no existe: {detail}"))
        }
        geo_cas::CasError::Unsupported { hint, .. } => pending(feature, hint),
        geo_cas::CasError::ResourceLimit { detail } => pending(feature, detail),
    }
}

fn map_risch(feature: &'static str, err: geo_integral::RischError) -> ExchangeError {
    match err {
        geo_integral::RischError::InputTooLong { provided, maximum } => invalid(
            feature,
            format!("integrando de {provided} bytes excede el máximo {maximum}"),
        ),
        geo_integral::RischError::InvalidVariable { variable } => invalid(
            feature,
            format!("variable '{variable}' no es un identificador válido"),
        ),
        geo_integral::RischError::Parse { reason } => {
            invalid(feature, format!("no se pudo parsear: {reason}"))
        }
        geo_integral::RischError::BadInterval { detail } => invalid(feature, detail),
        geo_integral::RischError::Unsupported { hint } => pending(feature, hint),
        geo_integral::RischError::ResourceLimit { detail } => pending(feature, detail),
    }
}

fn map_ode(feature: &'static str, err: geo_ode::OdeSymbolicError) -> ExchangeError {
    match err {
        geo_ode::OdeSymbolicError::InputTooLong { provided, maximum } => invalid(
            feature,
            format!("EDO de {provided} bytes excede el máximo {maximum}"),
        ),
        geo_ode::OdeSymbolicError::InvalidVariable { variable } => invalid(
            feature,
            format!("variable '{variable}' no es un identificador válido"),
        ),
        geo_ode::OdeSymbolicError::Parse { reason } => {
            invalid(feature, format!("no se pudo parsear: {reason}"))
        }
        geo_ode::OdeSymbolicError::NotSupported { hint } => pending(feature, hint),
        geo_ode::OdeSymbolicError::IntegrationFailed { expr } => pending(
            feature,
            format!("sin primitiva para '{expr}'; usa cuadratura numérica o reduce el sistema"),
        ),
    }
}

/// `Limit[expr, var → at]` por Gruntz (0/0, ∞/∞) + Richardson.
pub fn cas_limit_gruntz(expr: &str, var: &str, at: f64) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Limit";
    match geo_cas::gruntz_limit(expr, var, at) {
        Ok(out) => Ok(format!(
            "lim({var}→{at}) {expr} = {:.8} ({:?}/{:?})",
            out.value, out.form, out.method
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

/// `Limit[expr, var → ±∞]` por jerarquía exp/log/potencia.
pub fn cas_limit_gruntz_infinite(
    expr: &str,
    var: &str,
    positive: bool,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Limit";
    match geo_cas::gruntz_limit_infinite(expr, var, positive) {
        Ok(out) => Ok(format!(
            "lim({var}→{}∞) {expr} = {:.8} ({:?})",
            if positive { "+" } else { "-" },
            out.value,
            out.method
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

/// `Integral[expr]` por Risch-Norman (polinomios, exponenciales, logaritmos).
pub fn cas_integrate_risch(expr: &str, var: &str) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Integral";
    match geo_integral::risch_norman_integrate(expr, var) {
        Ok(prim) => Ok(format!("∫ {expr} d{var} = {prim}")),
        Err(err) => Err(map_risch(FEATURE, err)),
    }
}

/// `Integral[expr, a, b]` por FTC sobre la primitiva Risch-Norman.
pub fn cas_definite_risch(expr: &str, var: &str, a: f64, b: f64) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Integral";
    match geo_integral::risch_norman_definite(expr, var, a, b) {
        Ok(value) => Ok(format!("∫[{a},{b}] {expr} d{var} = {value:.8}")),
        Err(err) => Err(map_risch(FEATURE, err)),
    }
}

/// `SolveODE` lineal `y' + p(x)·y = q(x)` por factor integrante.
pub fn cas_solve_ode_linear(p: &str, q: &str, x: &str) -> Result<String, ExchangeError> {
    geo_ode::solve_linear_first_order(p, q, x)
        .map(|sol| format!("SolveODE[y' + ({p})*y = {q}] → {sol}"))
        .map_err(|err| map_ode("SolveODE", err))
}

/// `SolveODE` separable `y' = g(x)·h(y)`.
pub fn cas_solve_ode_separable(
    g: &str,
    h: &str,
    x: &str,
    y: &str,
) -> Result<String, ExchangeError> {
    geo_ode::solve_separable(g, h, x, y)
        .map(|sol| format!("SolveODE[y' = ({g})*({h})] → {sol}"))
        .map_err(|err| map_ode("SolveODE", err))
}

/// `SolveODE` general de 1er orden (lineal o separable; resto `Err`).
pub fn cas_solve_ode(rhs: &str, x: &str, y: &str) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_first_order(rhs, x, y)
        .map(|sol| format!("SolveODE[y' = {rhs}] → {sol}"))
        .map_err(|err| map_ode("SolveODE", err))
}

// ---------------------------------------------------------------------------
// Frente F3c: EDO 2º orden, sistemas 2×2, Laplace (puerta honesta G-A).
// ---------------------------------------------------------------------------

/// `SolveODE` de 2º orden `a·y''+b·y'+c·y = rhs` (constantes, `a ≠ 0`).
pub fn cas_solve_ode_second_order(
    a: &str,
    b: &str,
    c: &str,
    rhs: &str,
    x: &str,
) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_second_order_const(a, b, c, rhs, x)
        .map(|sol| format!("SolveODE[({a})*y''+({b})*y'+({c})*y = {rhs}] → {sol}"))
        .map_err(|err| map_ode("SolveODE", err))
}

/// Sistema lineal 2×2 constante por autovalores.
pub fn cas_solve_ode_system_2x2(
    a11: &str,
    a12: &str,
    a21: &str,
    a22: &str,
    t: &str,
) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_system_2x2(a11, a12, a21, a22, t)
        .map(|sol| format!("ODESystem[[{a11},{a12}],[{a21},{a22}]] → {sol}"))
        .map_err(|err| map_ode("ODESystem", err))
}

/// `Laplace[f(t)]` directa del subset F3c.
pub fn cas_laplace_direct(expr: &str, t: &str, s: &str) -> Result<String, ExchangeError> {
    geo_ode::laplace_direct(expr, t, s)
        .map(|out| format!("Laplace[{expr}]({t}→{s}) = {out}"))
        .map_err(|err| map_ode("Laplace", err))
}

/// `Laplace⁻¹[F(s)]` de racionales propios grado ≤ 2 (B2.3d: 3 con raíz real).
pub fn cas_laplace_inverse(expr: &str, s: &str, t: &str) -> Result<String, ExchangeError> {
    geo_ode::laplace_inverse(expr, s, t)
        .map(|out| format!("Laplace⁻¹[{expr}]({s}→{t}) = {out}"))
        .map_err(|err| map_ode("Laplace", err))
}

// ---------------------------------------------------------------------------
// Frente B2: puertas ADITIVAS (no tocan las G-A/F3c/B1 existentes).
// ---------------------------------------------------------------------------

/// `SolveODE` orden-n constante por anulador + resonancia (B2.3a).
///
/// `coeffs = [aₙ..a₀]` constantes; `n = len−1 ≤ 8`.
pub fn cas_solve_ode_nth_order(
    coeffs: &[String],
    rhs: &str,
    x: &str,
) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_nth_order_const(coeffs, rhs, x).map_err(|err| map_ode("SolveODE", err))
}

/// Euler `x²y''+a·x·y'+b·y = rhs` vía `x=eᵗ` (B2.3b, `x > 0`).
pub fn cas_solve_ode_euler(a: &str, b: &str, rhs: &str, x: &str) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_euler_2nd(a, b, rhs, x).map_err(|err| map_ode("SolveODE", err))
}

/// Serie de Frobenius en punto ordinario (B2.3c, `terms ≤ 9`).
pub fn cas_frobenius(
    p: &str,
    q: &str,
    x: &str,
    x0: f64,
    terms: usize,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Frobenius";
    match geo_ode::frobenius_series_2nd(p, q, x, x0, terms) {
        Ok(out) => Ok(format!(
            "Frobenius[{p}, {q}, {x} = {x0}] y1 = {}; y2 = {}",
            geo_ode::format_frobenius_series(&out.y1, x, x0),
            geo_ode::format_frobenius_series(&out.y2, x, x0),
        )),
        Err(err) => Err(map_ode(FEATURE, err)),
    }
}

/// `L{y⁽ⁿ⁾}` por regla con iniciales (B2.3d, `n ≤ 8`).
pub fn cas_laplace_derivative(
    order: u32,
    y: &str,
    t: &str,
    s: &str,
    initials: &[String],
) -> Result<String, ExchangeError> {
    geo_ode::laplace_derivative(order, y, t, s, initials)
        .map(|out| format!("Laplace[d^{order}{y}/d{t}^{order}] = {out}"))
        .map_err(|err| map_ode("Laplace", err))
}

/// `L{∫₀ᵗ f} = L{f}/s` (B2.3d).
pub fn cas_laplace_integral(f: &str, t: &str, s: &str) -> Result<String, ExchangeError> {
    geo_ode::laplace_integral_rule(f, t, s)
        .map(|out| format!("Laplace[∫{f}dt]({t}→{s}) = {out}"))
        .map_err(|err| map_ode("Laplace", err))
}

/// `Groebner` con orden monomial explícito (B2.4: `lex|grlex|grevlex`).
pub fn cas_groebner_ordered(
    polys: &[String],
    vars: &[String],
    order: &str,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Groebner";
    let clean = order.trim().to_ascii_lowercase();
    let ord = match clean.as_str() {
        "lex" => geo_cas::MonomialOrder::Lex,
        "grlex" => geo_cas::MonomialOrder::GrLex,
        "grevlex" => geo_cas::MonomialOrder::GrRevLex,
        _ => {
            return Err(invalid(
                FEATURE,
                format!("orden '{order}' inválido (lex|grlex|grevlex)"),
            ));
        }
    };
    match geo_cas::buchberger_basis_ordered(polys, vars, ord) {
        Ok(out) => Ok(format!(
            "Groebner[{{{}}}, {{{}}}, {clean}] = {{{}}} ({} S-polinomios)",
            polys.join(", "),
            vars.join(", "),
            out.basis.join(", "),
            out.s_polys_used
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

/// `Eliminate[polys, vars, elim]` por lex + filtrado (B2.4, intersecciones).
pub fn cas_eliminate(
    polys: &[String],
    vars: &[String],
    elim: &[String],
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Eliminate";
    match geo_cas::buchberger_eliminate(polys, vars, elim) {
        Ok(out) => Ok(format!(
            "Eliminate[{{{}}}, {{{}}}, {{{}}}] = {{{}}} ({} S-polinomios)",
            polys.join(", "),
            vars.join(", "),
            elim.join(", "),
            out.basis.join(", "),
            out.s_polys_used
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

/// `Residue[expr, var = at]` (polos simples + orden N ≤ 16).
pub fn cas_residue(
    expr: &str,
    var: &str,
    at: f64,
    max_order: usize,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Residue";
    match geo_cas::laurent_residue(expr, var, at, max_order) {
        Ok(out) => Ok(format!(
            "Residue[{expr}, {var} = {at}] = {:.8} (orden {})",
            out.residue, out.pole_order
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

/// `Series[expr]` parte principal truncada (potencias negativas).
pub fn cas_principal_part(
    expr: &str,
    var: &str,
    at: f64,
    max_order: usize,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Series";
    match geo_cas::laurent_principal_part(expr, var, at, max_order) {
        Ok(terms) => {
            if terms.is_empty() {
                return Ok(format!(
                    "Series[{expr}, {var} = {at}] parte principal vacía (analítica)"
                ));
            }
            let body: Vec<String> = terms
                .iter()
                .map(|(power, coeff)| format!("{coeff:.6}*({var}-{at})^({power})"))
                .collect();
            Ok(format!(
                "Series[{expr}, {var} = {at}] = {}",
                body.join(" + ")
            ))
        }
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

fn map_solve(feature: &'static str, err: geo_solve::SolveError) -> ExchangeError {
    match err {
        geo_solve::SolveError::InvalidInput { detail }
        | geo_solve::SolveError::NotPolynomial { hint: detail } => invalid(feature, detail),
        geo_solve::SolveError::DegreeExceeded { .. } => {
            pending(feature, format!("{err}; usa NSolve[..] o Eliminate[..]"))
        }
        geo_solve::SolveError::Unsupported { hint } => pending(feature, hint),
    }
}

/// `Solve[expr, var]` general (frente B1): todas las raíces reales.
///
/// Lineal/cuadrática exactas, resto por Sturm+bisección+Newton con cota de
/// Cauchy; grado ≥ 5 solo numérico acotado, trascendentes → `NotImplemented`
/// honesto que deriva a `NSolve`.
pub fn cas_solve_all(expr: &str, var: &str) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Solve";
    match geo_solve::solve_all_real(expr, var) {
        Ok(roots) => Ok(format!(
            "Solve[{expr}, {var}] = {}",
            geo_solve::format_real_roots(&roots)
        )),
        Err(err) => Err(map_solve(FEATURE, err)),
    }
}

/// Sistema polinómico 2×2 por eliminación (frente B1) → puntos verificados.
///
/// Fallos honestos derivan a `Groebner[..]`/`Eliminate[..]`.
pub fn cas_solve_system_2x2(
    eq1: &str,
    eq2: &str,
    x: &str,
    y: &str,
) -> Result<String, ExchangeError> {
    const FEATURE: &str = "SolveNlSystem";
    match geo_solve::solve_system_2x2(eq1, eq2, x, y) {
        Ok(points) => Ok(format!(
            "SolveNlSystem[{eq1}, {eq2}, {x}, {y}] = {}",
            geo_solve::format_system_points(&points)
        )),
        Err(err) => Err(map_solve(FEATURE, err)),
    }
}

/// `Groebner[polys, vars]` por Buchberger acotado (≤128 S-polinomios).
pub fn cas_groebner(polys: &[String], vars: &[String]) -> Result<String, ExchangeError> {
    const FEATURE: &str = "Groebner";
    match geo_cas::buchberger_basis(polys, vars) {
        Ok(out) => Ok(format!(
            "Groebner[{{{}}}, {{{}}}] = {{{}}} ({} S-polinomios)",
            polys.join(", "),
            vars.join(", "),
            out.basis.join(", "),
            out.s_polys_used
        )),
        Err(err) => Err(map_cas(FEATURE, err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_limit_zero_over_zero() {
        let out = cas_limit_gruntz("sin(x)/x", "x", 0.0).expect("puerta Limit");
        assert!(out.contains("1.00000000"), "got {out}");
    }

    #[test]
    fn gate_limit_hierarchy() {
        let out = cas_limit_gruntz_infinite("exp(x)/x^2", "x", true).expect("jerarquía");
        assert!(out.contains("inf"), "got {out}");
    }

    #[test]
    fn gate_integral_risch() {
        let out = cas_integrate_risch("x^2", "x").expect("puerta Integral");
        assert!(out.contains('x'), "got {out}");
        let err = cas_integrate_risch("exp(x^2)", "x").expect_err("fuera de S/M");
        assert!(
            matches!(err, ExchangeError::NotImplemented { .. }),
            "got {err}"
        );
    }

    #[test]
    fn gate_definite_risch() {
        let out = cas_definite_risch("x^2", "x", 0.0, 1.0).expect("FTC");
        assert!(out.contains("0.33333333"), "got {out}");
    }

    #[test]
    fn gate_solve_ode() {
        let out = cas_solve_ode_linear("2", "3", "x").expect("puerta SolveODE lineal");
        assert!(out.replace(' ', "").contains("exp(2*x)"), "got {out}");
        let sep = cas_solve_ode("x/y", "x", "y").expect("separable");
        assert!(sep.contains('C'), "got {sep}");
        let err = cas_solve_ode("y^2 + sin(x*y)", "x", "y").expect_err("no soportada");
        assert!(
            matches!(err, ExchangeError::NotImplemented { .. }),
            "got {err}"
        );
    }

    #[test]
    fn gate_residue_and_series() {
        let out = cas_residue("1/x", "x", 0.0, 8).expect("puerta Residue");
        assert!(out.contains("(orden 1)"), "got {out}");
        let series = cas_principal_part("1/x", "x", 0.0, 8).expect("puerta Series");
        assert!(series.contains("^(-1)"), "got {series}");
    }

    #[test]
    fn gate_solve_all_and_system_2x2() {
        let cubic = cas_solve_all("x^3-6x^2+11x-6", "x").expect("puerta Solve B1");
        assert!(cubic.contains("{1, 2, 3}"), "got {cubic}");
        let no_real = cas_solve_all("x^2+1", "x").expect("puerta x^2+1");
        assert!(no_real.contains("{}"), "got {no_real}");
        assert!(no_real.contains("complej"), "got {no_real}");
        let transcend = cas_solve_all("sin(x)+x", "x").expect_err("trascendente");
        assert!(
            matches!(transcend, ExchangeError::InvalidData { .. }),
            "got {transcend}"
        );
        let pts = cas_solve_system_2x2("x^2+y^2-25", "x-y-1", "x", "y").expect("puerta sistema");
        assert!(pts.contains("(4, 3)"), "got {pts}");
        let dep = cas_solve_system_2x2("x+y-1", "2*x+2*y-2", "x", "y").expect_err("dependiente");
        assert!(
            matches!(dep, ExchangeError::NotImplemented { .. }),
            "got {dep}"
        );
    }

    #[test]
    fn gate_groebner_bounded() {
        let polys = vec!["x + y - 3".to_string(), "x - y - 1".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let out = cas_groebner(&polys, &vars).expect("puerta Groebner");
        assert!(out.contains("S-polinomios"), "got {out}");
        let big: Vec<String> = (0..20).map(|i| format!("x + {i}")).collect();
        let err = cas_groebner(&big, &["x".to_string()]).expect_err("cota");
        let msg = format!("{err}");
        assert!(msg.contains("Eliminate"), "got {msg}");
    }

    // --- Frente B2: puertas ADITIVAS (aceptación 1:1, sin commit) ---

    #[test]
    fn gate_b2_ode_nth_euler_frobenius() {
        let coeffs = ["1", "-6", "11", "-6"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let nth = cas_solve_ode_nth_order(&coeffs, "0", "x").expect("puerta orden-n");
        assert!(nth.contains("exp"), "got {nth}");
        let euler = cas_solve_ode_euler("1", "-1", "0", "x").expect("puerta Euler");
        assert!(euler.contains("y = "), "got {euler}");
        let frob = cas_frobenius("-2*x", "0", "x", 0.0, 6).expect("puerta Frobenius");
        assert!(frob.contains("y1 = "), "got {frob}");
        let cubic = cas_solve_ode_nth_order(
            &["1", "0", "0", "-2"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "0",
            "x",
        )
        .expect_err("cúbica sin raíz racional");
        assert!(format!("{cubic}").contains("RKF45"), "got {cubic}");
    }

    #[test]
    fn gate_b2_laplace_calculus() {
        let d2 = cas_laplace_derivative(2, "Y", "t", "s", &["y0".to_string(), "y1".to_string()])
            .expect("puerta derivada");
        assert!(d2.contains("s^2*Y"), "got {d2}");
        let integ = cas_laplace_integral("sin(t)", "t", "s").expect("puerta integral");
        assert!(integ.contains("/s"), "got {integ}");
        let heav = cas_laplace_direct("heaviside(t-2)", "t", "s").expect("puerta Heaviside");
        assert!(heav.contains("exp(-2*s)"), "got {heav}");
        let dirac = cas_laplace_direct("dirac(t-3)", "t", "s").expect("puerta Dirac");
        assert!(dirac.contains("exp(-3*s)"), "got {dirac}");
        let inv3 = cas_laplace_inverse("1/(s^3+1)", "s", "t").expect("puerta cúbica");
        assert!(inv3.contains("exp"), "got {inv3}");
    }

    #[test]
    fn gate_b2_groebner_ordered_eliminate() {
        let polys = vec!["x^2+y^2-25".to_string(), "x-y-1".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        for order in ["lex", "grlex", "grevlex"] {
            let out = cas_groebner_ordered(&polys, &vars, order).expect("puerta orden");
            assert!(out.contains(order), "got {out}");
        }
        let bad = cas_groebner_ordered(&polys, &vars, "invlex").expect_err("orden malo");
        assert!(
            matches!(bad, ExchangeError::InvalidData { .. }),
            "got {bad}"
        );
        let elim = cas_eliminate(&polys, &vars, &["y".to_string()]).expect("puerta Eliminate");
        assert!(elim.contains("Eliminate"), "got {elim}");
        assert!(
            !elim.rsplit('=').next().unwrap_or("").contains('y'),
            "got {elim}"
        );
    }

    #[test]
    fn gate_bad_input_is_invalid_data() {
        let err = cas_limit_gruntz(&"x".repeat(5000), "x", 0.0).expect_err("entrada larga");
        assert!(
            matches!(err, ExchangeError::InvalidData { .. }),
            "got {err}"
        );
    }

    // --- Frente F3c: puerta de 2º orden, sistemas y Laplace ---

    #[test]
    fn gate_solve_ode_second_order() {
        let out = cas_solve_ode_second_order("1", "-3", "2", "exp(x)", "x").expect("puerta EDO2");
        assert!(out.contains('C'), "got {out}");
        assert!(out.replace(' ', "").contains("exp(1*x)"), "got {out}");
        let err = cas_solve_ode_second_order("0", "1", "1", "x", "x").expect_err("a=0");
        assert!(
            matches!(err, ExchangeError::NotImplemented { .. }),
            "got {err}"
        );
        let euler = cas_solve_ode_second_order("x^2", "x", "1", "0", "x").expect_err("Euler");
        let msg = format!("{euler}");
        assert!(msg.contains("constante"), "got {msg}");
    }

    #[test]
    fn gate_solve_ode_system_2x2() {
        let out = cas_solve_ode_system_2x2("0", "1", "-2", "-3", "t").expect("puerta sistema");
        assert!(out.contains("exp(-1*t)"), "got {out}");
        let err = cas_solve_ode_system_2x2("t", "1", "0", "1", "t").expect_err("variable");
        assert!(
            matches!(err, ExchangeError::NotImplemented { .. }),
            "got {err}"
        );
    }

    #[test]
    fn gate_laplace_direct_and_inverse() {
        let direct = cas_laplace_direct("sin(t)", "t", "s").expect("puerta Laplace");
        assert!(direct.contains("s^2+1"), "got {direct}");
        let inverse = cas_laplace_inverse("1/(s+1)", "s", "t").expect("puerta inversa");
        assert!(inverse.contains("exp(-1*t)"), "got {inverse}");
        // B2.3d: la cúbica con raíz real YA resuelve; grado 4 sigue honesto.
        let cubic = cas_laplace_inverse("1/(s^3+1)", "s", "t").expect("puerta cúbica");
        assert!(cubic.contains("exp(-1*t)"), "got {cubic}");
        let err = cas_laplace_inverse("1/(s^4+1)", "s", "t").expect_err("grado 4");
        assert!(
            matches!(err, ExchangeError::NotImplemented { .. }),
            "got {err}"
        );
        // `laplace_pdf` es la densidad estadística, no la transformada:
        // la puerta Laplace vive aquí, no en `statistics`.
        let stats = grafito_geometry::statistics::laplace_pdf(0.0, 0.0, 1.0);
        assert!(
            (stats - 0.5).abs() < 1e-12,
            "densidad Laplace(0,0,1)=0.5, got {stats}"
        );
    }
}
