//! Grafito Symbolic CAS — Computer Algebra System propio sobre el AST nativo.
//!
//! Implementación propia de derivación simbólica, integración (simbólica básica
//! y numérica), límites (Richardson), simplificación algebraica, expansión,
//! factorización, series de Taylor y resolución de ecuaciones polinómicas
//! (lineal, cuadrática, cúbica vía Cardano, cuártica vía Ferrari, y Newton
//! numérico para grados mayores).
//!
//! Todo el cómputo se realiza sobre el `Expr` de `crate::ast` (parseo, evaluación
//! numérica `eval_at`, impresión `to_expr_string`). No se utiliza `evalexpr` ni
//! el módulo `crate::expr` en absoluto: el CAS es 100% nativo del AST de Grafito.

use crate::assumptions::{Assumption, Assumptions};
use crate::ast::{parse_ast, Expr};
use crate::exact::{ExactRational, ExactRationalError};
use crate::{MathError, MathOperation, MathResult, MAX_MATH_INPUT_BYTES};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt;

const MAX_EXPAND_AST_NODES: usize = 32_768;
const MAX_EXPAND_WORK_UNITS: usize = 131_072;
const MAX_EXPAND_OUTPUT_BYTES: usize = 100_000;

// ============================================================================
// API pública
// ============================================================================

/// Resultado de una simplificación que puede requerir hipótesis de dominio.
///
/// La variante condicional nunca borra una restricción: por ejemplo, `x / x`
/// pasa a `1` únicamente bajo `NonZero("x")`.
#[derive(Clone, Debug, PartialEq)]
pub enum SimplificationOutcome {
    /// La expresión simplificada es válida sin hipótesis adicionales.
    Unconditional(Expr),
    /// La expresión simplificada requiere las condiciones declaradas.
    Conditional(ConditionalExpr),
}

/// Expresión simplificada junto con las hipótesis necesarias para su validez.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalExpr {
    /// Expresión equivalente bajo [`Self::conditions`].
    pub expression: Expr,
    /// Restricciones de dominio que no pueden descartarse.
    pub conditions: BTreeSet<Assumption>,
}

/// Error al intentar evaluar sintácticamente una expresión como racional exacto.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactEvaluationError {
    /// La expresión no pudo convertirse al AST de Grafito.
    Parse(String),
    /// Una operación racional exacta no está definida o excede `i128`.
    Arithmetic(ExactRationalError),
}

impl fmt::Display for ExactEvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "no se pudo parsear la expresión exacta: {reason}"),
            Self::Arithmetic(error) => error.fmt(f),
        }
    }
}

impl Error for ExactEvaluationError {}

impl From<ExactRationalError> for ExactEvaluationError {
    fn from(error: ExactRationalError) -> Self {
        Self::Arithmetic(error)
    }
}

/// Evalúa una expresión aritmética entera como racional exacto, cuando es posible.
///
/// La gramática continúa siendo la del AST de Grafito, por compatibilidad. Como
/// sus literales ya se almacenan como `f64`, esta API acepta solo constantes
/// enteras representables exactamente por `f64`; para literales `i128` completos
/// use `ExactRational::from_str`. Variables y funciones devuelven `Ok(None)`.
pub fn evaluate_exact_rational(expr: &str) -> Result<Option<ExactRational>, ExactEvaluationError> {
    let ast = parse_ast(&expr.replace(' ', "")).map_err(ExactEvaluationError::Parse)?;
    exact_rational_from_expr(&ast)
}

/// Simplifica identidades seguras bajo `assumptions` sin modificar la API `f64`
/// legacy de [`simplify`]. Cuando una identidad elimina un posible punto fuera de
/// dominio, devuelve una condición explícita en lugar de afirmarla globalmente.
#[must_use]
pub fn simplify_with_assumptions(expr: &Expr, assumptions: &Assumptions) -> SimplificationOutcome {
    let (expression, conditions) = simplify_conditionally(expr, assumptions);
    if conditions.is_empty() {
        SimplificationOutcome::Unconditional(expression)
    } else {
        SimplificationOutcome::Conditional(ConditionalExpr {
            expression,
            conditions,
        })
    }
}

fn exact_rational_from_expr(
    expression: &Expr,
) -> Result<Option<ExactRational>, ExactEvaluationError> {
    use Expr::*;

    let exact_binary = |left: &Expr,
                        right: &Expr,
                        operation: fn(
        ExactRational,
        ExactRational,
    ) -> Result<ExactRational, ExactRationalError>| {
        let Some(left) = exact_rational_from_expr(left)? else {
            return Ok(None);
        };
        let Some(right) = exact_rational_from_expr(right)? else {
            return Ok(None);
        };
        operation(left, right).map(Some).map_err(Into::into)
    };

    match expression {
        Const(value) => Ok(exact_integer_constant(*value).map(ExactRational::from)),
        Neg(value) => exact_rational_from_expr(value)?
            .map(ExactRational::checked_neg)
            .transpose()
            .map_err(Into::into),
        Add(left, right) => exact_binary(left, right, ExactRational::checked_add),
        Sub(left, right) => exact_binary(left, right, ExactRational::checked_sub),
        Mul(left, right) => exact_binary(left, right, ExactRational::checked_mul),
        Div(left, right) => exact_binary(left, right, ExactRational::checked_div),
        _ => Ok(None),
    }
}

fn exact_integer_constant(value: f64) -> Option<i128> {
    // `f64` can preserve every integer only through 2^53. Larger constants
    // belong to ExactRational::from_str, before the AST erases their digits.
    const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;
    if value.is_finite()
        && value.fract() == 0.0
        && (-MAX_EXACT_F64_INTEGER..=MAX_EXACT_F64_INTEGER).contains(&value)
    {
        Some(value as i128)
    } else {
        None
    }
}

fn simplify_conditionally(
    expression: &Expr,
    assumptions: &Assumptions,
) -> (Expr, BTreeSet<Assumption>) {
    use Expr::*;

    match expression {
        Const(_) | Var(_) => (expression.clone(), BTreeSet::new()),
        Neg(value) => {
            let (value, conditions) = simplify_conditionally(value, assumptions);
            match value {
                Const(value) => (Const(-value), conditions),
                Neg(inner) => (*inner, conditions),
                value => (Neg(Box::new(value)), conditions),
            }
        }
        Add(left, right) => {
            let (left, mut conditions) = simplify_conditionally(left, assumptions);
            let (right, right_conditions) = simplify_conditionally(right, assumptions);
            conditions.extend(right_conditions);
            match (&left, &right) {
                (Const(left), Const(right)) => (Const(left + right), conditions),
                (Const(0.0), _) => (right, conditions),
                (_, Const(0.0)) => (left, conditions),
                _ => (Add(Box::new(left), Box::new(right)), conditions),
            }
        }
        Sub(left, right) => {
            let (left, mut conditions) = simplify_conditionally(left, assumptions);
            let (right, right_conditions) = simplify_conditionally(right, assumptions);
            conditions.extend(right_conditions);
            match (&left, &right) {
                (Const(left), Const(right)) => (Const(left - right), conditions),
                (_, Const(0.0)) => (left, conditions),
                _ => (Sub(Box::new(left), Box::new(right)), conditions),
            }
        }
        Mul(left, right) => {
            let (left, mut conditions) = simplify_conditionally(left, assumptions);
            let (right, right_conditions) = simplify_conditionally(right, assumptions);
            conditions.extend(right_conditions);
            match (&left, &right) {
                (Const(left), Const(right)) => (Const(left * right), conditions),
                (Const(1.0), _) => (right, conditions),
                (_, Const(1.0)) => (left, conditions),
                (Const(-1.0), _) => (Neg(Box::new(right)), conditions),
                (_, Const(-1.0)) => (Neg(Box::new(left)), conditions),
                // Do not fold 0 * f to zero: f can still be undefined.
                _ => (Mul(Box::new(left), Box::new(right)), conditions),
            }
        }
        Div(left, right) => {
            let (left, mut conditions) = simplify_conditionally(left, assumptions);
            let (right, right_conditions) = simplify_conditionally(right, assumptions);
            conditions.extend(right_conditions);
            match (&left, &right) {
                (_, Const(0.0)) => (Div(Box::new(left), Box::new(right)), conditions),
                (Const(left), Const(right)) => (Const(left / right), conditions),
                (_, Const(1.0)) => (left, conditions),
                (Const(0.0), Var(variable)) => {
                    require_nonzero(variable, assumptions, &mut conditions);
                    (Const(0.0), conditions)
                }
                (Var(left), Var(right)) if left == right => {
                    require_nonzero(left, assumptions, &mut conditions);
                    (Const(1.0), conditions)
                }
                _ => (Div(Box::new(left), Box::new(right)), conditions),
            }
        }
        Pow(base, exponent) => {
            let (base, mut conditions) = simplify_conditionally(base, assumptions);
            let (exponent, exponent_conditions) = simplify_conditionally(exponent, assumptions);
            conditions.extend(exponent_conditions);
            match (&base, &exponent) {
                (Const(0.0), Const(0.0)) => (Pow(Box::new(base), Box::new(exponent)), conditions),
                (Const(base), Const(exponent)) if *base != 0.0 || *exponent > 0.0 => {
                    (Const(base.powf(*exponent)), conditions)
                }
                (_, Const(1.0)) => (base, conditions),
                (Var(variable), Const(0.0)) => {
                    require_nonzero(variable, assumptions, &mut conditions);
                    (Const(1.0), conditions)
                }
                _ => (Pow(Box::new(base), Box::new(exponent)), conditions),
            }
        }
        // Keeping unsupported forms intact is intentional: this additive API
        // only applies identities whose domain proof is represented above.
        _ => (expression.clone(), BTreeSet::new()),
    }
}

fn require_nonzero(
    variable: &str,
    assumptions: &Assumptions,
    conditions: &mut BTreeSet<Assumption>,
) {
    if !assumptions.is_nonzero(variable) {
        conditions.insert(Assumption::NonZero(variable.to_owned()));
    }
}

/// Derivada simbólica tipada de `expr` respecto de `var`.
///
/// Las derivadas construidas por reglas simbólicas se devuelven como
/// [`MathResult::Exact`]. Las derivadas que requieren funciones aún no
/// representables por el AST se devuelven como [`MathResult::Unsupported`].
pub fn derivative_typed(expr: &str, var: &str) -> MathResult<String> {
    let ast = match parse_math_expression(expr, MathOperation::SymbolicDerivative) {
        Ok(ast) => ast,
        Err(error) => return math_failure(error),
    };
    let mut variables = HashSet::new();
    ast.get_variables(&mut variables);
    if ast.to_expr_string().contains("trigamma(") && variables.contains(var) {
        return MathResult::Unsupported(MathError::DerivativeUnavailable {
            expression: expr.into(),
            variable: var.into(),
            reason: "requiere polygamma de orden superior".into(),
        });
    }
    let d = simplify_expr(&diff_expr(&ast, var));
    MathResult::Exact(d.to_expr_string())
}

/// Adaptador compatible de [`derivative_typed`] para consumidores que esperan
/// mensajes de error como texto.
pub fn derivative(expr: &str, var: &str) -> Result<String, String> {
    adapt_symbolic_result(derivative_typed(expr, var))
}

/// Integración indefinida simbólica básica.
///
/// Soporta: constantes, potencias (regla de la potencia, incluida 1/x → ln|x|),
/// linealidad, constante por función, sin(x) → -cos(x), cos(x) → sin(x),
/// exp(x) → exp(x), ln(x) → x·ln(x) − x. Para casos más complejos cae al
/// integrador simbólico nativo (`Expr::integrate`). Si no hay una primitiva
/// soportada devuelve un error; una cuadratura sobre un intervalo arbitrario
/// no representa una integral indefinida.
pub fn integrate_typed(expr: &str, var: &str) -> MathResult<String> {
    let ast = match parse_math_expression(expr, MathOperation::IndefiniteIntegration) {
        Ok(ast) => ast,
        Err(error) => return math_failure(error),
    };
    if let Some(prim) = integrate_expr(&ast, var).or_else(|| ast.integrate(var)) {
        return MathResult::Exact(simplify_expr(&prim).to_expr_string());
    }
    MathResult::Unsupported(MathError::AntiderivativeUnavailable {
        expression: expr.into(),
        variable: var.into(),
    })
}

/// Adaptador compatible de [`integrate_typed`] para consumidores que esperan
/// mensajes de error como texto.
pub fn integrate(expr: &str, var: &str) -> Result<String, String> {
    adapt_symbolic_result(integrate_typed(expr, var))
}

/// Integral definida de `a` a `b`.
///
/// Intenta antidiferenciación simbólica y evalúa en los extremos; si no es
/// posible, usa cuadratura numérica (Simpson adaptativo) sobre el AST.
pub fn integrate_definite_typed(expr: &str, var: &str, a: f64, b: f64) -> MathResult<f64> {
    let ast = match parse_math_expression(expr, MathOperation::DefiniteIntegration) {
        Ok(ast) => ast,
        Err(error) => return math_failure(error),
    };
    if has_potential_interval_domain_error(&ast, var, a, b) {
        return MathResult::DomainError(MathError::IntervalDomainViolation {
            expression: expr.into(),
            variable: var.into(),
            lower: a,
            upper: b,
        });
    }
    // Validate the interval before accepting an otherwise finite endpoint
    // difference from an antiderivative, which can hide interior poles.
    let numeric = match numeric_integrate(&ast, expr, var, a, b, 1e-10, 20) {
        MathResult::Approximate {
            value,
            error_estimate,
        } => (value, error_estimate),
        MathResult::DomainError(error) => return MathResult::DomainError(error),
        MathResult::NotConverged(error) => return MathResult::NotConverged(error),
        MathResult::Unsupported(error) => return MathResult::Unsupported(error),
        MathResult::ResourceLimit(error) => return MathResult::ResourceLimit(error),
        MathResult::Exact(_) => unreachable!("numeric integration is never exact"),
    };
    if let Some(prim) = integrate_expr(&ast, var).or_else(|| ast.integrate(var)) {
        let prim = simplify_expr(&prim);
        let fa = prim.eval_at(var, a);
        let fb = prim.eval_at(var, b);
        if fa.is_finite() && fb.is_finite() {
            return MathResult::Exact(fb - fa);
        }
    }
    MathResult::Approximate {
        value: numeric.0,
        error_estimate: numeric.1,
    }
}

/// Integral definida numérica con tolerancia y profundidad explícitas.
///
/// El resultado usa Simpson adaptativo y siempre es [`MathResult::Approximate`]
/// al converger; no se declara exactitud para una cuadratura.
pub fn integrate_numerical_with_limits(
    expr: &str,
    var: &str,
    a: f64,
    b: f64,
    tolerance: f64,
    max_depth: u32,
) -> MathResult<f64> {
    let ast = match parse_math_expression(expr, MathOperation::NumericalIntegration) {
        Ok(ast) => ast,
        Err(error) => return math_failure(error),
    };
    if has_potential_interval_domain_error(&ast, var, a, b) {
        return MathResult::DomainError(MathError::IntervalDomainViolation {
            expression: expr.into(),
            variable: var.into(),
            lower: a,
            upper: b,
        });
    }
    numeric_integrate(&ast, expr, var, a, b, tolerance, max_depth)
}

/// Integral definida numérica con la tolerancia `1e-10` y profundidad máxima 20.
pub fn integrate_numerical(expr: &str, var: &str, a: f64, b: f64) -> MathResult<f64> {
    integrate_numerical_with_limits(expr, var, a, b, 1e-10, 20)
}

/// Adaptador compatible de [`integrate_definite_typed`] para consumidores que
/// esperan una descripción textual de la integral.
pub fn integrate_definite(expr: &str, var: &str, a: f64, b: f64) -> Result<String, String> {
    match integrate_definite_typed(expr, var, a, b) {
        MathResult::Exact(value) => Ok(format!("\u{222b}[{a},{b}] {expr} d{var} = {value:.8}")),
        MathResult::Approximate { value, .. } => Ok(format!(
            "\u{222b}[{a},{b}] {expr} d{var} \u{2248} {value:.8}"
        )),
        result => Err(legacy_error(result)),
    }
}

fn parse_math_expression(expr: &str, operation: MathOperation) -> Result<Expr, MathError> {
    if expr.len() > MAX_MATH_INPUT_BYTES {
        return Err(MathError::InputTooLarge {
            operation,
            provided_bytes: expr.len(),
            maximum_bytes: MAX_MATH_INPUT_BYTES,
        });
    }
    let normalized = expr.replace(' ', "");
    parse_ast(&normalized).map_err(|reason| MathError::InvalidExpression {
        operation,
        expression: expr.into(),
        reason,
    })
}

fn math_failure<T>(error: MathError) -> MathResult<T> {
    match error {
        error @ MathError::InputTooLarge { .. } => MathResult::ResourceLimit(error),
        error @ MathError::DerivativeUnavailable { .. }
        | error @ MathError::AntiderivativeUnavailable { .. } => MathResult::Unsupported(error),
        error @ MathError::RecursionLimit { .. } => MathResult::NotConverged(error),
        error => MathResult::DomainError(error),
    }
}

fn adapt_symbolic_result(result: MathResult<String>) -> Result<String, String> {
    match result {
        MathResult::Exact(value) => Ok(value),
        result => Err(legacy_error(result)),
    }
}

