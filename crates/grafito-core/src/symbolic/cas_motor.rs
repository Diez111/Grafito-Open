//! Puerta CAS G-A del cerebro (frente G-A).
//!
//! Expone el motor de `grafito-geometry` (`cas`, `integral`, `ode`) con
//! errores [`ExchangeError`] honestos: entradas malformadas van a
//! `InvalidData`; subconjuntos fuera de S/M o presupuestos agotados van a
//! `NotImplemented` con la derivación (`Eliminate`, cuadratura, diseño L).
//! Cero `unwrap` en producción; presupuestos heredados del motor
//! (2000 bytes, Taylor 64, Laurent orden 16, S-polinomios 128).

use super::exchange::ExchangeError;
use grafito_geometry::cas as geo_cas;
use grafito_geometry::integral as geo_integral;
use grafito_geometry::ode as geo_ode;

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

fn map_ode(err: geo_ode::OdeSymbolicError) -> ExchangeError {
    const FEATURE: &str = "SolveODE";
    match err {
        geo_ode::OdeSymbolicError::InputTooLong { provided, maximum } => invalid(
            FEATURE,
            format!("EDO de {provided} bytes excede el máximo {maximum}"),
        ),
        geo_ode::OdeSymbolicError::InvalidVariable { variable } => invalid(
            FEATURE,
            format!("variable '{variable}' no es un identificador válido"),
        ),
        geo_ode::OdeSymbolicError::Parse { reason } => {
            invalid(FEATURE, format!("no se pudo parsear: {reason}"))
        }
        geo_ode::OdeSymbolicError::NotSupported { hint } => pending(FEATURE, hint),
        geo_ode::OdeSymbolicError::IntegrationFailed { expr } => pending(
            FEATURE,
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
        .map_err(map_ode)
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
        .map_err(map_ode)
}

/// `SolveODE` general de 1er orden (lineal o separable; resto `Err`).
pub fn cas_solve_ode(rhs: &str, x: &str, y: &str) -> Result<String, ExchangeError> {
    geo_ode::solve_ode_first_order(rhs, x, y)
        .map(|sol| format!("SolveODE[y' = {rhs}] → {sol}"))
        .map_err(map_ode)
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

    #[test]
    fn gate_bad_input_is_invalid_data() {
        let err = cas_limit_gruntz(&"x".repeat(5000), "x", 0.0).expect_err("entrada larga");
        assert!(
            matches!(err, ExchangeError::InvalidData { .. }),
            "got {err}"
        );
    }
}