fn legacy_error<T>(result: MathResult<T>) -> String {
    match result {
        MathResult::DomainError(MathError::InvalidExpression {
            operation: MathOperation::SymbolicDerivative,
            expression,
            reason,
        }) => format!("No se pudo derivar '{expression}': {reason}"),
        MathResult::DomainError(MathError::InvalidExpression {
            operation: MathOperation::IndefiniteIntegration | MathOperation::DefiniteIntegration,
            expression,
            reason,
        }) => format!("No se pudo integrar '{expression}': {reason}"),
        MathResult::Unsupported(MathError::DerivativeUnavailable { .. }) => {
            "La derivada de trigamma requiere polygamma de orden superior".into()
        }
        MathResult::Unsupported(MathError::AntiderivativeUnavailable {
            expression,
            variable,
        }) => format!(
            "No hay una antiderivada simbólica soportada para '{expression}' respecto de {variable}"
        ),
        MathResult::DomainError(MathError::IntervalDomainViolation { .. }) => {
            "La integral definida puede no estar definida en el intervalo".into()
        }
        MathResult::DomainError(MathError::NonFiniteValue { variable, at, .. }) => format!(
            "La integral definida no es finita: la expresión no está definida en {variable} = {at}"
        ),
        MathResult::NotConverged(MathError::RecursionLimit { .. }) => {
            "La integral definida no convergió dentro del límite de recursión".into()
        }
        MathResult::ResourceLimit(MathError::InputTooLarge {
            provided_bytes,
            maximum_bytes,
            ..
        }) => format!(
            "La expresión excede el límite de entrada: {provided_bytes} bytes (máximo {maximum_bytes})"
        ),
        MathResult::Unsupported(MathError::NonFiniteLimitPoint { .. }) => {
            "Los límites en infinito no están soportados todavía".into()
        }
        _ => "La operación matemática no pudo completarse".into(),
    }
}

/// Conservatively detects rational and elementary-domain failures that a
/// quadrature grid could miss between sampled points.
fn has_potential_interval_domain_error(ast: &Expr, var: &str, a: f64, b: f64) -> bool {
    use Expr::*;
    match ast {
        Div(numerator, denominator) => {
            interval_bounds(denominator, var, a, b).is_some_and(|(lo, hi)| lo <= 0.0 && hi >= 0.0)
                || has_potential_interval_domain_error(numerator, var, a, b)
                || has_potential_interval_domain_error(denominator, var, a, b)
        }
        Ln(argument) | Log(argument) => {
            interval_bounds(argument, var, a, b).is_some_and(|(lo, _)| lo <= 0.0)
                || has_potential_interval_domain_error(argument, var, a, b)
        }
        Sqrt(argument) => {
            interval_bounds(argument, var, a, b).is_some_and(|(lo, _)| lo < 0.0)
                || has_potential_interval_domain_error(argument, var, a, b)
        }
        Neg(argument) | Sin(argument) | Cos(argument) | Tan(argument) | Asin(argument)
        | Acos(argument) | Atan(argument) | Exp(argument) | Abs(argument) | Sinh(argument)
        | Cosh(argument) | Tanh(argument) | Floor(argument) | Ceil(argument) | Round(argument)
        | Sec(argument) | Csc(argument) | Cot(argument) | Asinh(argument) | Acosh(argument)
        | Atanh(argument) | Sign(argument) | Heaviside(argument) | Cbrt(argument)
        | Re(argument) | Im(argument) | Arg(argument) | Conj(argument) | Erf(argument)
        | Erfc(argument) | Gamma(argument) | LnGamma(argument) | Digamma(argument)
        | Trigamma(argument) => has_potential_interval_domain_error(argument, var, a, b),
        Add(left, right)
        | Sub(left, right)
        | Mul(left, right)
        | Pow(left, right)
        | Atan2(left, right)
        | Modulo(left, right)
        | Min(left, right)
        | Max(left, right)
        | Beta(left, right)
        | BesselJ(left, right)
        | BesselY(left, right)
        | BesselI(left, right)
        | Lt(left, right)
        | Gt(left, right)
        | Le(left, right)
        | Ge(left, right)
        | Eq(left, right)
        | Ne(left, right) => {
            has_potential_interval_domain_error(left, var, a, b)
                || has_potential_interval_domain_error(right, var, a, b)
        }
        Clamp(value, lo, hi) => {
            has_potential_interval_domain_error(value, var, a, b)
                || has_potential_interval_domain_error(lo, var, a, b)
                || has_potential_interval_domain_error(hi, var, a, b)
        }
        Sum(body, _, start, end) | Product(body, _, start, end) => {
            has_potential_interval_domain_error(body, var, a, b)
                || has_potential_interval_domain_error(start, var, a, b)
                || has_potential_interval_domain_error(end, var, a, b)
        }
        Piecewise(branches, default) => {
            branches.iter().any(|(condition, value)| {
                has_potential_interval_domain_error(condition, var, a, b)
                    || has_potential_interval_domain_error(value, var, a, b)
            }) || has_potential_interval_domain_error(default, var, a, b)
        }
        Const(_) | Var(_) => false,
    }
}

fn interval_bounds(ast: &Expr, var: &str, a: f64, b: f64) -> Option<(f64, f64)> {
    use Expr::*;
    let (a, b) = (a.min(b), a.max(b));
    match ast {
        Const(value) if value.is_finite() => Some((*value, *value)),
        Var(name) if name == var => Some((a, b)),
        Neg(argument) => interval_bounds(argument, var, a, b).map(|(lo, hi)| (-hi, -lo)),
        Add(left, right) => {
            let (left_lo, left_hi) = interval_bounds(left, var, a, b)?;
            let (right_lo, right_hi) = interval_bounds(right, var, a, b)?;
            Some((left_lo + right_lo, left_hi + right_hi))
        }
        Sub(left, right) => {
            let (left_lo, left_hi) = interval_bounds(left, var, a, b)?;
            let (right_lo, right_hi) = interval_bounds(right, var, a, b)?;
            Some((left_lo - right_hi, left_hi - right_lo))
        }
        Mul(left, right) => {
            let (left_lo, left_hi) = interval_bounds(left, var, a, b)?;
            let (right_lo, right_hi) = interval_bounds(right, var, a, b)?;
            let products = [
                left_lo * right_lo,
                left_lo * right_hi,
                left_hi * right_lo,
                left_hi * right_hi,
            ];
            Some((
                products.iter().copied().fold(f64::INFINITY, f64::min),
                products.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ))
        }
        Pow(base, exponent) => {
            let (lo, hi) = interval_bounds(base, var, a, b)?;
            let exponent = match exponent.as_ref() {
                Const(value) if value.is_finite() && *value >= 0.0 && value.fract() == 0.0 => {
                    *value as i32
                }
                _ => return None,
            };
            let left = lo.powi(exponent);
            let right = hi.powi(exponent);
            if exponent % 2 == 0 && lo <= 0.0 && hi >= 0.0 {
                Some((0.0, left.max(right)))
            } else {
                Some((left.min(right), left.max(right)))
            }
        }
        Exp(argument) => interval_bounds(argument, var, a, b).map(|(lo, hi)| (lo.exp(), hi.exp())),
        Abs(argument) => interval_bounds(argument, var, a, b).map(|(lo, hi)| {
            if lo <= 0.0 && hi >= 0.0 {
                (0.0, lo.abs().max(hi.abs()))
            } else {
                (lo.abs().min(hi.abs()), lo.abs().max(hi.abs()))
            }
        }),
        _ => None,
    }
}

/// Límite finito de `expr` cuando `var → at`.
///
/// La estimación usa extrapolación bilateral de Richardson. Un valor finito
/// siempre informa una estimación de error; la falta de coincidencia lateral
/// no se codifica como un mensaje de éxito.
pub fn limit_typed(expr: &str, var: &str, at: f64) -> MathResult<f64> {
    if !at.is_finite() {
        return MathResult::Unsupported(MathError::NonFiniteLimitPoint {
            expression: expr.into(),
            variable: var.into(),
            at,
        });
    }
    let ast = match parse_math_expression(expr, MathOperation::Limit) {
        Ok(ast) => ast,
        Err(error) => return math_failure(error),
    };
    if has_proven_squeezed_zero_limit(&ast, var, at) {
        return MathResult::Approximate {
            value: 0.0,
            error_estimate: 0.0,
        };
    }
    if has_unresolved_oscillatory_singularity(&ast, var, at) {
        return MathResult::DomainError(MathError::LimitDoesNotExist {
            expression: expr.into(),
            variable: var.into(),
            at,
        });
    }
    match richardson_limit(&ast, var, at) {
        Some(estimate) => MathResult::Approximate {
            value: estimate.value,
            error_estimate: estimate.error_estimate,
        },
        None => MathResult::DomainError(MathError::LimitDoesNotExist {
            expression: expr.into(),
            variable: var.into(),
            at,
        }),
    }
}

/// Adaptador compatible de [`limit_typed`] para consumidores que esperan un
/// mensaje legible. La inexistencia del límite conserva el texto histórico.
pub fn limit(expr: &str, var: &str, at: f64) -> Result<String, String> {
    match limit_typed(expr, var, at) {
        MathResult::Exact(value) | MathResult::Approximate { value, .. } => {
            Ok(format!("lim({var}\u{2192}{at}) {expr} = {value:.8}"))
        }
        MathResult::DomainError(MathError::LimitDoesNotExist { .. }) => Ok(format!(
            "lim({var}\u{2192}{at}) {expr} = no existe (o es \u{221e})"
        )),
        result => Err(legacy_error(result)),
    }
}

/// Expande productos y potencias enteras no negativas por distributividad.
///
/// Las potencias cero permanecen intactas para no borrar el caso indefinido
/// `0^0`. La operación falla si el AST, el trabajo acumulado o la salida
/// proyectada exceden sus presupuestos, antes de materializar una expansión
/// potencialmente explosiva.
pub fn expand(expr: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo expandir '{expr}': {e}"))?;
    validate_expand_expression(&ast)?;

    let mut budget = ExpandBudget::default();
    let terms = expand_expr(&ast, &mut budget)?;
    let expanded = expanded_terms_into_expr(terms, &mut budget)?;
    let simplified = simplify_expanded_expr(expanded, &mut budget)?;
    validate_expand_expression(&simplified)?;

    let output = simplified.to_expr_string();
    if output.len() > MAX_EXPAND_OUTPUT_BYTES {
        return Err(expand_budget_error(
            "output",
            output.len(),
            MAX_EXPAND_OUTPUT_BYTES,
        ));
    }
    Ok(output)
}

/// Factoriza polinomios lineales y cuadráticos preservando equivalencia.
///
/// Para expresiones fuera de este subconjunto devuelve la expresión original:
/// es preferible no factorizar a publicar factores numéricos incompletos.
pub fn factor(expr: &str, var: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo factorizar '{expr}': {e}"))?;
    let simplified = simplify_expr(&ast);
    let Some(coeffs) = collect_polynomial_coeffs(&simplified, var, 2) else {
        return Ok(pp);
    };
    if coeffs.iter().any(|coefficient| !coefficient.is_finite()) {
        return Ok(pp);
    }
    let degree = coeffs.iter().rposition(|coefficient| *coefficient != 0.0);
    let Some(degree) = degree else {
        return Ok("0".to_string());
    };

    let factors = match degree {
        1 => {
            let leading = coeffs[1];
            let root = -coeffs[0] / leading;
            if !root.is_finite()
                || (coeffs[0] != 0.0 && root == 0.0)
                || !preserves_linear_coefficients(leading, root, coeffs[0])
            {
                return Ok(pp);
            }
            vec![leading.to_string(), linear_factor(var, root)]
        }
        2 => {
            let leading = coeffs[2];
            let scale = coeffs[..=2]
                .iter()
                .map(|coefficient| coefficient.abs())
                .fold(0.0, f64::max);
            if scale == 0.0 || !scale.is_finite() {
                return Ok(pp);
            }
            let a = leading / scale;
            let b = coeffs[1] / scale;
            let c = coeffs[0] / scale;
            let discriminant = b.mul_add(b, -4.0 * a * c);
            if !discriminant.is_finite() || discriminant < 0.0 {
                return Ok(pp);
            }
            let root_delta = discriminant.sqrt();
            let q = if b >= 0.0 {
                -0.5 * (b + root_delta)
            } else {
                -0.5 * (b - root_delta)
            };
            let (first, second) = if q == 0.0 {
                let root = -b / (2.0 * a);
                (root, root)
            } else {
                (q / a, c / q)
            };
            if !first.is_finite()
                || !second.is_finite()
                || (coeffs[0] != 0.0 && (first == 0.0 || second == 0.0))
                || !preserves_quadratic_coefficients(leading, first, second, &coeffs)
            {
                return Ok(pp);
            }
            vec![
                leading.to_string(),
                linear_factor(var, first),
                linear_factor(var, second),
            ]
        }
        _ => return Ok(pp),
    };

    Ok(factors.join(" * "))
}

fn preserves_linear_coefficients(leading: f64, root: f64, constant: f64) -> bool {
    -leading * root == constant
}

fn preserves_quadratic_coefficients(
    leading: f64,
    first_root: f64,
    second_root: f64,
    coeffs: &[f64],
) -> bool {
    leading == coeffs[2]
        && -(leading * first_root + leading * second_root) == coeffs[1]
        && leading * first_root * second_root == coeffs[0]
}

fn linear_factor(var: &str, root: f64) -> String {
    if root == 0.0 {
        var.to_string()
    } else if root > 0.0 {
        format!("({var} - {root})")
    } else {
        format!("({var} + {})", -root)
    }
}

/// Simplificación algebraica: const folding aritmético e identidades
/// (0+x=x, 1*x=x, x^1=x, -(-x)=x, x-x=0, …), sin eliminar
/// condiciones de dominio como `x/x`, `0/x` o `f^0`.
pub fn simplify(expr: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo simplificar '{expr}': {e}"))?;
    Ok(simplify_expr(&ast).to_expr_string())
}

/// Comprueba igualdad estructural entre dos expresiones parseadas sin aplicar
/// identidades que puedan eliminar errores de dominio.
pub fn structurally_equal(left: &str, right: &str) -> Result<bool, String> {
    let left = parse_ast(left).map_err(|error| format!("Expresión izquierda inválida: {error}"))?;
    let right = parse_ast(right).map_err(|error| format!("Expresión derecha inválida: {error}"))?;
    Ok(left.structurally_eq(&right))
}

/// Indica si una expresión finita es diferenciable en todo punto real según
/// las reglas simbólicas soportadas por Grafito.
pub fn is_everywhere_differentiable(expression: &str) -> Result<bool, String> {
    let expression = parse_ast(expression)
        .map_err(|error| format!("No se pudo validar diferenciabilidad: {error}"))?;
    Ok(expression.is_everywhere_differentiable())
}

/// Serie de Taylor de `expr` alrededor de `center` hasta orden `order`
/// (derivadas n-ésimas calculadas simbólicamente y evaluadas en `center`).
pub fn taylor_series(expr: &str, var: &str, center: f64, order: usize) -> Result<String, String> {
    if order > crate::analysis::MAX_TAYLOR_ORDER {
        return Err(format!(
            "Taylor order {order} exceeds maximum {}",
            crate::analysis::MAX_TAYLOR_ORDER
        ));
    }
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo calcular Taylor de '{expr}': {e}"))?;
    let coefficients = crate::analysis::taylor_coefficients_from_ast(&ast, var, center, order)?;
    let mut terms = Vec::new();
    for (n, coef) in coefficients.into_iter().enumerate() {
        if coef.abs() > 1e-12 {
            let term = if n == 0 {
                format!("{coef}")
            } else if n == 1 {
                if center == 0.0 {
                    format!("{coef}*{var}")
                } else {
                    format!("{coef}*({var} - {center})")
                }
            } else if center == 0.0 {
                format!("{coef}*{var}^{n}")
            } else {
                format!("{coef}*({var} - {center})^{n}")
            };
            terms.push(term);
        }
    }
    if terms.is_empty() {
        Ok("0".to_string())
    } else {
        Ok(terms.join(" + ").replace("+ -", "- "))
    }
}

/// Sustitución simbólica de `var` por `value` en `expr`.
pub fn substitute(expr: &str, var: &str, value: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo sustituir en '{expr}': {e}"))?;
    let val_pp = value.replace(' ', "");
    let val_ast = parse_ast(&val_pp)
        .or_else(|_| parse_ast(value))
        .map_err(|e| format!("No se pudo parsear el valor '{value}': {e}"))?;
    let result = if let Expr::Const(c) = val_ast {
        let mut map = HashMap::new();
        map.insert(var.to_string(), c);
        simplify_expr(&ast.substitute_vars(&map, &[]))
    } else {
        let replaced = replace_var_token(&pp, var, &val_pp);
        parse_ast(&replaced)
            .map(|a| simplify_expr(&a))
            .unwrap_or(ast)
    };
    Ok(result.to_expr_string())
}

/// Resuelve la ecuación `expr = 0` para la variable `var`.
///
/// Extrae coeficientes polinómicos (grado ≤ 4) y usa fórmulas cerradas
/// (lineal, cuadrática, cúbica de Cardano, cuártica de Ferrari). Para grados
/// mayores o expresiones no polinómicas, busca raíces numéricamente en
/// [-10, 10] (bisección sobre `eval_at`).
pub fn solve(expr: &str, var: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo resolver '{expr}': {e}"))?;
    if let Some(roots) = solve_polynomial_ast(&ast, var) {
        if roots.is_empty() {
            let nr = find_real_roots_numeric(&ast, var, -10.0, 10.0);
            if nr.is_empty() {
                return Ok("No real roots found".to_string());
            }
            return Ok(format_roots(var, &nr));
        }
        return Ok(format_roots(var, &roots));
    }
    let nr = find_real_roots_numeric(&ast, var, -10.0, 10.0);
    if nr.is_empty() {
        Ok("No real roots found in [-10, 10]".to_string())
    } else {
        Ok(format_roots(var, &nr))
    }
}

fn format_roots(var: &str, roots: &[f64]) -> String {
    roots
        .iter()
        .map(|r| format!("{var} = {:.8}", r))
        .collect::<Vec<_>>()
        .join(", ")
}

// ============================================================================
// Derivación simbólica (reglas propias sobre el AST)
// ============================================================================

/// Derivada de un `Expr` respecto de `var` aplicando las reglas simbólicas.
/// Las variantes no listadas delegan al derivador nativo `Expr::diff`.
fn diff_expr(e: &Expr, var: &str) -> Expr {
    diff_expr_depth(e, var, 0)
}

fn diff_expr_depth(e: &Expr, var: &str, depth: u32) -> Expr {
    const MAX_DIFF_EXPR_DEPTH: u32 = 256;
    if depth > MAX_DIFF_EXPR_DEPTH {
        return Expr::Const(f64::NAN);
    }
    use Expr::*;
    let du = |x: &Expr| Box::new(diff_expr_depth(x, var, depth + 1));
    match e {
        Const(_) => Const(0.0),
        Var(v) => {
            if v == var {
                Const(1.0)
            } else {
                Const(0.0)
            }
        }
        Neg(a) => Neg(du(a)),
        Add(a, b) => Add(du(a), du(b)),
        Sub(a, b) => Sub(du(a), du(b)),

        // Regla del producto: (u*v)' = u'v + uv'
        Mul(u, v) => Add(
            Box::new(Mul(du(u), v.clone())),
            Box::new(Mul(u.clone(), du(v))),
        ),

        // Regla del cociente: (u/v)' = (u'v - uv') / v²
        Div(u, v) => Div(
            Box::new(Sub(
                Box::new(Mul(du(u), v.clone())),
                Box::new(Mul(u.clone(), du(v))),
            )),
            Box::new(Pow(v.clone(), Box::new(Const(2.0)))),
        ),

        // Potencia: exponente constante → n·u^(n-1)·u'; caso general → u^v·(v'·ln u + v·u'/u)
        Pow(base, exp) => match exp.as_ref() {
            Const(n) => Mul(
                Box::new(Mul(
                    Box::new(Const(*n)),
                    Box::new(Pow(base.clone(), Box::new(Const(n - 1.0)))),
                )),
                du(base),
            ),
            _ => {
                let dv = diff_expr_depth(exp, var, depth + 1);
                Mul(
                    Box::new(Pow(base.clone(), exp.clone())),
                    Box::new(Add(
                        Box::new(Mul(Box::new(dv), Box::new(Ln(base.clone())))),
                        Box::new(Mul(exp.clone(), Box::new(Div(du(base), base.clone())))),
                    )),
                )
            }
        },

        // Trigonométricas (regla de la cadena)
        Sin(u) => Mul(Box::new(Cos(u.clone())), du(u)),
        Cos(u) => Mul(Box::new(Neg(Box::new(Sin(u.clone())))), du(u)),
        Tan(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Pow(Box::new(Cos(u.clone())), Box::new(Const(2.0)))),
            )),
            du(u),
        ),
        Sec(u) => Mul(
            Box::new(Mul(Box::new(Sec(u.clone())), Box::new(Tan(u.clone())))),
            du(u),
        ),
        Csc(u) => Mul(
            Box::new(Neg(Box::new(Mul(
                Box::new(Csc(u.clone())),
                Box::new(Cot(u.clone())),
            )))),
            du(u),
        ),
        Cot(u) => Mul(
            Box::new(Neg(Box::new(Pow(
                Box::new(Csc(u.clone())),
                Box::new(Const(2.0)),
            )))),
            du(u),
        ),

        // Inversas trigonométricas
        Asin(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            )),
            du(u),
        ),
        Acos(u) => Mul(
            Box::new(Neg(Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            )))),
            du(u),
        ),
        Atan(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Add(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )),
            )),
            du(u),
        ),

        // Hiperbólicas
        Sinh(u) => Mul(Box::new(Cosh(u.clone())), du(u)),
        Cosh(u) => Mul(Box::new(Sinh(u.clone())), du(u)),
        Tanh(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Pow(Box::new(Cosh(u.clone())), Box::new(Const(2.0)))),
            )),
            du(u),
        ),
        Asinh(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Sqrt(Box::new(Add(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            )),
            du(u),
        ),
        Acosh(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                    Box::new(Const(1.0)),
                )))),
            )),
            du(u),
        ),
        Atanh(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )),
            )),
            du(u),
        ),

        // Exponencial y logaritmos
        Exp(u) => Mul(Box::new(Exp(u.clone())), du(u)),
        Ln(u) => Mul(Box::new(Div(Box::new(Const(1.0)), u.clone())), du(u)),
        Log(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Mul(u.clone(), Box::new(Const(std::f64::consts::LN_10)))),
            )),
            du(u),
        ),

        // Raíces
        Sqrt(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Mul(Box::new(Const(2.0)), Box::new(Sqrt(u.clone())))),
            )),
            du(u),
        ),
        Cbrt(u) => Mul(
            Box::new(Div(
                Box::new(Const(1.0)),
                Box::new(Mul(
                    Box::new(Const(3.0)),
                    Box::new(Pow(Box::new(Cbrt(u.clone())), Box::new(Const(2.0)))),
                )),
            )),
            du(u),
        ),

        // Misc
        Abs(u) => Mul(Box::new(Sign(u.clone())), du(u)),
        // No diferenciables (derivada 0 en casi todo punto)
        Sign(_) | Floor(_) | Ceil(_) | Round(_) | Heaviside(_) => Const(0.0),

        // Resto de variantes: delega al derivador nativo del AST.
        _ => e.diff(var),
    }
}

// ============================================================================
// Simplificación algebraica (propias, iterada hasta punto fijo)
// ============================================================================

fn simplify_expr(e: &Expr) -> Expr {
    let mut current = e.clone();
    for _ in 0..30 {
        let next = simplify_once(&current);
        if next.structurally_eq(&current) {
            return next;
        }
        current = next;
    }
    current
}

/// Una pasada bottom-up de simplificación con const folding aritmético e
/// identidades algebraicas básicas.
fn simplify_once(e: &Expr) -> Expr {
    use Expr::*;
    match e {
        Neg(a) => {
            let sa = simplify_once(a);
            match sa {
                Const(c) => Const(-c),
                Neg(inner) => *inner,
                _ => Neg(Box::new(sa)),
            }
        }
        Add(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca + cb),
                (Const(ca), _) if *ca == 0.0 => sb,
                (_, Const(cb)) if *cb == 0.0 => sa,
                _ => Add(Box::new(sa), Box::new(sb)),
            }
        }
        Sub(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca - cb),
                (_, Const(cb)) if *cb == 0.0 => sa,
                (Const(ca), _) if *ca == 0.0 => Neg(Box::new(sb)),
                _ if sa.structurally_eq(&sb) && sa.is_guaranteed_finite() => Const(0.0),
                _ => Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Mul(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca * cb),
                (Const(ca), _) if *ca == 0.0 && sb.is_guaranteed_finite() => Const(0.0),
                (_, Const(cb)) if *cb == 0.0 && sa.is_guaranteed_finite() => Const(0.0),
                (Const(ca), _) if *ca == 1.0 => sb,
                (_, Const(cb)) if *cb == 1.0 => sa,
                (Const(ca), _) if *ca == -1.0 => Neg(Box::new(sb)),
                (_, Const(cb)) if *cb == -1.0 => Neg(Box::new(sa)),
                _ => Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Div(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) if cb.abs() > 1e-300 => Const(ca / cb),
                (_, Const(cb)) if *cb == 1.0 => sa,
                _ => Div(Box::new(sa), Box::new(sb)),
            }
        }
        Pow(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) if *ca != 0.0 || *cb > 0.0 => Const(ca.powf(*cb)),
                (_, Const(cb)) if *cb == 1.0 => sa,
                _ => Pow(Box::new(sa), Box::new(sb)),
            }
        }

        // Funciones unarias: simplificar el argumento (sin const folding).
        Sin(a) => Sin(Box::new(simplify_once(a))),
        Cos(a) => Cos(Box::new(simplify_once(a))),
        Tan(a) => Tan(Box::new(simplify_once(a))),
        Asin(a) => Asin(Box::new(simplify_once(a))),
        Acos(a) => Acos(Box::new(simplify_once(a))),
        Atan(a) => Atan(Box::new(simplify_once(a))),
        Exp(a) => Exp(Box::new(simplify_once(a))),
        Ln(a) => Ln(Box::new(simplify_once(a))),
        Log(a) => Log(Box::new(simplify_once(a))),
        Sqrt(a) => Sqrt(Box::new(simplify_once(a))),
        Abs(a) => Abs(Box::new(simplify_once(a))),
        Sinh(a) => Sinh(Box::new(simplify_once(a))),
        Cosh(a) => Cosh(Box::new(simplify_once(a))),
        Tanh(a) => Tanh(Box::new(simplify_once(a))),
        Asinh(a) => Asinh(Box::new(simplify_once(a))),
        Acosh(a) => Acosh(Box::new(simplify_once(a))),
        Atanh(a) => Atanh(Box::new(simplify_once(a))),
        Sec(a) => Sec(Box::new(simplify_once(a))),
        Csc(a) => Csc(Box::new(simplify_once(a))),
        Cot(a) => Cot(Box::new(simplify_once(a))),
        Floor(a) => Floor(Box::new(simplify_once(a))),
        Ceil(a) => Ceil(Box::new(simplify_once(a))),
        Round(a) => Round(Box::new(simplify_once(a))),
        Sign(a) => Sign(Box::new(simplify_once(a))),
        Heaviside(a) => Heaviside(Box::new(simplify_once(a))),
        Cbrt(a) => Cbrt(Box::new(simplify_once(a))),
        Re(a) => Re(Box::new(simplify_once(a))),
        Im(a) => Im(Box::new(simplify_once(a))),
        Arg(a) => Arg(Box::new(simplify_once(a))),
        Conj(a) => Conj(Box::new(simplify_once(a))),
        Erf(a) => Erf(Box::new(simplify_once(a))),
        Erfc(a) => Erfc(Box::new(simplify_once(a))),
        Gamma(a) => Gamma(Box::new(simplify_once(a))),
        LnGamma(a) => LnGamma(Box::new(simplify_once(a))),
        Digamma(a) => Digamma(Box::new(simplify_once(a))),
        Trigamma(a) => Trigamma(Box::new(simplify_once(a))),

        // Binarias no aritméticas: recursión en ambos operandos.
        Atan2(a, b) => Atan2(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Modulo(a, b) => Modulo(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Min(a, b) => Min(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Max(a, b) => Max(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Beta(a, b) => Beta(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselJ(a, b) => BesselJ(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselY(a, b) => BesselY(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselI(a, b) => BesselI(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Lt(a, b) => Lt(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Gt(a, b) => Gt(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Le(a, b) => Le(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Ge(a, b) => Ge(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Eq(a, b) => Eq(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Ne(a, b) => Ne(Box::new(simplify_once(a)), Box::new(simplify_once(b))),

        Clamp(a, b, c) => Clamp(
            Box::new(simplify_once(a)),
            Box::new(simplify_once(b)),
            Box::new(simplify_once(c)),
        ),
        Sum(body, v, s, t) => Sum(
            Box::new(simplify_once(body)),
            v.clone(),
            Box::new(simplify_once(s)),
            Box::new(simplify_once(t)),
        ),
        Product(body, v, s, t) => Product(
            Box::new(simplify_once(body)),
            v.clone(),
            Box::new(simplify_once(s)),
            Box::new(simplify_once(t)),
        ),
        Piecewise(pieces, default) => {
            let np: Vec<(Box<Expr>, Box<Expr>)> = pieces
                .iter()
                .map(|(c, v)| (Box::new(simplify_once(c)), Box::new(simplify_once(v))))
                .collect();
            Piecewise(np, Box::new(simplify_once(default)))
        }

        Const(_) | Var(_) => e.clone(),
    }
}

// ============================================================================
// Integración simbólica básica (propias) + numérica
// ============================================================================

/// Integración indefinida por reglas básicas. Devuelve `None` si la expresión
/// no encaja en las reglas soportadas (quedando el fallback a `Expr::integrate`).
fn integrate_expr(e: &Expr, var: &str) -> Option<Expr> {
    integrate_expr_depth(e, var, 0)
}

fn integrate_expr_depth(e: &Expr, var: &str, depth: u32) -> Option<Expr> {
    const MAX_INTEGRATE_DEPTH: u32 = 256;
    if depth > MAX_INTEGRATE_DEPTH {
        return None;
    }
    use Expr::*;
    let v = var.to_string();
    if !contains_var(e, var) {
        return Some(Mul(Box::new(e.clone()), Box::new(Var(v))));
    }
    let rec = |x: &Expr| integrate_expr_depth(x, var, depth + 1);
    Some(match e {
        Const(c) => {
            if *c == 0.0 {
                Const(0.0)
            } else {
                Mul(Box::new(Const(*c)), Box::new(Var(v.clone())))
            }
        }
        Var(name) if name == var => Mul(
            Box::new(Pow(Box::new(Var(v.clone())), Box::new(Const(2.0)))),
            Box::new(Const(0.5)),
        ),
        Neg(a) => Neg(Box::new(rec(a)?)),
        Add(a, b) => Add(Box::new(rec(a)?), Box::new(rec(b)?)),
        Sub(a, b) => Sub(Box::new(rec(a)?), Box::new(rec(b)?)),
        Mul(a, b) => {
            if !contains_var(a, var) {
                Mul(a.clone(), Box::new(rec(b)?))
            } else if !contains_var(b, var) {
                Mul(Box::new(rec(a)?), b.clone())
            } else {
                return None;
            }
        }
        Pow(base, exp) => {
            if let Var(name) = base.as_ref() {
                if name == var {
                    if let Const(n) = exp.as_ref() {
                        if (*n + 1.0).abs() < 1e-12 {
                            // ∫ x^-1 dx = ln|x|
                            Ln(Box::new(Abs(Box::new(Var(v.clone())))))
                        } else {
                            let new_exp = n + 1.0;
                            Mul(
                                Box::new(Const(1.0 / new_exp)),
                                Box::new(Pow(Box::new(Var(v.clone())), Box::new(Const(new_exp)))),
                            )
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        Div(num, den) => {
            if let Var(name) = den.as_ref() {
                if name == var && !contains_var(num, var) {
                    return Some(Mul(
                        num.clone(),
                        Box::new(Ln(Box::new(Abs(Box::new(Var(v.clone())))))),
                    ));
                }
            }
            return None;
        }
        Sin(u) if matches!(u.as_ref(), Var(name) if name == var) => {
            Neg(Box::new(Cos(Box::new(Var(v.clone())))))
        }
        Cos(u) if matches!(u.as_ref(), Var(name) if name == var) => Sin(Box::new(Var(v.clone()))),
        Exp(u) if matches!(u.as_ref(), Var(name) if name == var) => Exp(Box::new(Var(v.clone()))),
        Ln(u) if matches!(u.as_ref(), Var(name) if name == var) => {
            // ∫ ln(x) dx = x·ln(x) − x
            Sub(
                Box::new(Mul(
                    Box::new(Var(v.clone())),
                    Box::new(Ln(Box::new(Var(v.clone())))),
                )),
                Box::new(Var(v.clone())),
            )
        }
        _ => return None,
    })
}

/// Integración numérica por Simpson adaptativo sobre `Expr::eval_at`.
fn numeric_integrate(
    ast: &Expr,
    expression: &str,
    var: &str,
    a: f64,
    b: f64,
    tolerance: f64,
    max_depth: u32,
) -> MathResult<f64> {
    fn simpson<F: Fn(f64) -> Result<f64, MathError>>(
        f: &F,
        a: f64,
        b: f64,
    ) -> Result<f64, MathError> {
        let m = (a + b) / 2.0;
        Ok((b - a) / 6.0 * (f(a)? + 4.0 * f(m)? + f(b)?))
    }
    fn adaptive<F: Fn(f64) -> Result<f64, MathError>>(
        f: &F,
        a: f64,
        b: f64,
        eps: f64,
        depth: u32,
        max_depth: u32,
    ) -> MathResult<f64> {
        let whole = match simpson(f, a, b) {
            Ok(value) => value,
            Err(error) => return math_failure(error),
        };
        let mid = (a + b) / 2.0;
        let left = match simpson(f, a, mid) {
            Ok(value) => value,
            Err(error) => return math_failure(error),
        };
        let right = match simpson(f, mid, b) {
            Ok(value) => value,
            Err(error) => return math_failure(error),
        };
        let correction = (left + right - whole) / 15.0;
        let error_estimate = correction.abs();
        if error_estimate < eps {
            MathResult::Approximate {
                value: left + right + correction,
                error_estimate,
            }
        } else if depth == 0 {
            MathResult::NotConverged(MathError::RecursionLimit {
                operation: MathOperation::NumericalIntegration,
                lower: a,
                upper: b,
                max_depth,
                tolerance: eps,
                error_estimate,
            })
        } else {
            match (
                adaptive(f, a, mid, eps / 2.0, depth - 1, max_depth),
                adaptive(f, mid, b, eps / 2.0, depth - 1, max_depth),
            ) {
                (
                    MathResult::Approximate {
                        value: left_value,
                        error_estimate: left_error,
                    },
                    MathResult::Approximate {
                        value: right_value,
                        error_estimate: right_error,
                    },
                ) => MathResult::Approximate {
                    value: left_value + right_value,
                    error_estimate: left_error + right_error,
                },
                (MathResult::DomainError(error), _) | (_, MathResult::DomainError(error)) => {
                    MathResult::DomainError(error)
                }
                (MathResult::NotConverged(error), _) | (_, MathResult::NotConverged(error)) => {
                    MathResult::NotConverged(error)
                }
                (MathResult::Unsupported(error), _) | (_, MathResult::Unsupported(error)) => {
                    MathResult::Unsupported(error)
                }
                (MathResult::ResourceLimit(error), _) | (_, MathResult::ResourceLimit(error)) => {
                    MathResult::ResourceLimit(error)
                }
                (MathResult::Exact(_), _) | (_, MathResult::Exact(_)) => {
                    unreachable!("numeric integration is never exact")
                }
            }
        }
    }
    let f = |x: f64| {
        let val = ast.eval_at(var, x);
        if val.is_finite() {
            Ok(val)
        } else {
            Err(MathError::NonFiniteValue {
                expression: expression.into(),
                variable: var.into(),
                at: x,
            })
        }
    };
    if !a.is_finite() || !b.is_finite() {
        return MathResult::DomainError(MathError::IntervalDomainViolation {
            expression: expression.into(),
            variable: var.into(),
            lower: a,
            upper: b,
        });
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return MathResult::DomainError(MathError::IntervalDomainViolation {
            expression: expression.into(),
            variable: var.into(),
            lower: a,
            upper: b,
        });
    }
    adaptive(&f, a, b, tolerance, max_depth, max_depth)
}

// ============================================================================
// Límite numérico (Richardson bilateral)
// ============================================================================

struct LimitEstimate {
    value: f64,
    error_estimate: f64,
}

fn has_proven_squeezed_zero_limit(expression: &Expr, var: &str, at: f64) -> bool {
    let Expr::Mul(left, right) = expression else {
        return false;
    };

    (is_continuous_zero_factor(left, var, at) && is_bounded_singular_sin_or_cos(right, var, at))
        || (is_continuous_zero_factor(right, var, at)
            && is_bounded_singular_sin_or_cos(left, var, at))
}

fn is_continuous_zero_factor(expression: &Expr, var: &str, at: f64) -> bool {
    expression.is_everywhere_differentiable()
        && positive_zero_order_at_target(expression, var, at).is_some()
}

fn positive_zero_order_at_target(expression: &Expr, var: &str, at: f64) -> Option<u32> {
    use Expr::*;

    match expression {
        Var(name) if name == var && at == 0.0 => Some(1),
        Sub(left, right) => match (left.as_ref(), right.as_ref()) {
            (Var(name), Const(target)) | (Const(target), Var(name))
                if name == var && target.is_finite() && *target == at =>
            {
                Some(1)
            }
            _ => Some(
                positive_zero_order_at_target(left, var, at)?
                    .min(positive_zero_order_at_target(right, var, at)?),
            ),
        },
        Add(left, right) => Some(
            positive_zero_order_at_target(left, var, at)?
                .min(positive_zero_order_at_target(right, var, at)?),
        ),
        Neg(inner) => positive_zero_order_at_target(inner, var, at),
        Mul(left, right) => {
            let left_order = positive_zero_order_at_target(left, var, at);
            let right_order = positive_zero_order_at_target(right, var, at);
            match (left_order, right_order) {
                (Some(left), Some(right)) => left.checked_add(right),
                (Some(order), None) if right.is_everywhere_differentiable() => Some(order),
                (None, Some(order)) if left.is_everywhere_differentiable() => Some(order),
                _ => None,
            }
        }
        Pow(base, exponent) => {
            let Const(exponent) = exponent.as_ref() else {
                return None;
            };
            if !exponent.is_finite()
                || *exponent <= 0.0
                || exponent.fract() != 0.0
                || *exponent > u32::MAX as f64
            {
                return None;
            }
            positive_zero_order_at_target(base, var, at)?.checked_mul(*exponent as u32)
        }
        _ => None,
    }
}

fn is_bounded_singular_sin_or_cos(expression: &Expr, var: &str, at: f64) -> bool {
    let argument = match expression {
        Expr::Sin(argument) | Expr::Cos(argument) => argument.as_ref(),
        _ => return false,
    };
    contains_var(argument, var)
        && !argument.eval_at(var, at).is_finite()
        && is_finite_on_punctured_probes(argument, var, at)
}

fn is_finite_on_punctured_probes(expression: &Expr, var: &str, at: f64) -> bool {
    const PROBE_SCALES: [f64; 6] = [0.113, 0.071, 0.043, 0.019, 0.007, 0.0023];

    let local_scale = at.abs().max(1.0);
    [-1.0, 1.0].into_iter().all(|sign| {
        PROBE_SCALES.into_iter().all(|scale| {
            let point = at + sign * scale * local_scale;
            point.is_finite() && point != at && expression.eval_at(var, point).is_finite()
        })
    })
}

fn has_unresolved_oscillatory_singularity(expression: &Expr, var: &str, at: f64) -> bool {
    use Expr::*;

    let singular_argument =
        |argument: &Expr| contains_var(argument, var) && !argument.eval_at(var, at).is_finite();
    match expression {
        Sin(argument) | Cos(argument) | Tan(argument) | Sec(argument) | Csc(argument)
        | Cot(argument) | Floor(argument) | Ceil(argument) | Round(argument) | Sign(argument)
        | Heaviside(argument) => {
            singular_argument(argument) || has_unresolved_oscillatory_singularity(argument, var, at)
        }
        Neg(argument) | Asin(argument) | Acos(argument) | Atan(argument) | Exp(argument)
        | Ln(argument) | Log(argument) | Sqrt(argument) | Abs(argument) | Sinh(argument)
        | Cosh(argument) | Tanh(argument) | Asinh(argument) | Acosh(argument) | Atanh(argument)
        | Cbrt(argument) | Re(argument) | Im(argument) | Arg(argument) | Conj(argument)
        | Erf(argument) | Erfc(argument) | Gamma(argument) | LnGamma(argument)
        | Digamma(argument) | Trigamma(argument) => {
            has_unresolved_oscillatory_singularity(argument, var, at)
        }
        Modulo(left, right) => {
            singular_argument(left)
                || singular_argument(right)
                || has_unresolved_oscillatory_singularity(left, var, at)
                || has_unresolved_oscillatory_singularity(right, var, at)
        }
        Add(left, right)
        | Sub(left, right)
        | Mul(left, right)
        | Div(left, right)
        | Pow(left, right)
        | Atan2(left, right)
        | Min(left, right)
        | Max(left, right)
        | Beta(left, right)
        | BesselJ(left, right)
        | BesselY(left, right)
        | BesselI(left, right)
        | Lt(left, right)
        | Gt(left, right)
        | Le(left, right)
        | Ge(left, right)
        | Eq(left, right)
        | Ne(left, right) => {
            has_unresolved_oscillatory_singularity(left, var, at)
                || has_unresolved_oscillatory_singularity(right, var, at)
        }
        Clamp(value, lower, upper) => {
            has_unresolved_oscillatory_singularity(value, var, at)
                || has_unresolved_oscillatory_singularity(lower, var, at)
                || has_unresolved_oscillatory_singularity(upper, var, at)
        }
        Sum(body, _, start, end) | Product(body, _, start, end) => {
            has_unresolved_oscillatory_singularity(body, var, at)
                || has_unresolved_oscillatory_singularity(start, var, at)
                || has_unresolved_oscillatory_singularity(end, var, at)
        }
        Piecewise(branches, default) => {
            branches.iter().any(|(condition, value)| {
                has_unresolved_oscillatory_singularity(condition, var, at)
                    || has_unresolved_oscillatory_singularity(value, var, at)
            }) || has_unresolved_oscillatory_singularity(default, var, at)
        }
        Const(_) | Var(_) => false,
    }
}

fn richardson_limit(ast: &Expr, var: &str, at: f64) -> Option<LimitEstimate> {
    const STEP_SCALES: [f64; 3] = [0.125, 0.1, 0.075];

    let local_scale = at.abs().max(1.0);
    let mut estimates = Vec::with_capacity(STEP_SCALES.len() * 2);
    for scale in STEP_SCALES {
        let initial_step = scale * local_scale;
        let left = stable_side_limit(ast, var, at, -1.0, initial_step)?;
        let right = stable_side_limit(ast, var, at, 1.0, initial_step)?;
        if !limit_estimates_agree(&left, &right) {
            return None;
        }
        estimates.push(left);
        estimates.push(right);
    }

    let mut value = estimates[0].value;
    for (index, estimate) in estimates.iter().enumerate().skip(1) {
        let difference = estimate.value - value;
        if !difference.is_finite() {
            return None;
        }
        value += difference / (index + 1) as f64;
    }
    if !value.is_finite() {
        return None;
    }

    let mut error_estimate = 0.0_f64;
    for estimate in &estimates {
        let deviation = (estimate.value - value).abs();
        let local_error = estimate.error_estimate + deviation;
        if !local_error.is_finite() {
            return None;
        }
        error_estimate = error_estimate.max(local_error);
    }
    let tolerance = limit_tolerance(value);
    (error_estimate <= tolerance).then_some(LimitEstimate {
        value,
        error_estimate,
    })
}

fn stable_side_limit(
    ast: &Expr,
    var: &str,
    at: f64,
    sign: f64,
    initial_step: f64,
) -> Option<LimitEstimate> {
    const SAMPLE_COUNT: usize = 12;
    const TAIL_DIFFERENCES: usize = 4;
    const MAX_CONTRACTION: f64 = 0.8;

    let mut values = [0.0; SAMPLE_COUNT];
    for (index, value) in values.iter_mut().enumerate() {
        let step = initial_step / 2.0f64.powi(index as i32);
        let point = at + sign * step;
        if !point.is_finite() || point == at {
            return None;
        }
        *value = ast.eval_at(var, point);
        if !value.is_finite() {
            return None;
        }
    }

    let value_scale = values
        .iter()
        .map(|value| value.abs())
        .fold(1.0_f64, f64::max);
    // Keep arithmetic headroom for differences and error estimates.
    if value_scale > f64::MAX / 16.0 {
        return None;
    }
    let roundoff = 128.0 * f64::EPSILON * value_scale;
    let mut differences = [0.0; TAIL_DIFFERENCES];
    let first = SAMPLE_COUNT - TAIL_DIFFERENCES;
    for (offset, difference) in differences.iter_mut().enumerate() {
        let index = first + offset;
        *difference = values[index] - values[index - 1];
        if !difference.is_finite() {
            return None;
        }
    }

    if differences
        .iter()
        .all(|difference| difference.abs() <= roundoff)
    {
        let tail = &values[SAMPLE_COUNT - 3..];
        let value = tail.iter().sum::<f64>() / tail.len() as f64;
        let spread = tail
            .iter()
            .map(|sample| (sample - value).abs())
            .fold(0.0_f64, f64::max);
        return (value.is_finite() && spread.is_finite()).then_some(LimitEstimate {
            value,
            error_estimate: spread.max(roundoff),
        });
    }

    let mut ratios = [0.0; TAIL_DIFFERENCES - 1];
    for index in 1..differences.len() {
        let previous = differences[index - 1];
        let current = differences[index];
        if previous.abs() <= roundoff
            || current.abs() <= roundoff
            || previous.is_sign_positive() != current.is_sign_positive()
        {
            return None;
        }
        let ratio = (current / previous).abs();
        if !ratio.is_finite() || ratio >= MAX_CONTRACTION {
            return None;
        }
        ratios[index - 1] = ratio;
    }

    let ratio_spread = ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - ratios.iter().copied().fold(f64::INFINITY, f64::min);
    if !ratio_spread.is_finite() || ratio_spread > 0.25 {
        return None;
    }

    let ratio = ratios[ratios.len() - 1];
    let last = values[SAMPLE_COUNT - 1];
    let last_difference = differences[differences.len() - 1];
    let correction = last_difference * ratio / (1.0 - ratio);
    let value = last + correction;

    let previous_ratio = ratios[ratios.len() - 2];
    let previous = values[SAMPLE_COUNT - 2];
    let previous_difference = differences[differences.len() - 2];
    let previous_value = previous + previous_difference * previous_ratio / (1.0 - previous_ratio);
    let ratio_uncertainty = correction.abs() * ratio_spread / (1.0 - ratio);
    let error_estimate = (value - previous_value)
        .abs()
        .max(ratio_uncertainty)
        .max(roundoff);

    (value.is_finite() && error_estimate.is_finite() && error_estimate <= limit_tolerance(value))
        .then_some(LimitEstimate {
            value,
            error_estimate,
        })
}

fn limit_estimates_agree(left: &LimitEstimate, right: &LimitEstimate) -> bool {
    let disagreement = (left.value - right.value).abs();
    let error_allowance = 8.0 * left.error_estimate.max(right.error_estimate);
    disagreement.is_finite()
        && error_allowance.is_finite()
        && disagreement
            <= limit_tolerance(left.value.abs().max(right.value.abs())) + error_allowance
}

fn limit_tolerance(value: f64) -> f64 {
    1e-9 + 1e-6 * value.abs().max(1.0)
}

// ============================================================================
// Utilidades: expansión, sustitución, variables, raíces numéricas
// ============================================================================

#[derive(Clone, Copy)]
struct ExpandMetrics {
    nodes: usize,
    output_bytes: usize,
}

#[derive(Clone)]
struct ExpandedTerm {
    negative: bool,
    expression: Expr,
    metrics: ExpandMetrics,
}

#[derive(Default)]
struct ExpandBudget {
    work_units: usize,
}

impl ExpandBudget {
    fn charge(&mut self, units: usize) -> Result<(), String> {
        let projected = self
            .work_units
            .checked_add(units)
            .ok_or_else(|| expand_budget_error("work", usize::MAX, MAX_EXPAND_WORK_UNITS))?;
        if projected > MAX_EXPAND_WORK_UNITS {
            return Err(expand_budget_error(
                "work",
                projected,
                MAX_EXPAND_WORK_UNITS,
            ));
        }
        self.work_units = projected;
        Ok(())
    }
}

fn expand_budget_error(kind: &str, provided: usize, maximum: usize) -> String {
    format!("Expand {kind} budget exceeded: {provided}, maximum {maximum}")
}

fn checked_expand_add(
    kind: &str,
    left: usize,
    right: usize,
    maximum: usize,
) -> Result<usize, String> {
    let value = left
        .checked_add(right)
        .ok_or_else(|| expand_budget_error(kind, usize::MAX, maximum))?;
    if value > maximum {
        return Err(expand_budget_error(kind, value, maximum));
    }
    Ok(value)
}

fn checked_expand_mul(
    kind: &str,
    left: usize,
    right: usize,
    maximum: usize,
) -> Result<usize, String> {
    let value = left
        .checked_mul(right)
        .ok_or_else(|| expand_budget_error(kind, usize::MAX, maximum))?;
    if value > maximum {
        return Err(expand_budget_error(kind, value, maximum));
    }
    Ok(value)
}

fn merge_expand_metrics(
    children: &[ExpandMetrics],
    node_overhead: usize,
    output_overhead: usize,
) -> Result<ExpandMetrics, String> {
    let mut nodes = node_overhead;
    let mut output_bytes = output_overhead;
    for child in children {
        nodes = checked_expand_add("AST node", nodes, child.nodes, MAX_EXPAND_AST_NODES)?;
        output_bytes = checked_expand_add(
            "output",
            output_bytes,
            child.output_bytes,
            MAX_EXPAND_OUTPUT_BYTES,
        )?;
    }
    Ok(ExpandMetrics {
        nodes,
        output_bytes,
    })
}

fn expand_expression_metrics(expression: &Expr) -> Result<ExpandMetrics, String> {
    use Expr::*;
    match expression {
        Const(_) => Ok(ExpandMetrics {
            nodes: 1,
            output_bytes: 32,
        }),
        Var(name) => Ok(ExpandMetrics {
            nodes: 1,
            output_bytes: name.len(),
        }),
        Neg(value) | Sin(value) | Cos(value) | Tan(value) | Asin(value) | Acos(value)
        | Atan(value) | Exp(value) | Ln(value) | Log(value) | Sqrt(value) | Abs(value)
        | Sinh(value) | Cosh(value) | Tanh(value) | Floor(value) | Ceil(value) | Round(value)
        | Sec(value) | Csc(value) | Cot(value) | Asinh(value) | Acosh(value) | Atanh(value)
        | Sign(value) | Heaviside(value) | Cbrt(value) | Re(value) | Im(value) | Arg(value)
        | Conj(value) | Erf(value) | Erfc(value) | Gamma(value) | LnGamma(value)
        | Digamma(value) | Trigamma(value) => {
            merge_expand_metrics(&[expand_expression_metrics(value)?], 1, 16)
        }
        Add(left, right)
        | Sub(left, right)
        | Mul(left, right)
        | Div(left, right)
        | Pow(left, right)
        | Atan2(left, right)
        | Modulo(left, right)
        | Min(left, right)
        | Max(left, right)
        | Beta(left, right)
        | BesselJ(left, right)
        | BesselY(left, right)
        | BesselI(left, right)
        | Lt(left, right)
        | Gt(left, right)
        | Le(left, right)
        | Ge(left, right)
        | Eq(left, right)
        | Ne(left, right) => merge_expand_metrics(
            &[
                expand_expression_metrics(left)?,
                expand_expression_metrics(right)?,
            ],
            1,
            16,
        ),
        Clamp(value, lower, upper) => merge_expand_metrics(
            &[
                expand_expression_metrics(value)?,
                expand_expression_metrics(lower)?,
                expand_expression_metrics(upper)?,
            ],
            1,
            16,
        ),
        Sum(body, variable, start, end) | Product(body, variable, start, end) => {
            merge_expand_metrics(
                &[
                    expand_expression_metrics(body)?,
                    expand_expression_metrics(start)?,
                    expand_expression_metrics(end)?,
                ],
                1,
                checked_expand_add("output", 20, variable.len(), MAX_EXPAND_OUTPUT_BYTES)?,
            )
        }
        Piecewise(branches, default) => {
            let mut metrics = merge_expand_metrics(
                &[expand_expression_metrics(default)?],
                1,
                checked_expand_add(
                    "output",
                    16,
                    checked_expand_mul("output", 4, branches.len(), MAX_EXPAND_OUTPUT_BYTES)?,
                    MAX_EXPAND_OUTPUT_BYTES,
                )?,
            )?;
            for (condition, value) in branches {
                metrics = merge_expand_metrics(
                    &[
                        metrics,
                        expand_expression_metrics(condition)?,
                        expand_expression_metrics(value)?,
                    ],
                    0,
                    0,
                )?;
            }
            Ok(metrics)
        }
    }
}

fn validate_expand_expression(expression: &Expr) -> Result<ExpandMetrics, String> {
    expand_expression_metrics(expression)
}

fn expanded_terms_metrics(terms: &[ExpandedTerm]) -> Result<ExpandMetrics, String> {
    if terms.is_empty() {
        return Err("Expand produced no terms".to_string());
    }
    let mut nodes = usize::from(terms[0].negative);
    let mut output_bytes = if terms[0].negative { 3 } else { 0 };
    for term in terms {
        nodes = checked_expand_add("AST node", nodes, term.metrics.nodes, MAX_EXPAND_AST_NODES)?;
        output_bytes = checked_expand_add(
            "output",
            output_bytes,
            term.metrics.output_bytes,
            MAX_EXPAND_OUTPUT_BYTES,
        )?;
    }
    if terms.len() > 1 {
        nodes = checked_expand_add("AST node", nodes, terms.len() - 1, MAX_EXPAND_AST_NODES)?;
        output_bytes = checked_expand_add(
            "output",
            output_bytes,
            checked_expand_mul("output", 7, terms.len() - 1, MAX_EXPAND_OUTPUT_BYTES)?,
            MAX_EXPAND_OUTPUT_BYTES,
        )?;
    }
    Ok(ExpandMetrics {
        nodes,
        output_bytes,
    })
}

fn clone_expanded_terms(
    terms: &[ExpandedTerm],
    budget: &mut ExpandBudget,
) -> Result<Vec<ExpandedTerm>, String> {
    let nodes = terms.iter().try_fold(0usize, |nodes, term| {
        checked_expand_add("AST node", nodes, term.metrics.nodes, MAX_EXPAND_AST_NODES)
    })?;
    budget.charge(nodes)?;
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(terms.len())
        .map_err(|error| format!("Expand allocation failed within budget: {error}"))?;
    cloned.extend(terms.iter().cloned());
    Ok(cloned)
}

fn multiply_expanded_terms(
    left: Vec<ExpandedTerm>,
    right: &[ExpandedTerm],
    budget: &mut ExpandBudget,
) -> Result<Vec<ExpandedTerm>, String> {
    let term_count = checked_expand_mul("AST node", left.len(), right.len(), MAX_EXPAND_AST_NODES)?;
    let left_nodes = left.iter().try_fold(0usize, |nodes, term| {
        checked_expand_add("AST node", nodes, term.metrics.nodes, MAX_EXPAND_AST_NODES)
    })?;
    let right_nodes = right.iter().try_fold(0usize, |nodes, term| {
        checked_expand_add("AST node", nodes, term.metrics.nodes, MAX_EXPAND_AST_NODES)
    })?;
    let product_nodes = checked_expand_add(
        "AST node",
        checked_expand_mul("AST node", left_nodes, right.len(), MAX_EXPAND_AST_NODES)?,
        checked_expand_mul("AST node", right_nodes, left.len(), MAX_EXPAND_AST_NODES)?,
        MAX_EXPAND_AST_NODES,
    )?;
    let product_nodes =
        checked_expand_add("AST node", product_nodes, term_count, MAX_EXPAND_AST_NODES)?;
    let projected_ast_nodes = checked_expand_add(
        "AST node",
        product_nodes,
        term_count.saturating_sub(1),
        MAX_EXPAND_AST_NODES,
    )?;

    let left_output = left.iter().try_fold(0usize, |bytes, term| {
        checked_expand_add(
            "output",
            bytes,
            term.metrics.output_bytes,
            MAX_EXPAND_OUTPUT_BYTES,
        )
    })?;
    let right_output = right.iter().try_fold(0usize, |bytes, term| {
        checked_expand_add(
            "output",
            bytes,
            term.metrics.output_bytes,
            MAX_EXPAND_OUTPUT_BYTES,
        )
    })?;
    let product_output = checked_expand_add(
        "output",
        checked_expand_mul("output", left_output, right.len(), MAX_EXPAND_OUTPUT_BYTES)?,
        checked_expand_mul("output", right_output, left.len(), MAX_EXPAND_OUTPUT_BYTES)?,
        MAX_EXPAND_OUTPUT_BYTES,
    )?;
    let product_output = checked_expand_add(
        "output",
        product_output,
        checked_expand_mul("output", 7, term_count, MAX_EXPAND_OUTPUT_BYTES)?,
        MAX_EXPAND_OUTPUT_BYTES,
    )?;
    checked_expand_add(
        "output",
        product_output,
        checked_expand_mul(
            "output",
            7,
            term_count.saturating_sub(1),
            MAX_EXPAND_OUTPUT_BYTES,
        )?,
        MAX_EXPAND_OUTPUT_BYTES,
    )?;

    budget.charge(projected_ast_nodes)?;
    let mut products = Vec::new();
    products
        .try_reserve_exact(term_count)
        .map_err(|error| format!("Expand allocation failed within budget: {error}"))?;
    for left_term in left {
        for right_term in right {
            products.push(ExpandedTerm {
                negative: left_term.negative != right_term.negative,
                expression: Expr::Mul(
                    Box::new(left_term.expression.clone()),
                    Box::new(right_term.expression.clone()),
                ),
                metrics: ExpandMetrics {
                    nodes: left_term.metrics.nodes + right_term.metrics.nodes + 1,
                    output_bytes: left_term.metrics.output_bytes
                        + right_term.metrics.output_bytes
                        + 7,
                },
            });
        }
    }
    Ok(products)
}

/// Distributividad de productos sobre sumas/restas y potencias enteras positivas.
fn expand_expr(expression: &Expr, budget: &mut ExpandBudget) -> Result<Vec<ExpandedTerm>, String> {
    use Expr::*;
    budget.charge(1)?;
    let terms = match expression {
        Add(left, right) | Sub(left, right) => {
            let mut left_terms = expand_expr(left, budget)?;
            let mut right_terms = expand_expr(right, budget)?;
            if matches!(expression, Sub(_, _)) {
                budget.charge(right_terms.len())?;
                for term in &mut right_terms {
                    term.negative = !term.negative;
                }
            }
            left_terms
                .try_reserve_exact(right_terms.len())
                .map_err(|error| format!("Expand allocation failed within budget: {error}"))?;
            left_terms.append(&mut right_terms);
            left_terms
        }
        Neg(value) => {
            let mut terms = expand_expr(value, budget)?;
            budget.charge(terms.len())?;
            for term in &mut terms {
                term.negative = !term.negative;
            }
            terms
        }
        Mul(left, right) => {
            let left = expand_expr(left, budget)?;
            let right = expand_expr(right, budget)?;
            multiply_expanded_terms(left, &right, budget)?
        }
        Pow(base, exponent) => {
            let Const(exponent) = exponent.as_ref() else {
                return single_expand_term(expression, budget);
            };
            if !exponent.is_finite()
                || *exponent < 1.0
                || exponent.fract() != 0.0
                || *exponent > MAX_EXPAND_WORK_UNITS as f64
            {
                return if exponent.is_finite()
                    && *exponent > MAX_EXPAND_WORK_UNITS as f64
                    && exponent.fract() == 0.0
                {
                    Err(expand_budget_error(
                        "work",
                        *exponent as usize,
                        MAX_EXPAND_WORK_UNITS,
                    ))
                } else {
                    single_expand_term(expression, budget)
                };
            }
            let exponent = *exponent as usize;
            let factor = expand_expr(base, budget)?;
            if exponent == 1 {
                factor
            } else {
                let mut result = clone_expanded_terms(&factor, budget)?;
                for _ in 1..exponent {
                    result = multiply_expanded_terms(result, &factor, budget)?;
                }
                result
            }
        }
        _ => return single_expand_term(expression, budget),
    };
    expanded_terms_metrics(&terms)?;
    Ok(terms)
}

fn single_expand_term(
    expression: &Expr,
    budget: &mut ExpandBudget,
) -> Result<Vec<ExpandedTerm>, String> {
    let metrics = validate_expand_expression(expression)?;
    budget.charge(metrics.nodes)?;
    Ok(vec![ExpandedTerm {
        negative: false,
        expression: expression.clone(),
        metrics,
    }])
}

fn expanded_terms_into_expr(
    terms: Vec<ExpandedTerm>,
    budget: &mut ExpandBudget,
) -> Result<Expr, String> {
    let metrics = expanded_terms_metrics(&terms)?;
    budget.charge(metrics.nodes)?;
    let mut terms: Vec<Option<ExpandedTerm>> = terms.into_iter().map(Some).collect();

    fn build_balanced(terms: &mut [Option<ExpandedTerm>], invert: bool) -> Expr {
        if terms.len() == 1 {
            let term = terms[0].take().expect("validated expansion term");
            return if term.negative != invert {
                Expr::Neg(Box::new(term.expression))
            } else {
                term.expression
            };
        }

        let middle = terms.len() / 2;
        let right_is_negative = terms[middle]
            .as_ref()
            .expect("validated expansion term")
            .negative
            != invert;
        let (left_terms, right_terms) = terms.split_at_mut(middle);
        let left = build_balanced(left_terms, invert);
        if right_is_negative {
            Expr::Sub(
                Box::new(left),
                Box::new(build_balanced(right_terms, !invert)),
            )
        } else {
            Expr::Add(
                Box::new(left),
                Box::new(build_balanced(right_terms, invert)),
            )
        }
    }

    Ok(build_balanced(&mut terms, false))
}

fn simplify_expanded_expr(mut expression: Expr, budget: &mut ExpandBudget) -> Result<Expr, String> {
    for _ in 0..30 {
        let current_metrics = validate_expand_expression(&expression)?;
        budget.charge(current_metrics.nodes)?;
        let next = simplify_once(&expression);
        validate_expand_expression(&next)?;
        if next.structurally_eq(&expression) {
            return Ok(next);
        }
        expression = next;
    }
    Ok(expression)
}

/// Reemplaza ocurrencias standalone del identificador `var` por `(replacement)`.
fn replace_var_token(expr: &str, var: &str, replacement: &str) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let var_chars: Vec<char> = var.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + var_chars.len() <= chars.len() && chars[i..i + var_chars.len()] == var_chars[..] {
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            let next = if i + var_chars.len() < chars.len() {
                Some(chars[i + var_chars.len()])
            } else {
                None
            };
            let prev_ident = prev
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
            let next_ident = next
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
            if !prev_ident && !next_ident {
                out.push_str(&format!("({replacement})"));
                i += var_chars.len();
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn contains_var(e: &Expr, var: &str) -> bool {
    let mut vars = HashSet::new();
    e.get_variables(&mut vars);
    vars.contains(var)
}

fn constant_scalar_value(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Const(value) if value.is_finite() => Some(*value),
        Expr::Neg(inner) => Some(-constant_scalar_value(inner)?),
        Expr::Add(left, right) => {
            Some(constant_scalar_value(left)? + constant_scalar_value(right)?)
        }
        Expr::Sub(left, right) => {
            Some(constant_scalar_value(left)? - constant_scalar_value(right)?)
        }
        Expr::Mul(left, right) => {
            Some(constant_scalar_value(left)? * constant_scalar_value(right)?)
        }
        Expr::Div(numerator, denominator) => {
            let denominator = constant_scalar_value(denominator)?;
            (denominator != 0.0).then(|| {
                constant_scalar_value(numerator).map(|numerator| numerator / denominator)
            })?
        }
        Expr::Pow(base, exponent) => {
            Some(constant_scalar_value(base)?.powf(constant_scalar_value(exponent)?))
        }
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn is_finite_nonzero_scalar(expr: &Expr) -> bool {
    match expr {
        Expr::Const(value) => value.is_finite() && *value != 0.0,
        Expr::Neg(inner) => is_finite_nonzero_scalar(inner),
        Expr::Mul(left, right) => is_finite_nonzero_scalar(left) && is_finite_nonzero_scalar(right),
        Expr::Div(numerator, denominator) => {
            is_finite_nonzero_scalar(numerator) && is_finite_nonzero_scalar(denominator)
        }
        _ => constant_scalar_value(expr).is_some_and(|value| value != 0.0),
    }
}

fn is_zero_scalar(expr: &Expr) -> bool {
    match expr {
        Expr::Const(value) => *value == 0.0,
        Expr::Neg(inner) | Expr::Abs(inner) => is_zero_scalar(inner),
        Expr::Mul(left, right) => is_zero_scalar(left) || is_zero_scalar(right),
        Expr::Div(numerator, denominator) => {
            is_zero_scalar(numerator) && is_finite_nonzero_scalar(denominator)
        }
        Expr::Pow(base, exponent) => {
            is_zero_scalar(base) && constant_scalar_value(exponent).is_some_and(|value| value > 0.0)
        }
        Expr::Add(_, _) | Expr::Sub(_, _) => constant_scalar_value(expr) == Some(0.0),
        _ => false,
    }
}

/// Indica si la estructura de la expresión representa la función cero.
pub fn is_identically_zero(expr: &Expr) -> bool {
    if !has_total_real_domain(expr) {
        return false;
    }
    if is_zero_scalar(expr) {
        return true;
    }
    let mut variables = HashSet::new();
    expr.get_variables(&mut variables);
    if variables.len() <= 1 {
        let variable = variables.iter().next().map(String::as_str).unwrap_or("x");
        if collect_polynomial_coeffs(expr, variable, 20)
            .is_some_and(|coefficients| coefficients.iter().all(|coefficient| *coefficient == 0.0))
        {
            return true;
        }
    }
    match expr {
        Expr::Neg(inner) | Expr::Abs(inner) => is_identically_zero(inner),
        Expr::Add(left, right) => is_identically_zero(left) && is_identically_zero(right),
        Expr::Sub(left, right) => {
            (is_identically_zero(left) && is_identically_zero(right))
                || (left.structurally_eq(right) && left.is_guaranteed_finite())
        }
        Expr::Mul(left, right) => is_identically_zero(left) || is_identically_zero(right),
        Expr::Pow(base, exponent) => {
            is_identically_zero(base)
                && constant_scalar_value(exponent).is_some_and(|value| value > 0.0)
        }
        _ => false,
    }
}

fn has_total_real_domain(expr: &Expr) -> bool {
    match expr {
        Expr::Const(value) => value.is_finite(),
        Expr::Var(_) => true,
        Expr::Neg(inner)
        | Expr::Sin(inner)
        | Expr::Cos(inner)
        | Expr::Atan(inner)
        | Expr::Abs(inner)
        | Expr::Tanh(inner)
        | Expr::Floor(inner)
        | Expr::Ceil(inner)
        | Expr::Round(inner)
        | Expr::Sign(inner)
        | Expr::Heaviside(inner)
        | Expr::Cbrt(inner)
        | Expr::Asinh(inner) => has_total_real_domain(inner),
        Expr::Add(left, right) | Expr::Sub(left, right) | Expr::Mul(left, right) => {
            has_total_real_domain(left) && has_total_real_domain(right)
        }
        Expr::Div(numerator, denominator) => {
            has_total_real_domain(numerator) && is_finite_nonzero_scalar(denominator)
        }
        Expr::Pow(base, exponent) => {
            has_total_real_domain(base)
                && matches!(
                    exponent.as_ref(),
                    Expr::Const(value) if value.is_finite() && *value >= 0.0 && value.fract() == 0.0
                )
        }
        Expr::Min(left, right) | Expr::Max(left, right) => {
            has_total_real_domain(left) && has_total_real_domain(right)
        }
        _ => false,
    }
}

fn root_equivalent_ast(mut ast: &Expr) -> &Expr {
    loop {
        ast = match ast {
            Expr::Neg(inner) => inner,
            Expr::Mul(left, right) if is_finite_nonzero_scalar(left) => right,
            Expr::Mul(left, right) if is_finite_nonzero_scalar(right) => left,
            Expr::Div(numerator, denominator) if is_finite_nonzero_scalar(denominator) => numerator,
            Expr::Add(left, right) if is_zero_scalar(left) => right,
            Expr::Add(left, right) if is_zero_scalar(right) => left,
            Expr::Sub(left, right) if is_zero_scalar(left) => right,
            Expr::Sub(left, right) if is_zero_scalar(right) => left,
            Expr::Pow(base, exponent)
                if constant_scalar_value(exponent).is_some_and(|value| value > 0.0) =>
            {
                base
            }
            Expr::Abs(inner) => inner,
            _ => return ast,
        };
    }
}

/// Busca raíces reales de `ast(var)` en [lo, hi] por escaneo + bisección.
/// Busca raíces reales aisladas en `[lo, hi]` con residuos relativos a la
/// escala observada de la función. Los cambios de signo se comparan sin
/// multiplicar valores, evitando underflow en ecuaciones globalmente pequeñas.
pub fn find_real_roots_numeric(ast: &Expr, var: &str, lo: f64, hi: f64) -> Vec<f64> {
    const STEPS: usize = 4000;
    const MAX_ROOTS: usize = 256;
    const RELATIVE_RESIDUAL_TOLERANCE: f64 = 1e-10;
    const X_TOLERANCE: f64 = 1e-12;

    if !lo.is_finite() || !hi.is_finite() || lo == hi || !(hi - lo).is_finite() {
        return Vec::new();
    }
    if is_identically_zero(ast) {
        return Vec::new();
    }
    let ast = root_equivalent_ast(ast);
    if let Expr::Mul(left, right) = ast {
        let mut roots = find_real_roots_numeric(left, var, lo, hi);
        for root in find_real_roots_numeric(right, var, lo, hi) {
            push_unique(&mut roots, root);
        }
        roots.sort_by(f64::total_cmp);
        return roots;
    }
    let f = |x: f64| ast.eval_at(var, x);
    let dx = (hi - lo) / STEPS as f64;
    if !dx.is_finite() || dx == 0.0 {
        return Vec::new();
    }

    let samples: Vec<(f64, f64)> = (0..=STEPS)
        .map(|index| {
            let x = lo + index as f64 * dx;
            (x, f(x))
        })
        .collect();
    let scale = samples
        .iter()
        .filter_map(|(_, value)| value.is_finite().then_some(value.abs()))
        .fold(0.0_f64, f64::max);
    if scale == 0.0 || !scale.is_finite() {
        return Vec::new();
    }

    let residual_is_small = |value: f64| {
        value.is_finite() && (value == 0.0 || value.abs() / scale <= RELATIVE_RESIDUAL_TOLERANCE)
    };
    let opposite_signs =
        |left: f64, right: f64| (left < 0.0 && right > 0.0) || (left > 0.0 && right < 0.0);
    let mut roots = Vec::new();
    let endpoint_is_root = |endpoint: usize, neighbor: usize, second_neighbor: usize| {
        let (x, value) = samples[endpoint];
        let neighbor_value = samples[neighbor].1;
        let second_neighbor_value = samples[second_neighbor].1;
        if !value.is_finite() || !neighbor_value.is_finite() || !second_neighbor_value.is_finite() {
            return false;
        }
        let near_slope = (neighbor_value - value) / (samples[neighbor].0 - x);
        let next_slope = (second_neighbor_value - neighbor_value)
            / (samples[second_neighbor].0 - samples[neighbor].0);
        let slope_scale = near_slope.abs().max(next_slope.abs());
        let local_scale = neighbor_value.abs().max(second_neighbor_value.abs());
        near_slope != 0.0
            && slope_scale.is_finite()
            && local_scale > 0.0
            && value.abs() <= RELATIVE_RESIDUAL_TOLERANCE * local_scale
            && (near_slope - next_slope).abs() <= 0.1 * slope_scale
            && (value / near_slope).abs() <= 1e-10 * (1.0 + x.abs())
            && {
                let estimate = x - value / near_slope;
                (lo.min(hi)..=lo.max(hi)).contains(&estimate)
            }
    };
    if endpoint_is_root(0, 1, 2) {
        push_unique(&mut roots, samples[0].0);
    }
    let last = samples.len() - 1;
    if endpoint_is_root(last, last - 1, last - 2) {
        push_unique(&mut roots, samples[last].0);
    }
    let mut previous_nonzero = None;
    for (index, &(x, value)) in samples.iter().enumerate() {
        if roots.len() == MAX_ROOTS {
            break;
        }
        if !value.is_finite() {
            previous_nonzero = None;
            continue;
        }

        if value == 0.0 {
            let interior = index
                .checked_sub(1)
                .and_then(|left| samples.get(left))
                .zip(samples.get(index + 1))
                .is_some_and(|((_, left), (_, right))| {
                    left.is_finite() && *left != 0.0 && right.is_finite() && *right != 0.0
                });
            let boundary = (index == 0
                && samples
                    .get(1)
                    .is_some_and(|(_, right)| right.is_finite() && *right != 0.0))
                || (index + 1 == samples.len()
                    && samples
                        .get(index - 1)
                        .is_some_and(|(_, left)| left.is_finite() && *left != 0.0));
            if interior || boundary {
                push_unique(&mut roots, x);
            }
            continue;
        }

        let Some((previous_x, previous_value)) = previous_nonzero.replace((x, value)) else {
            continue;
        };
        if !opposite_signs(previous_value, value) {
            continue;
        }

        let mut left_x = previous_x;
        let mut left_value = previous_value;
        let mut right_x = x;
        let mut candidate = None;
        for _ in 0..80 {
            let mid = (left_x + right_x) * 0.5;
            let mid_value = f(mid);
            if !mid_value.is_finite() {
                candidate = None;
                break;
            }
            if mid_value == 0.0 {
                candidate = Some(mid);
                break;
            }
            if opposite_signs(left_value, mid_value) {
                right_x = mid;
            } else {
                left_x = mid;
                left_value = mid_value;
            }
            let midpoint = (left_x + right_x) * 0.5;
            if (right_x - left_x).abs() <= X_TOLERANCE * (1.0 + midpoint.abs()) {
                candidate = Some(midpoint);
                break;
            }
            candidate = Some(midpoint);
        }
        if let Some(root) = candidate.filter(|root| residual_is_small(f(*root))) {
            push_unique(&mut roots, root);
        }
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

fn push_unique(roots: &mut Vec<f64>, x: f64) {
    if !roots.iter().any(|r| (r - x).abs() < 1e-6) {
        roots.push(x);
    }
}

// ============================================================================
// Resolución de ecuaciones polinómicas
// ============================================================================

fn solve_polynomial_ast(ast: &Expr, var: &str) -> Option<Vec<f64>> {
    if is_identically_zero(ast) {
        return Some(Vec::new());
    }
    let mut roots = match ast {
        Expr::Neg(inner) | Expr::Abs(inner) => solve_polynomial_ast(inner, var)?,
        Expr::Add(left, right) if is_zero_scalar(left) => solve_polynomial_ast(right, var)?,
        Expr::Add(left, right) if is_zero_scalar(right) => solve_polynomial_ast(left, var)?,
        Expr::Sub(left, right) if is_zero_scalar(left) => solve_polynomial_ast(right, var)?,
        Expr::Sub(left, right) if is_zero_scalar(right) => solve_polynomial_ast(left, var)?,
        Expr::Mul(left, right) if is_finite_nonzero_scalar(left) => {
            solve_polynomial_ast(right, var)?
        }
        Expr::Mul(left, right) if is_finite_nonzero_scalar(right) => {
            solve_polynomial_ast(left, var)?
        }
        Expr::Mul(left, right) => {
            let mut roots = solve_polynomial_ast(left, var)?;
            roots.extend(solve_polynomial_ast(right, var)?);
            roots
        }
        Expr::Div(numerator, denominator) if is_finite_nonzero_scalar(denominator) => {
            solve_polynomial_ast(numerator, var)?
        }
        Expr::Pow(base, exponent)
            if matches!(
                exponent.as_ref(),
                Expr::Const(value) if value.is_finite() && *value > 0.0 && value.fract() == 0.0
            ) =>
        {
            solve_polynomial_ast(base, var)?
        }
        _ => {
            let coeffs = collect_polynomial_coeffs(ast, var, 4)?;
            solve_polynomial_real(&coeffs)
                .into_iter()
                .filter(|root| polynomial_ast_candidate_is_valid(ast, var, (*root, 0.0)))
                .collect()
        }
    };
    roots.sort_by(f64::total_cmp);
    roots.dedup_by(|left, right| (*left - *right).abs() <= 1e-10 * (1.0 + left.abs()));
    Some(roots)
}

/// Resolución de polinomios con raíces complejas (Durand–Kerner).
pub fn solve_polynomial_complex(ast: &Expr, var: &str) -> Option<Vec<(f64, f64)>> {
    if is_identically_zero(ast) {
        return Some(Vec::new());
    }
    let mut roots = match ast {
        Expr::Neg(inner) | Expr::Abs(inner) => solve_polynomial_complex(inner, var)?,
        Expr::Add(left, right) if is_zero_scalar(left) => solve_polynomial_complex(right, var)?,
        Expr::Add(left, right) if is_zero_scalar(right) => solve_polynomial_complex(left, var)?,
        Expr::Sub(left, right) if is_zero_scalar(left) => solve_polynomial_complex(right, var)?,
        Expr::Sub(left, right) if is_zero_scalar(right) => solve_polynomial_complex(left, var)?,
        Expr::Mul(left, right) if is_finite_nonzero_scalar(left) => {
            solve_polynomial_complex(right, var)?
        }
        Expr::Mul(left, right) if is_finite_nonzero_scalar(right) => {
            solve_polynomial_complex(left, var)?
        }
        Expr::Mul(left, right) => {
            let mut roots = solve_polynomial_complex(left, var)?;
            roots.extend(solve_polynomial_complex(right, var)?);
            roots
        }
        Expr::Div(numerator, denominator) if is_finite_nonzero_scalar(denominator) => {
            solve_polynomial_complex(numerator, var)?
        }
        Expr::Pow(base, exponent) => {
            let Expr::Const(exponent) = exponent.as_ref() else {
                return None;
            };
            if !exponent.is_finite()
                || *exponent <= 0.0
                || exponent.fract() != 0.0
                || *exponent > 20.0
            {
                return None;
            }
            let base_roots = solve_polynomial_complex(base, var)?;
            let mut roots = Vec::with_capacity(base_roots.len() * *exponent as usize);
            for _ in 0..*exponent as usize {
                roots.extend(base_roots.iter().copied());
            }
            roots
        }
        _ => {
            let raw_coeffs = collect_polynomial_coeffs(ast, var, 20)?;
            let Some(coeffs) = normalize_polynomial_coefficients(&raw_coeffs) else {
                return Some(Vec::new());
            };
            solve_polynomial_complex_coefficients(&coeffs)
                .into_iter()
                .filter(|root| polynomial_ast_candidate_is_valid(ast, var, *root))
                .collect()
        }
    };
    roots.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
    });
    Some(roots)
}

fn polynomial_ast_candidate_is_valid(ast: &Expr, var: &str, root: (f64, f64)) -> bool {
    let Some((value, scale)) = evaluate_polynomial_complex(ast, var, root) else {
        return false;
    };
    let residual = value.0.hypot(value.1);
    if residual.is_finite() && scale.is_finite() {
        return residual == 0.0 || (scale > 0.0 && residual / scale <= 1e-10);
    }
    collect_polynomial_coeffs(ast, var, 20)
        .is_some_and(|coeffs| polynomial_coefficients_candidate_is_valid(&coeffs, root))
}

fn polynomial_coefficients_candidate_is_valid(coeffs: &[f64], root: (f64, f64)) -> bool {
    let Some(coeffs) = normalize_polynomial_coefficients(coeffs) else {
        return false;
    };
    let magnitude = root.0.hypot(root.1);
    if !magnitude.is_finite() {
        return false;
    }
    let multiply = |left: (f64, f64), right: (f64, f64)| {
        (
            left.0 * right.0 - left.1 * right.1,
            left.0 * right.1 + left.1 * right.0,
        )
    };
    let (variable, coefficients): ((f64, f64), Box<dyn Iterator<Item = &f64>>) = if magnitude <= 1.0
    {
        (root, Box::new(coeffs.iter().rev()))
    } else {
        let reciprocal = if root.0.abs() >= root.1.abs() {
            let ratio = root.1 / root.0;
            let denominator = root.0 + root.1 * ratio;
            (1.0 / denominator, -ratio / denominator)
        } else {
            let ratio = root.0 / root.1;
            let denominator = root.1 + root.0 * ratio;
            (ratio / denominator, -1.0 / denominator)
        };
        (reciprocal, Box::new(coeffs.iter()))
    };
    let mut coefficients = coefficients;
    let Some(first) = coefficients.next() else {
        return false;
    };
    let mut value = (*first, 0.0);
    let mut scale = first.abs();
    for coefficient in coefficients {
        value = multiply(value, variable);
        value.0 += coefficient;
        scale = scale.mul_add(variable.0.hypot(variable.1), coefficient.abs());
    }
    let residual = value.0.hypot(value.1);
    residual.is_finite()
        && scale.is_finite()
        && (residual == 0.0 || (scale > 0.0 && residual / scale <= 1e-10))
}

fn evaluate_polynomial_complex(
    ast: &Expr,
    var: &str,
    variable: (f64, f64),
) -> Option<((f64, f64), f64)> {
    let multiply = |left: (f64, f64), right: (f64, f64)| {
        (
            left.0 * right.0 - left.1 * right.1,
            left.0 * right.1 + left.1 * right.0,
        )
    };
    match ast {
        Expr::Const(value) if value.is_finite() => Some(((*value, 0.0), value.abs())),
        Expr::Var(name) if name == var => Some((variable, variable.0.hypot(variable.1))),
        Expr::Neg(inner) => {
            let (value, scale) = evaluate_polynomial_complex(inner, var, variable)?;
            Some(((-value.0, -value.1), scale))
        }
        Expr::Add(left, right) | Expr::Sub(left, right) => {
            let (left_value, left_scale) = evaluate_polynomial_complex(left, var, variable)?;
            let (right_value, right_scale) = evaluate_polynomial_complex(right, var, variable)?;
            let right_sign = if matches!(ast, Expr::Add(_, _)) {
                1.0
            } else {
                -1.0
            };
            Some((
                (
                    left_value.0 + right_sign * right_value.0,
                    left_value.1 + right_sign * right_value.1,
                ),
                left_scale + right_scale,
            ))
        }
        Expr::Mul(left, right) => {
            let (left_value, left_scale) = evaluate_polynomial_complex(left, var, variable)?;
            let (right_value, right_scale) = evaluate_polynomial_complex(right, var, variable)?;
            Some((multiply(left_value, right_value), left_scale * right_scale))
        }
        Expr::Div(numerator, denominator) => {
            let (numerator, numerator_scale) =
                evaluate_polynomial_complex(numerator, var, variable)?;
            let (denominator, denominator_scale) =
                evaluate_polynomial_complex(denominator, var, variable)?;
            let norm = denominator.0 * denominator.0 + denominator.1 * denominator.1;
            if norm == 0.0 || !norm.is_finite() {
                return None;
            }
            Some((
                (
                    (numerator.0 * denominator.0 + numerator.1 * denominator.1) / norm,
                    (numerator.1 * denominator.0 - numerator.0 * denominator.1) / norm,
                ),
                numerator_scale / denominator_scale,
            ))
        }
        Expr::Pow(base, exponent) => {
            let Expr::Const(exponent) = exponent.as_ref() else {
                return None;
            };
            if !exponent.is_finite()
                || *exponent < 0.0
                || exponent.fract() != 0.0
                || *exponent > 20.0
            {
                return None;
            }
            let (base, base_scale) = evaluate_polynomial_complex(base, var, variable)?;
            let mut value = (1.0, 0.0);
            let mut scale = 1.0;
            for _ in 0..*exponent as usize {
                value = multiply(value, base);
                scale *= base_scale;
            }
            Some((value, scale))
        }
        _ => None,
    }
}

fn normalize_polynomial_coefficients(coeffs: &[f64]) -> Option<Vec<f64>> {
    let maximum = coeffs.iter().try_fold(0.0_f64, |scale, coefficient| {
        coefficient
            .is_finite()
            .then_some(scale.max(coefficient.abs()))
    })?;
    (maximum > 0.0).then(|| {
        let bits = maximum.to_bits();
        let exponent = (bits >> 52) & 0x7ff;
        let scale = if exponent == 0 {
            1.0_f64 * f64::from_bits(1_u64 << (63 - bits.leading_zeros()))
        } else {
            f64::from_bits(exponent << 52)
        };
        coeffs
            .iter()
            .map(|coefficient| coefficient / scale)
            .collect()
    })
}

fn solve_polynomial_complex_coefficients(coeffs: &[f64]) -> Vec<(f64, f64)> {
    let Some(degree) = coeffs.iter().rposition(|coefficient| *coefficient != 0.0) else {
        return Vec::new();
    };
    if degree == 0 {
        return Vec::new();
    }
    if degree >= 3 && coeffs[degree].abs() < coeffs[0].abs() {
        let reversed: Vec<_> = coeffs[..=degree].iter().rev().copied().collect();
        let mut roots = Vec::new();
        for mut reciprocal in solve_polynomial_complex_coefficients(&reversed) {
            if reciprocal == (0.0, 0.0) && reversed[0] != 0.0 && reversed[1] != 0.0 {
                let estimate = (-reversed[0] / reversed[1], 0.0);
                if polynomial_coefficients_candidate_is_valid(&reversed, estimate) {
                    reciprocal = estimate;
                }
            }
            let inverse = if reciprocal.0.abs() >= reciprocal.1.abs() {
                if reciprocal.0 == 0.0 {
                    continue;
                }
                let ratio = reciprocal.1 / reciprocal.0;
                let denominator = reciprocal.0 + reciprocal.1 * ratio;
                (1.0 / denominator, -ratio / denominator)
            } else {
                let ratio = reciprocal.0 / reciprocal.1;
                let denominator = reciprocal.1 + reciprocal.0 * ratio;
                (ratio / denominator, -1.0 / denominator)
            };
            if inverse.0.is_finite() && inverse.1.is_finite() {
                roots.push(inverse);
            }
        }
        roots.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });
        return roots;
    }
    let zero_multiplicity = coeffs[..degree]
        .iter()
        .take_while(|coefficient| **coefficient == 0.0)
        .count();
    let mut roots = if zero_multiplicity > 0 {
        let mut roots = vec![(0.0, 0.0); zero_multiplicity];
        roots.extend(durand_kerner(&coeffs[zero_multiplicity..=degree]));
        roots
    } else if degree >= 3 {
        if let Some((depressed, center)) = depress_polynomial(coeffs, degree) {
            solve_polynomial_complex_coefficients(&depressed)
                .into_iter()
                .map(|root| (root.0 + center, root.1))
                .collect()
        } else {
            durand_kerner(&coeffs[..=degree])
        }
    } else {
        durand_kerner(&coeffs[..=degree])
    };
    roots.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.total_cmp(&right.1))
    });
    roots
}

fn depress_polynomial(coeffs: &[f64], degree: usize) -> Option<(Vec<f64>, f64)> {
    let center = -coeffs[degree - 1] / (degree as f64 * coeffs[degree]);
    if !center.is_finite() || center == 0.0 {
        return None;
    }

    let mut translated = vec![0.0; degree + 1];
    for (target_degree, translated_coefficient) in translated.iter_mut().enumerate() {
        let mut binomial = 1.0;
        let mut sum = 0.0;
        for (source_degree, coefficient) in coeffs
            .iter()
            .copied()
            .enumerate()
            .take(degree + 1)
            .skip(target_degree)
        {
            if source_degree > target_degree {
                binomial *= source_degree as f64 / (source_degree - target_degree) as f64;
            }
            let term = coefficient * binomial * center.powi((source_degree - target_degree) as i32);
            if !term.is_finite() {
                return None;
            }
            sum += term;
        }
        if !sum.is_finite() {
            return None;
        }
        *translated_coefficient = sum;
    }
    translated[degree - 1] = 0.0;
    normalize_polynomial_coefficients(&translated).map(|translated| (translated, center))
}

fn durand_kerner(coeffs: &[f64]) -> Vec<(f64, f64)> {
    let degree = coeffs
        .iter()
        .rposition(|&coefficient| coefficient != 0.0)
        .unwrap_or(0);
    if degree == 0 {
        return vec![];
    }

    if degree == 1 {
        return solve_linear(&coeffs[..=1])
            .into_iter()
            .map(|root| (root, 0.0))
            .collect();
    }
    if degree == 2 {
        let a = coeffs[2];
        let b = coeffs[1];
        let c = coeffs[0];
        let discriminant = b.mul_add(b, -4.0 * a * c);
        if !discriminant.is_finite() {
            return Vec::new();
        }
        if discriminant < 0.0 {
            let real = -b / (2.0 * a);
            let imaginary = (-discriminant).sqrt() / (2.0 * a.abs());
            return if real.is_finite() && imaginary.is_finite() {
                vec![(real, -imaginary), (real, imaginary)]
            } else {
                Vec::new()
            };
        }
        return solve_quadratic(&[c, b, a])
            .into_iter()
            .map(|root| (root, 0.0))
            .collect();
    }

    let lead = coeffs[degree];
    let norm_coeffs: Vec<_> = coeffs[..=degree]
        .iter()
        .map(|coefficient| coefficient / lead)
        .collect();
    if norm_coeffs
        .iter()
        .any(|coefficient| !coefficient.is_finite())
    {
        return Vec::new();
    }

    let mut roots = Vec::with_capacity(degree);
    let mut angle: f64 = 0.4;
    let radius = 2.0
        * norm_coeffs[..degree]
            .iter()
            .enumerate()
            .map(|(index, coefficient)| coefficient.abs().powf(1.0 / (degree - index) as f64))
            .fold(0.0_f64, f64::max);
    if !radius.is_finite() || radius == 0.0 {
        return Vec::new();
    }
    for _ in 0..degree {
        roots.push((radius * angle.cos(), radius * angle.sin()));
        angle += std::f64::consts::TAU / (degree as f64) + 0.1;
    }

    let cmul = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) {
        (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
    };
    let cadd = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) { (a.0 + b.0, a.1 + b.1) };
    let csub = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) { (a.0 - b.0, a.1 - b.1) };
    let cdiv = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) {
        let den = b.0 * b.0 + b.1 * b.1;
        if den == 0.0 {
            return (0.0, 0.0);
        }
        ((a.0 * b.0 + a.1 * b.1) / den, (a.1 * b.0 - a.0 * b.1) / den)
    };

    let poly_eval = |z: (f64, f64)| -> (f64, f64) {
        let mut res = (norm_coeffs[0], 0.0);
        let mut zn = z;
        for &coef in norm_coeffs.iter().skip(1) {
            res = cadd(res, cmul((coef, 0.0), zn));
            zn = cmul(zn, z);
        }
        res
    };

    let mut converged = false;
    for _ in 0..1000 {
        let mut max_err = 0.0_f64;
        let mut next_roots = roots.clone();
        for i in 0..degree {
            let pz = poly_eval(roots[i]);
            let mut denom = (1.0, 0.0);
            for j in 0..degree {
                if i != j {
                    denom = cmul(denom, csub(roots[i], roots[j]));
                }
            }
            let diff = cdiv(pz, denom);
            if !diff.0.is_finite() || !diff.1.is_finite() {
                return Vec::new();
            }
            next_roots[i] = csub(roots[i], diff);
            let err = diff.0.hypot(diff.1);
            if err > max_err {
                max_err = err;
            }
        }
        roots = next_roots;
        if roots
            .iter()
            .any(|root| !root.0.is_finite() || !root.1.is_finite())
        {
            return Vec::new();
        }
        let root_scale = roots
            .iter()
            .map(|root| root.0.hypot(root.1))
            .fold(1.0_f64, f64::max);
        if max_err <= 1e-12 * root_scale {
            converged = true;
            break;
        }
    }

    let residual_is_small = |root: (f64, f64)| {
        let residual = poly_eval(root);
        let magnitude = root.0.hypot(root.1);
        let mut scale = 0.0;
        let mut power = 1.0;
        for coefficient in &norm_coeffs {
            scale += coefficient.abs() * power;
            power *= magnitude;
        }
        residual.0.is_finite()
            && residual.1.is_finite()
            && scale.is_finite()
            && residual.0.hypot(residual.1) <= 1e-10 * scale
    };
    if !converged && !roots.iter().copied().all(residual_is_small) {
        return Vec::new();
    }

    let real_roots = solve_polynomial_real(coeffs);
    let mut assigned = vec![false; roots.len()];
    for real in real_roots {
        let multiplicity = polynomial_root_multiplicity(coeffs, real).max(1);
        for _ in 0..multiplicity {
            let Some((index, _)) = roots
                .iter()
                .enumerate()
                .filter(|(index, _)| !assigned[*index])
                .min_by(|(_, left), (_, right)| {
                    (left.0 - real)
                        .hypot(left.1)
                        .total_cmp(&(right.0 - real).hypot(right.1))
                })
            else {
                break;
            };
            roots[index] = (real, 0.0);
            assigned[index] = true;
        }
    }

    roots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

fn polynomial_root_multiplicity(coeffs: &[f64], root: f64) -> usize {
    let mut polynomial = coeffs.to_vec();
    let mut multiplicity = 0;
    while polynomial.len() > 1 {
        let degree = polynomial.len() - 1;
        let mut quotient = vec![0.0; degree];
        quotient[degree - 1] = polynomial[degree];
        for index in (1..degree).rev() {
            quotient[index - 1] = polynomial[index] + root * quotient[index];
        }
        let remainder = polynomial[0] + root * quotient[0];
        if !remainder.is_finite() || remainder != 0.0 {
            break;
        }
        multiplicity += 1;
        polynomial = quotient;
    }
    multiplicity
}

fn collect_polynomial_coeffs(ast: &Expr, var: &str, max_deg: usize) -> Option<Vec<f64>> {
    fn add(left: Vec<f64>, right: Vec<f64>, sign: f64) -> Option<Vec<f64>> {
        left.into_iter()
            .zip(right)
            .map(|(left, right)| {
                let sum = left + sign * right;
                sum.is_finite().then_some(sum)
            })
            .collect()
    }

    fn multiply(left: &[f64], right: &[f64], max_deg: usize) -> Option<Vec<f64>> {
        let mut product = vec![0.0; max_deg + 1];
        for (left_degree, left_coefficient) in left.iter().copied().enumerate() {
            if left_coefficient == 0.0 {
                continue;
            }
            for (right_degree, right_coefficient) in right.iter().copied().enumerate() {
                if right_coefficient == 0.0 {
                    continue;
                }
                let degree = left_degree + right_degree;
                if degree > max_deg {
                    return None;
                }
                let term = left_coefficient * right_coefficient;
                if !term.is_finite() || term == 0.0 {
                    return None;
                }
                let coefficient = product[degree] + term;
                if !coefficient.is_finite() {
                    return None;
                }
                product[degree] = coefficient;
            }
        }
        Some(product)
    }

    fn collect(expr: &Expr, var: &str, max_deg: usize) -> Option<Vec<f64>> {
        use Expr::*;
        match expr {
            Const(value) if value.is_finite() => {
                let mut coefficients = vec![0.0; max_deg + 1];
                coefficients[0] = *value;
                Some(coefficients)
            }
            Var(name) if name == var && max_deg >= 1 => {
                let mut coefficients = vec![0.0; max_deg + 1];
                coefficients[1] = 1.0;
                Some(coefficients)
            }
            Neg(inner) => collect(inner, var, max_deg)?
                .into_iter()
                .map(|coefficient| (-coefficient).is_finite().then_some(-coefficient))
                .collect(),
            Add(left, right) => add(
                collect(left, var, max_deg)?,
                collect(right, var, max_deg)?,
                1.0,
            ),
            Sub(left, right) => add(
                collect(left, var, max_deg)?,
                collect(right, var, max_deg)?,
                -1.0,
            ),
            Mul(left, right) => multiply(
                &collect(left, var, max_deg)?,
                &collect(right, var, max_deg)?,
                max_deg,
            ),
            Div(numerator, denominator) => {
                let numerator = collect(numerator, var, max_deg)?;
                let denominator = collect(denominator, var, max_deg)?;
                if denominator[0] == 0.0
                    || denominator
                        .iter()
                        .skip(1)
                        .any(|coefficient| *coefficient != 0.0)
                {
                    return None;
                }
                numerator
                    .into_iter()
                    .map(|coefficient| {
                        let quotient = coefficient / denominator[0];
                        quotient.is_finite().then_some(quotient)
                    })
                    .collect()
            }
            Pow(base, exponent) => {
                let Expr::Const(exponent) = exponent.as_ref() else {
                    return None;
                };
                if !exponent.is_finite()
                    || *exponent < 0.0
                    || exponent.fract() != 0.0
                    || *exponent > max_deg as f64
                {
                    return None;
                }
                let base = collect(base, var, max_deg)?;
                let mut result = vec![0.0; max_deg + 1];
                result[0] = 1.0;
                for _ in 0..*exponent as usize {
                    result = multiply(&result, &base, max_deg)?;
                }
                Some(result)
            }
            _ => None,
        }
    }

    collect(ast, var, max_deg)
}

fn solve_polynomial_real(coeffs: &[f64]) -> Vec<f64> {
    let Some(coeffs) = normalize_polynomial_coefficients(coeffs) else {
        return Vec::new();
    };
    let degree = coeffs
        .iter()
        .rposition(|&coefficient| coefficient != 0.0)
        .unwrap_or(0);
    if degree == 0 {
        return Vec::new();
    }
    let zero_multiplicity = coeffs[..degree]
        .iter()
        .take_while(|coefficient| **coefficient == 0.0)
        .count();
    if zero_multiplicity > 0 {
        let mut roots = solve_polynomial_real(&coeffs[zero_multiplicity..=degree]);
        roots.push(0.0);
        roots.sort_by(f64::total_cmp);
        roots.dedup_by(|left, right| *left == *right);
        return roots;
    }
    let mut roots = match degree {
        1 => solve_linear(&coeffs),
        2 => solve_quadratic(&coeffs),
        3 => solve_cubic(&coeffs),
        4 => solve_quartic(&coeffs),
        _ => solve_polynomial_newton(&coeffs),
    };
    roots
        .retain(|root| root.is_finite() && polynomial_residual_is_small(&coeffs[..=degree], *root));
    roots.sort_by(f64::total_cmp);
    roots.dedup_by(|left, right| *left == *right);
    roots
}

fn polynomial_residual_is_small(coeffs: &[f64], x: f64) -> bool {
    let mut residual = 0.0_f64;
    let mut scale = 0.0_f64;
    for coefficient in coeffs.iter().rev() {
        residual = residual.mul_add(x, *coefficient);
        scale = scale.mul_add(x.abs(), coefficient.abs());
    }
    residual.is_finite() && scale.is_finite() && residual.abs() <= 128.0 * f64::EPSILON * scale
}

fn solve_linear(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[1];
    let b = coeffs[0];
    if a == 0.0 {
        vec![]
    } else {
        let root = -b / a;
        root.is_finite().then_some(root).into_iter().collect()
    }
}

fn solve_quadratic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[2];
    let b = coeffs[1];
    let c = coeffs[0];
    if a == 0.0 {
        return solve_linear(&[c, b]);
    }
    let squared_linear = b * b;
    let constant_term = 4.0 * a * c;
    let discriminant = b.mul_add(b, -constant_term);
    let roundoff = 16.0 * f64::EPSILON * (squared_linear.abs() + constant_term.abs());
    if discriminant < -roundoff {
        vec![]
    } else if discriminant.abs() <= roundoff {
        vec![-b / (2.0 * a)]
    } else {
        let sqrt_d = discriminant.sqrt();
        let q = -0.5 * (b + sqrt_d.copysign(b));
        let mut roots = if q == 0.0 {
            vec![0.0]
        } else {
            vec![q / a, c / q]
        };
        roots.retain(|root| root.is_finite());
        roots.sort_by(f64::total_cmp);
        roots.dedup_by(|left, right| *left == *right);
        roots
    }
}

fn solve_cubic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[3];
    let b = coeffs[2];
    let c = coeffs[1];
    let d = coeffs[0];
    if a == 0.0 {
        return solve_quadratic(&[d, c, b]);
    }
    let b = b / a;
    let c = c / a;
    let d = d / a;
    let p = c - b * b / 3.0;
    let q = d - b * c / 3.0 + 2.0 * b * b * b / 27.0;
    let discriminant = q * q / 4.0 + p * p * p / 27.0;
    let shift = -b / 3.0;
    let discriminant_scale = (q * q / 4.0).abs() + (p * p * p / 27.0).abs();
    let discriminant_tolerance = 32.0 * f64::EPSILON * discriminant_scale;
    if discriminant > discriminant_tolerance {
        let sqrt_d = discriminant.sqrt();
        let u = (-q / 2.0 + sqrt_d).cbrt();
        let v = (-q / 2.0 - sqrt_d).cbrt();
        vec![u + v + shift]
    } else if discriminant.abs() <= discriminant_tolerance {
        let u = (-q / 2.0).cbrt();
        let r1 = 2.0 * u + shift;
        let r2 = -u + shift;
        if (r1 - r2).abs() < 1e-12 {
            vec![r1]
        } else {
            vec![r1, r2]
        }
    } else {
        let r = (-p / 3.0).sqrt();
        let phi = (-q / (2.0 * r * r * r)).acos();
        let r1 = 2.0 * r * (phi / 3.0).cos() + shift;
        let r2 = 2.0 * r * ((phi + 2.0 * std::f64::consts::PI) / 3.0).cos() + shift;
        let r3 = 2.0 * r * ((phi + 4.0 * std::f64::consts::PI) / 3.0).cos() + shift;
        vec![r1, r2, r3]
    }
}

/// Resolución de cuártica vía Ferrari (raíces reales). Degenera a cúbica si el
/// coeficiente principal es cero, y cae a Newton numérico en casos patológicos.
fn solve_quartic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[4];
    let b = coeffs[3];
    let c = coeffs[2];
    let d = coeffs[1];
    let e = coeffs[0];
    if a == 0.0 {
        return solve_cubic(&[e, d, c, b]);
    }
    // Normalizar: x^4 + B x^3 + C x^2 + D x + E
    let b = b / a;
    let c = c / a;
    let d = d / a;
    let e = e / a;
    // Depresión x = y - b/4
    let p = c - 3.0 * b * b / 8.0;
    let q = d - b * c / 2.0 + b * b * b / 8.0;
    let r = e - b * d / 4.0 + b * b * c / 16.0 - 3.0 * b * b * b * b / 256.0;
    let shift = -b / 4.0;

    if q == 0.0 {
        // Bicuadrática: y^4 + p y^2 + r = 0  →  z^2 + p z + r = 0 (z = y^2)
        let zroots = solve_quadratic(&[r, p, 1.0]);
        let mut out = Vec::new();
        for z in zroots {
            if z > 0.0 {
                let s = z.sqrt();
                out.push(s + shift);
                out.push(-s + shift);
            } else if z == 0.0 {
                out.push(shift);
            }
        }
        out.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        return out;
    }

    // Cúbica resolvente: 8 z^3 + 8 p z^2 + (2 p^2 - 8 r) z - q^2 = 0
    let res_coeffs = [-q * q, 2.0 * p * p - 8.0 * r, 8.0 * p, 8.0];
    let z_roots = solve_cubic(&res_coeffs);
    let z0 = match z_roots.iter().max_by(|x, y| {
        (2.0 * *x - p)
            .partial_cmp(&(2.0 * *y - p))
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Some(z) => *z,
        None => return solve_polynomial_newton(coeffs),
    };
    let alpha2 = 2.0 * z0 - p;
    if alpha2 < 0.0 {
        return solve_polynomial_newton(coeffs);
    }
    let alpha = alpha2.sqrt();
    if alpha == 0.0 {
        return solve_polynomial_newton(coeffs);
    }
    let beta = -q / (2.0 * alpha);
    let disc1 = -(2.0 * z0 + p + 2.0 * beta);
    let disc2 = -(2.0 * z0 + p - 2.0 * beta);

    let mut out = Vec::new();
    if disc1 >= 0.0 {
        let s = disc1.sqrt();
        out.push((alpha + s) / 2.0 + shift);
        out.push((alpha - s) / 2.0 + shift);
    }
    if disc2 >= 0.0 {
        let s = disc2.sqrt();
        out.push((-alpha + s) / 2.0 + shift);
        out.push((-alpha - s) / 2.0 + shift);
    }
    if out.is_empty() {
        return solve_polynomial_newton(coeffs);
    }
    out.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut dedup = Vec::new();
    for v in out {
        if !dedup.iter().any(|r: &f64| (r - v).abs() < 1e-6) {
            dedup.push(v);
        }
    }
    dedup
}

fn solve_polynomial_newton(coeffs: &[f64]) -> Vec<f64> {
    let mut roots = Vec::new();
    let f = |x: f64| {
        let mut result = 0.0;
        let mut xn = 1.0;
        for c in coeffs {
            result += c * xn;
            xn *= x;
        }
        result
    };
    let df = |x: f64| {
        let mut result = 0.0;
        let mut xn = 1.0;
        for (i, c) in coeffs.iter().enumerate() {
            if i > 0 {
                result += (i as f64) * c * xn;
            }
            xn *= x;
        }
        result
    };
    for start in -10..=10 {
        let mut x = start as f64;
        for _ in 0..50 {
            let fx = f(x);
            if fx.abs() < 1e-10 {
                let is_dup = roots.iter().any(|r: &f64| (r - x).abs() < 1e-6);
                if !is_dup {
                    roots.push(x);
                }
                break;
            }
            let dfx = df(x);
            if dfx.abs() < 1e-15 {
                break;
            }
            x -= fx / dfx;
        }
    }
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_result(s: &str, var: &str, x: f64) -> f64 {
        parse_ast(&s.replace(' ', "")).unwrap().eval_at(var, x)
    }

    #[test]
    fn test_derivative_x_squared() {
        let r = derivative("x^2", "x").unwrap();
        // d/dx x^2 = 2x → en x=3 vale 6
        assert!((eval_result(&r, "x", 3.0) - 6.0).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn test_derivative_sin() {
        let r = derivative("sin(x)", "x").unwrap();
        // d/dx sin(x) = cos(x) → en x=0 vale 1
        assert!((eval_result(&r, "x", 0.0) - 1.0).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn test_derivative_x_cubed() {
        let r = derivative("x^3", "x").unwrap();
        // d/dx x^3 = 3x^2 → en x=2 vale 12
        assert!((eval_result(&r, "x", 2.0) - 12.0).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn test_derivative_exp() {
        let r = derivative("exp(x)", "x").unwrap();
        // d/dx exp(x) = exp(x) → en x=1 vale e
        let e = std::f64::consts::E;
        assert!((eval_result(&r, "x", 1.0) - e).abs() < 1e-6, "got {r}");
    }

    #[test]
    fn test_derivative_ln() {
        let r = derivative("ln(x)", "x").unwrap();
        // d/dx ln(x) = 1/x → en x=2 vale 0.5
        assert!((eval_result(&r, "x", 2.0) - 0.5).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn test_simplify_x_plus_zero() {
        let r = simplify("x + 0").unwrap();
        assert_eq!(r, "x");
    }

    #[test]
    fn test_simplify_const_fold() {
        let r = simplify("2 + 3").unwrap();
        assert_eq!(r, "5");
    }

    #[test]
    fn test_simplify_double_neg() {
        let r = simplify("-(-(x))").unwrap();
        assert_eq!(r, "x");
    }

    #[test]
    fn test_simplify_x_minus_x() {
        let r = simplify("x - x").unwrap();
        assert_eq!(r, "0");
    }

    #[test]
    fn test_simplify_x_over_x() {
        let r = simplify("x / x").unwrap();
        assert_eq!(r, "x / x");
    }

    #[test]
    fn simplification_preserves_possible_domain_failures() {
        assert_eq!(simplify("x / x").unwrap(), "x / x");
        assert_eq!(simplify("0 / x").unwrap(), "0 / x");
        assert_eq!(simplify("x ^ 0").unwrap(), "x ^ 0");
    }

    #[test]
    fn test_solve_quadratic_pm2() {
        let r = solve("x^2 - 4", "x").unwrap();
        assert!(r.contains("-2"), "got {r}");
        assert!(r.contains("2.000"), "got {r}");
    }

    #[test]
    fn factor_preserves_leading_coefficient_and_multiplicity() {
        for (source, points) in [("2*x - 2", [-1.0, 0.0, 2.0]), ("x^2", [-2.0, 0.0, 3.0])] {
            let factored = factor(source, "x").unwrap();
            for x in points {
                let original = eval_result(source, "x", x);
                let rebuilt = eval_result(&factored, "x", x);
                assert!(
                    (original - rebuilt).abs() < 1e-9,
                    "{source} != {factored} at x={x}"
                );
            }
        }
    }

    #[test]
    fn factor_preserves_tiny_finite_coefficients_and_roots() {
        let source = "0.000000000000001*x - 0.000000000000000000000000000001";
        let factored = factor(source, "x").unwrap();

        for x in [0.0, 0.000000000000001, 2.0] {
            let original = eval_result(source, "x", x);
            let rebuilt = eval_result(&factored, "x", x);
            let tolerance = 1e-12 * original.abs().max(1e-30);
            assert!(
                (original - rebuilt).abs() <= tolerance,
                "{source} != {factored} at x={x}: {original} != {rebuilt}"
            );
        }
    }

    #[test]
    fn factor_does_not_drop_a_subnormal_linear_coefficient() {
        let coefficient = f64::from_bits(1);
        let source = format!("x^2-{coefficient}*x");
        let factored = factor(&source, "x").unwrap();

        // El subnormal minimo debe conservarse; `x*x` no factoriza el polinomio.
        assert_eq!(factored, source);
        for x in [0.0, coefficient, 2.0 * coefficient, 1.0] {
            assert_eq!(
                eval_result(&factored, "x", x).to_bits(),
                eval_result(&source, "x", x).to_bits(),
                "{source} != {factored} at x={x}"
            );
        }
    }

    #[test]
    fn factor_keeps_tiny_coefficients_when_real_factors_are_unavailable() {
        let source = "0.000000000000001*x^2+0.000000000000001";
        assert_eq!(factor(source, "x").unwrap(), source);
    }

    #[test]
    fn test_taylor_exp() {
        let r = taylor_series("exp(x)", "x", 0.0, 3).unwrap();
        assert!(r.starts_with('1'), "got {r}");
        // Evaluar la serie en x=0.5 debe aproximarse a exp(0.5)
        let v = eval_result(&r, "x", 0.5);
        assert!(
            (v - std::f64::consts::E.powf(0.5)).abs() < 0.05,
            "got {r} → {v}"
        );
    }

    #[test]
    fn taylor_series_rejects_orders_above_the_public_limit() {
        assert!(taylor_series("x", "x", 0.0, 65).is_err());
    }

    #[test]
    fn taylor_series_rejects_accepted_orders_that_exceed_the_ast_budget() {
        let expression = std::iter::repeat("sin(x)")
            .take(16)
            .collect::<Vec<_>>()
            .join("*");

        let error = taylor_series(&expression, "x", 0.0, 64)
            .expect_err("accepted orders still require a bounded derivative workload");
        assert!(error.contains("budget"), "unexpected error: {error}");
    }

    #[test]
    fn test_limit_sinc() {
        let r = limit("sin(x)/x", "x", 0.0).unwrap();
        assert!(r.contains("1.0"), "got {r}");
    }

    #[test]
    fn typed_limit_reports_a_finite_approximation_and_error_estimate() {
        match limit_typed("sin(x)/x", "x", 0.0) {
            MathResult::Approximate {
                value,
                error_estimate,
            } => {
                assert!((value - 1.0).abs() < 1e-8);
                assert!(error_estimate.is_finite() && error_estimate >= 0.0);
            }
            result => panic!("expected a finite typed limit, got {result:?}"),
        }
    }

    #[test]
    fn typed_limit_reports_non_existence_without_a_success_string() {
        assert!(matches!(
            limit_typed("1/x", "x", 0.0),
            MathResult::DomainError(MathError::LimitDoesNotExist { .. })
        ));
    }

    #[test]
    fn typed_limit_rejects_invalid_and_unbounded_requests_structurally() {
        assert!(matches!(
            limit_typed("sin(", "x", 0.0),
            MathResult::DomainError(MathError::InvalidExpression {
                operation: MathOperation::Limit,
                ..
            })
        ));
        assert!(matches!(
            limit_typed("x", "x", f64::INFINITY),
            MathResult::Unsupported(MathError::NonFiniteLimitPoint { .. })
        ));
    }

    #[test]
    fn typed_limit_enforces_the_shared_expression_budget() {
        let expression = "x".repeat(MAX_MATH_INPUT_BYTES + 1);
        assert!(matches!(
            limit_typed(&expression, "x", 0.0),
            MathResult::ResourceLimit(MathError::InputTooLarge {
                operation: MathOperation::Limit,
                ..
            })
        ));
    }

    #[test]
    fn typed_limit_never_reports_a_non_finite_multiscale_estimate() {
        assert!(matches!(
            limit_typed("exp(708)", "x", 0.0),
            MathResult::DomainError(MathError::LimitDoesNotExist { .. })
        ));
    }

    #[test]
    fn test_solve_linear() {
        let result = solve("2*x - 4", "x").unwrap();
        assert!(result.contains("2"));
    }

    #[test]
    fn test_solve_cubic() {
        let result = solve("x^3 - x", "x").unwrap();
        assert!(result.contains('0') || result.contains("-1") || result.contains('1'));
    }

    #[test]
    fn test_integrate_sin() {
        let result = integrate("sin(x)", "x").unwrap();
        assert!(result.contains("cos"));
    }

    #[test]
    fn test_integrate_power() {
        let result = integrate("x^2", "x").unwrap();
        assert!(
            result.contains("x ^ 3") || result.contains("x^3"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn integrate_expression_constant_in_the_integration_variable() {
        let y_squared = integrate("y^2", "x").unwrap();
        assert!(
            y_squared.contains("y ^ 2") && y_squared.contains('x'),
            "got {y_squared}"
        );

        let sine_constant = integrate("sin(y)", "x").unwrap();
        assert!(
            sine_constant.contains("sin(y)") && sine_constant.contains('x'),
            "got {sine_constant}"
        );
        assert!(!sine_constant.contains("inf"), "got {sine_constant}");
    }

    #[test]
    fn fractional_powers_are_not_collected_as_integer_polynomials() {
        let ast = parse_ast("x^1.4 - 2").unwrap();
        assert!(collect_polynomial_coeffs(&ast, "x", 4).is_none());
    }

    #[test]
    fn indefinite_integral_without_antiderivative_returns_error() {
        let result = integrate("abs(x)", "x");
        assert!(
            result.is_err(),
            "an unsupported indefinite integral must not become a numeric value: {result:?}"
        );
    }

    #[test]
    fn test_integrate_definite_linear() {
        let result = integrate_definite("2*x", "x", 0.0, 3.0).unwrap();
        assert!(result.contains('9'));
    }

    #[test]
    fn definite_integral_rejects_interior_poles_and_preserves_finite_bounds() {
        assert!(integrate_definite("1/x", "x", -1.0, 1.0).is_err());
        assert!(integrate_definite("1/(x-0.5)", "x", 0.0, 1.0).is_err());
        assert!(integrate_definite("1/(x-0.1)", "x", 0.0, 1.0).is_err());

        let forward = integrate_definite("x^2", "x", 0.0, 1.0).unwrap();
        assert!(forward.contains("0.33333333"), "{forward}");
        let reversed = integrate_definite("x^2", "x", 1.0, 0.0).unwrap();
        assert!(reversed.contains("-0.33333333"), "{reversed}");
    }

    #[test]
    fn derivative_of_trigamma_is_explicitly_unsupported() {
        assert!(derivative("trigamma(x)", "x").is_err());
    }

    #[test]
    fn typed_derivative_returns_an_exact_symbolic_result() {
        let result = derivative_typed("x^2", "x");
        assert!(matches!(result, MathResult::Exact(value) if value.contains('x')));
    }

    #[test]
    fn typed_indefinite_integral_returns_an_exact_symbolic_result() {
        let result = integrate_typed("x^2", "x");
        assert!(
            matches!(result, MathResult::Exact(value) if value.contains("x ^ 3") || value.contains("x^3"))
        );
    }

    #[test]
    fn typed_indefinite_integral_reports_unsupported_antiderivatives() {
        assert!(matches!(
            integrate_typed("abs(x)", "x"),
            MathResult::Unsupported(MathError::AntiderivativeUnavailable { .. })
        ));
    }

    #[test]
    fn typed_definite_integral_reports_domain_errors() {
        assert!(matches!(
            integrate_definite_typed("1/x", "x", -1.0, 1.0),
            MathResult::DomainError(MathError::IntervalDomainViolation { .. })
        ));
    }

    #[test]
    fn typed_numerical_integral_is_approximate_with_an_error_estimate() {
        match integrate_definite_typed("abs(x)", "x", -1.0, 1.0) {
            MathResult::Approximate {
                value,
                error_estimate,
            } => {
                assert!((value - 1.0).abs() < 1e-9);
                assert!(error_estimate.is_finite() && error_estimate >= 0.0);
            }
            result => panic!("expected an approximate numerical result, got {result:?}"),
        }
    }

    #[test]
    fn typed_numerical_integral_reports_depth_exhaustion() {
        assert!(matches!(
            integrate_numerical_with_limits("x^4", "x", 0.0, 1.0, 1e-15, 0),
            MathResult::NotConverged(MathError::RecursionLimit { max_depth: 0, .. })
        ));
    }

    #[test]
    fn typed_apis_report_input_budget_exhaustion() {
        let expression = "x".repeat(MAX_MATH_INPUT_BYTES + 1);
        assert!(matches!(
            derivative_typed(&expression, "x"),
            MathResult::ResourceLimit(MathError::InputTooLarge { .. })
        ));
    }

    #[test]
    fn legacy_adapters_preserve_symbolic_and_numerical_messages() {
        assert_eq!(derivative("x", "x").unwrap(), "1");
        assert!(integrate("abs(x)", "x")
            .unwrap_err()
            .contains("No hay una antiderivada simbólica soportada"));
        assert!(integrate_definite("abs(x)", "x", -1.0, 1.0)
            .unwrap()
            .contains('\u{2248}'));
    }
}
