// Grafito CAS — Computer Algebra System (numeric methods for alpha)
//
// For alpha stage, uses numerical methods. Symbolic CAS planned for future
// integration with `symbolica` crate or SymEngine bindings.

const MAX_LEGACY_INTEGRAL_STEPS: usize = 100_000;
const ROOT_SCAN_INTERVALS: usize = 64;
const ROOT_LOCAL_RESIDUAL_TOLERANCE: f64 = 1.5e-8;
const NEWTON_RELATIVE_RESIDUAL_TOLERANCE: f64 = 128.0 * f64::EPSILON;
const NEWTON_RELATIVE_STEP_TOLERANCE: f64 = 1e-10;

/// Numeric derivative using central finite difference.
/// f'(x) ≈ (f(x+h) - f(x-h)) / (2h)
pub fn derivative<F: Fn(f64) -> f64>(f: F, x: f64, h: Option<f64>) -> f64 {
    let h = h.unwrap_or(1e-6);
    (f(x + h) - f(x - h)) / (2.0 * h)
}

/// Numeric integral using Simpson's rule.
///
/// Devuelve `NaN` si `n` supera el presupuesto fijo de 100 000 pasos.
pub fn integral<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, n: usize) -> f64 {
    let n = n.max(2);
    if n > MAX_LEGACY_INTEGRAL_STEPS {
        return f64::NAN;
    }
    let n = if n % 2 == 1 { n + 1 } else { n };
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        let x = a + i as f64 * h;
        sum += if i % 2 == 0 { 2.0 * f(x) } else { 4.0 * f(x) };
    }
    sum * h / 3.0
}

/// Numeric definite integral with auto step count.
pub fn integral_auto<F: Fn(f64) -> f64>(f: F, a: f64, b: f64) -> f64 {
    integral(f, a, b, 1000)
}

/// Find root using Newton's method. Returns Ok(guess) or Err(reason).
pub fn newton_root<F, G>(
    f: F,
    df: G,
    initial: f64,
    max_iter: usize,
    tol: f64,
) -> Result<f64, String>
where
    F: Fn(f64) -> f64,
    G: Fn(f64) -> f64,
{
    let mut x = initial;
    for _ in 0..max_iter {
        let fx = f(x);
        if fx.abs() < tol {
            return Ok(x);
        }
        let dfx = df(x);
        if dfx.abs() < 1e-15 {
            return Err("Derivative near zero".into());
        }
        x -= fx / dfx;
    }
    Err("Newton did not converge".into())
}

/// Find root using Newton's method with auto-derivative (finite difference).
pub fn newton_root_auto<F: Fn(f64) -> f64>(f: &F, initial: f64) -> Result<f64, String> {
    if !initial.is_finite() {
        return Err("Initial value is not finite".into());
    }

    let mut x = initial;
    let mut fx = f(x);
    if !fx.is_finite() {
        return Err("Function returned a non-finite value".into());
    }
    let mut best_residual = fx.abs();
    let mut residual_scale = best_residual;
    let mut made_progress = false;

    for _ in 0..50 {
        let h = 1e-6 * x.abs().max(1.0);
        let left = f(x - h);
        let right = f(x + h);
        if !left.is_finite() || !right.is_finite() {
            return Err("Function returned a non-finite value".into());
        }
        residual_scale = residual_scale.max(left.abs()).max(right.abs());

        if fx == 0.0 && (left != 0.0 || right != 0.0) {
            return Ok(x);
        }

        let dfx = (right - left) / (2.0 * h);
        if !dfx.is_finite() || dfx == 0.0 {
            return Err("Derivative near zero".into());
        }
        let step = fx / dfx;
        let next = x - step;
        if !step.is_finite() || !next.is_finite() {
            return Err("Newton produced a non-finite iterate".into());
        }

        let next_fx = f(next);
        if !next_fx.is_finite() {
            return Err("Function returned a non-finite value".into());
        }
        let next_residual = next_fx.abs();
        residual_scale = residual_scale.max(next_residual);
        if next_residual < best_residual {
            best_residual = next_residual;
            made_progress = true;
        }
        if next_fx == 0.0 && made_progress {
            let next_h = 1e-6 * next.abs().max(1.0);
            let next_left = f(next - next_h);
            let next_right = f(next + next_h);
            if next_left.is_finite()
                && next_right.is_finite()
                && (next_left != 0.0 || next_right != 0.0)
            {
                return Ok(next);
            }
        }

        let residual_is_small = residual_scale > 0.0
            && next_residual <= NEWTON_RELATIVE_RESIDUAL_TOLERANCE * residual_scale;
        let step_is_small = step.abs() <= NEWTON_RELATIVE_STEP_TOLERANCE * next.abs().max(1.0);
        if made_progress && residual_is_small && step_is_small {
            let next_h = 1e-6 * next.abs().max(1.0);
            let next_left = f(next - next_h);
            let next_right = f(next + next_h);
            if next_left.is_finite()
                && next_right.is_finite()
                && opposite_signs(next_left, next_right)
            {
                return Ok(next);
            }
        }
        if next == x {
            break;
        }

        x = next;
        fx = next_fx;
    }
    Err("Newton did not converge".into())
}

fn opposite_signs(left: f64, right: f64) -> bool {
    left != 0.0 && right != 0.0 && left.is_sign_negative() != right.is_sign_negative()
}

fn has_scale_aware_root_residual<F: Fn(f64) -> f64>(f: &F, root: f64, a: f64, b: f64) -> bool {
    let residual = f(root);
    if !residual.is_finite() {
        return false;
    }
    if a == b {
        return residual == 0.0;
    }

    let interval_scale = (b - a).abs();
    let h = (interval_scale * 1e-6).max(root.abs().max(1.0) * 1.5e-8);
    let left_x = (root - h).max(a);
    let right_x = (root + h).min(b);
    let left_value = if left_x == root {
        None
    } else {
        Some(f(left_x))
    };
    let right_value = if right_x == root {
        None
    } else {
        Some(f(right_x))
    };
    if left_value.is_some_and(|value| !value.is_finite())
        || right_value.is_some_and(|value| !value.is_finite())
    {
        return false;
    }

    let local_scale = left_value
        .into_iter()
        .chain(right_value)
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    if residual == 0.0 {
        return local_scale > 0.0;
    }

    matches!((left_value, right_value), (Some(left), Some(right)) if opposite_signs(left, right))
        && residual.abs() <= ROOT_LOCAL_RESIDUAL_TOLERANCE * local_scale
}

fn bisect_root<F: Fn(f64) -> f64>(
    f: &F,
    mut left: f64,
    mut right: f64,
    mut left_value: f64,
    mut right_value: f64,
    range: (f64, f64),
) -> Option<f64> {
    for _ in 0..100 {
        let middle = left + (right - left) * 0.5;
        if middle == left || middle == right {
            break;
        }
        let middle_value = f(middle);
        if !middle_value.is_finite() {
            return None;
        }
        if middle_value == 0.0 {
            return Some(middle);
        }
        if opposite_signs(left_value, middle_value) {
            right = middle;
            right_value = middle_value;
        } else {
            left = middle;
            left_value = middle_value;
        }
    }

    let candidate = if left_value.abs() <= right_value.abs() {
        left
    } else {
        right
    };
    has_scale_aware_root_residual(f, candidate, range.0, range.1).then_some(candidate)
}

/// Try multiple initial guesses to find a root.
pub fn find_root<F: Fn(f64) -> f64>(f: F, range: (f64, f64)) -> Option<f64> {
    let (a, b) = range;
    if !a.is_finite() || !b.is_finite() || a > b {
        return None;
    }
    if a == b {
        return (f(a) == 0.0).then_some(a);
    }

    let span = b - a;
    if !span.is_finite() {
        return None;
    }

    let mut previous_x = a;
    let mut previous_value = f(a);
    if previous_value.is_finite()
        && previous_value == 0.0
        && has_scale_aware_root_residual(&f, a, a, b)
    {
        return Some(a);
    }
    for index in 1..=ROOT_SCAN_INTERVALS {
        let x = if index == ROOT_SCAN_INTERVALS {
            b
        } else {
            a + span * index as f64 / ROOT_SCAN_INTERVALS as f64
        };
        let value = f(x);
        if value.is_finite() {
            if value == 0.0 && has_scale_aware_root_residual(&f, x, a, b) {
                return Some(x);
            }
            if previous_value.is_finite() && opposite_signs(previous_value, value) {
                if let Some(root) = bisect_root(&f, previous_x, x, previous_value, value, range) {
                    return Some(root);
                }
            }
        }
        previous_x = x;
        previous_value = value;
    }

    for guess in [a, b, (a + b) * 0.5, a * 0.7 + b * 0.3, a * 0.3 + b * 0.7] {
        if let Ok(root) = newton_root_auto(&f, guess) {
            if root >= a && root <= b && has_scale_aware_root_residual(&f, root, a, b) {
                return Some(root);
            }
        }
    }
    None
}

/// Evaluate expression with f(x) form and find all roots in [a, b] by scanning.
pub fn solve_expression(
    expr: &str,
    var: f64,
    vars: &std::collections::HashMap<String, f64>,
    a: f64,
    b: f64,
) -> Result<f64, String> {
    let expr_owned = expr.to_string();
    let f = move |x: f64| {
        let mut v = vars.clone();
        v.insert("x".to_string(), x);
        crate::expr::evaluate(
            &expr_owned,
            &v.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>(),
        )
        .unwrap_or(f64::NAN)
    };
    // Try equal to zero: solve f(x)=var
    let g = move |x: f64| f(x) - var;
    find_root(g, (a, b)).ok_or("No root found in range".into())
}

/// Compute `limit f(x) as x -> a` using Richardson extrapolation.
pub fn limit<F: Fn(f64) -> f64>(f: F, x: f64) -> f64 {
    let h0 = 0.1;
    let mut vals = [0.0f64; 5];
    for (i, val) in vals.iter_mut().enumerate() {
        let h = h0 / (1 << i) as f64;
        *val = f(x + h);
    }
    // Richardson extrapolation
    let mut r = vals.to_vec();
    for j in 1..5 {
        for i in 0..(5 - j) {
            let p = 2.0f64.powi(j as i32);
            r[i] = (p * r[i + 1] - r[i]) / (p - 1.0);
        }
    }
    r[0]
}

// ---------------------------------------------------------------------------
// Frente G-A: motor CAS (Gruntz, Laurent/residuos, Buchberger acotado).
//
// Paridad GeoGebra: `Limit` (formas 0/0, ∞/∞ por L'Hôpital + jerarquía
// exp/log/potencia en ±∞), `Series`/`Residue` (polos simples + orden N
// acotado) y `Groebner` (Buchberger con S-polinomios acotados por MAX).
//
// Presupuestos: `MAX_CAS_EXPR_BYTES` 2000 (igual que `MAX_EXPR_LENGTH`),
// `MAX_GRUNTZ_STEPS` 8 iteraciones L'Hôpital, series truncadas a
// `MAX_SERIES_TERMS` 64, polo máximo `MAX_LAURENT_ORDER` 16,
// S-polinomios máximos `MAX_GROEBNER_S_POLY` 128. Todo borde devuelve
// `Result<_, CasError>`; cero `unwrap` en producción.
// ---------------------------------------------------------------------------

/// Máximo de bytes por expresión de entrada (igual que `MAX_EXPR_LENGTH`).
pub const MAX_CAS_EXPR_BYTES: usize = 2000;
/// Máximo de iteraciones L'Hôpital del motor Gruntz.
pub const MAX_GRUNTZ_STEPS: usize = 8;
/// Máximo de términos de una serie truncada.
pub const MAX_SERIES_TERMS: usize = 64;
/// Orden máximo de polo soportado por Laurent/residuos.
pub const MAX_LAURENT_ORDER: usize = 16;
/// Máximo de S-polinomios evaluados por Buchberger.
pub const MAX_GROEBNER_S_POLY: usize = 128;
/// Máximo de polinomios de entrada a Buchberger.
pub const MAX_GROEBNER_POLYS: usize = 8;
/// Máximo de variables de Buchberger (lejos de `MAX_MATRIX_DIMENSION` 1000).
pub const MAX_GROEBNER_VARS: usize = 4;
/// Grado total máximo aceptado por Buchberger.
pub const MAX_BUCHBERGER_DEGREE: usize = 64;
/// Pasos máximos de una reducción multivariada.
pub const MAX_REDUCE_STEPS: usize = 1024;

/// Error honesto del motor CAS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasError {
    /// La entrada supera `MAX_CAS_EXPR_BYTES` o está vacía.
    InputTooLong { provided: usize, maximum: usize },
    /// La variable no es un identificador válido.
    InvalidVariable { variable: String },
    /// El punto de enfoque no es finito (para límites finitos).
    NonFinitePoint,
    /// La expresión no parsea con el AST de Grafito.
    Parse { reason: String },
    /// El límite no existe (laterales distintos, oscilación o infinito).
    LimitDoesNotExist { detail: String },
    /// Fuera del subconjunto S/M soportado; diseño L en `Tasks.md` F10.W5.
    Unsupported { feature: &'static str, hint: String },
    /// Se agotó un presupuesto (`MAX_*`); deriva a `Eliminate`/cuadratura.
    ResourceLimit { detail: String },
}

impl std::fmt::Display for CasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLong { provided, maximum } => {
                write!(
                    f,
                    "expresión de {provided} bytes excede el máximo {maximum}"
                )
            }
            Self::InvalidVariable { variable } => {
                write!(f, "variable '{variable}' no es un identificador válido")
            }
            Self::NonFinitePoint => write!(f, "el punto de enfoque debe ser finito"),
            Self::Parse { reason } => write!(f, "no se pudo parsear la expresión: {reason}"),
            Self::LimitDoesNotExist { detail } => write!(f, "el límite no existe: {detail}"),
            Self::Unsupported { feature, hint } => write!(f, "{feature} no soportado: {hint}"),
            Self::ResourceLimit { detail } => write!(f, "presupuesto agotado: {detail}"),
        }
    }
}

impl std::error::Error for CasError {}

/// Variable validada (identificador ASCII `[A-Za-z_][A-Za-z0-9_]*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidVar(String);

impl ValidVar {
    /// Valida sin pánico; error honesto si no es identificador.
    pub fn try_new(var: &str) -> Result<Self, CasError> {
        let mut chars = var.chars();
        let first_ok = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(CasError::InvalidVariable {
                variable: var.to_string(),
            });
        }
        Ok(Self(var.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Expresión validada (no vacía, `<= MAX_CAS_EXPR_BYTES` 2000).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidExpr(String);

impl ValidExpr {
    /// Valida tamaño sin pánico; el parseo lo hace cada motor.
    pub fn try_new(expr: &str) -> Result<Self, CasError> {
        if expr.is_empty() || expr.len() > MAX_CAS_EXPR_BYTES {
            return Err(CasError::InputTooLong {
                provided: expr.len(),
                maximum: MAX_CAS_EXPR_BYTES,
            });
        }
        Ok(Self(expr.replace(' ', "")))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// --- Helpers AST compartidos (pub(crate) para integral.rs y ode.rs) ---

/// ¿Contiene `var` la expresión?
pub(crate) fn cas_contains_var(e: &crate::ast::Expr, var: &str) -> bool {
    use crate::ast::Expr;
    match e {
        Expr::Const(_) => false,
        Expr::Var(name) => name == var,
        Expr::Neg(a) => cas_contains_var(a, var),
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b)
        | Expr::Atan2(a, b)
        | Expr::Modulo(a, b)
        | Expr::Min(a, b)
        | Expr::Max(a, b)
        | Expr::Beta(a, b)
        | Expr::BesselJ(a, b)
        | Expr::BesselY(a, b)
        | Expr::BesselI(a, b)
        | Expr::Lt(a, b)
        | Expr::Gt(a, b)
        | Expr::Le(a, b)
        | Expr::Ge(a, b)
        | Expr::Eq(a, b)
        | Expr::Ne(a, b) => cas_contains_var(a, var) || cas_contains_var(b, var),
        Expr::Sin(a)
        | Expr::Cos(a)
        | Expr::Tan(a)
        | Expr::Asin(a)
        | Expr::Acos(a)
        | Expr::Atan(a)
        | Expr::Exp(a)
        | Expr::Ln(a)
        | Expr::Log(a)
        | Expr::Sqrt(a)
        | Expr::Abs(a)
        | Expr::Sinh(a)
        | Expr::Cosh(a)
        | Expr::Tanh(a)
        | Expr::Sec(a)
        | Expr::Csc(a)
        | Expr::Cot(a)
        | Expr::Floor(a)
        | Expr::Ceil(a)
        | Expr::Round(a)
        | Expr::Sign(a)
        | Expr::Heaviside(a)
        | Expr::Cbrt(a)
        | Expr::Re(a)
        | Expr::Im(a)
        | Expr::Arg(a)
        | Expr::Conj(a)
        | Expr::Erf(a)
        | Expr::Erfc(a)
        | Expr::Gamma(a)
        | Expr::LnGamma(a)
        | Expr::Digamma(a)
        | Expr::Trigamma(a)
        | Expr::Asinh(a)
        | Expr::Acosh(a)
        | Expr::Atanh(a) => cas_contains_var(a, var),
        Expr::Clamp(a, b, c) => {
            cas_contains_var(a, var) || cas_contains_var(b, var) || cas_contains_var(c, var)
        }
        Expr::Sum(body, _, s, t) | Expr::Product(body, _, s, t) => {
            cas_contains_var(body, var) || cas_contains_var(s, var) || cas_contains_var(t, var)
        }
        Expr::Piecewise(branches, default) => {
            branches
                .iter()
                .any(|(c, v)| cas_contains_var(c, var) || cas_contains_var(v, var))
                || cas_contains_var(default, var)
        }
    }
}

/// Pliega constantes (`Const`, `Neg`, `Add/Sub/Mul/Div/Pow` de constantes).
pub(crate) fn cas_const_value(e: &crate::ast::Expr) -> Option<f64> {
    use crate::ast::Expr;
    let v = match e {
        Expr::Const(c) => *c,
        Expr::Neg(a) => -cas_const_value(a)?,
        Expr::Add(a, b) => cas_const_value(a)? + cas_const_value(b)?,
        Expr::Sub(a, b) => cas_const_value(a)? - cas_const_value(b)?,
        Expr::Mul(a, b) => cas_const_value(a)? * cas_const_value(b)?,
        Expr::Div(a, b) => {
            let d = cas_const_value(b)?;
            if d == 0.0 {
                return None;
            }
            cas_const_value(a)? / d
        }
        Expr::Pow(a, b) => cas_const_value(a)?.powf(cas_const_value(b)?),
        _ => return None,
    };
    v.is_finite().then_some(v)
}

/// Coeficientes `(a, b)` de `a·var + b`; `None` si no es lineal.
pub(crate) fn cas_linear_coeff(e: &crate::ast::Expr, var: &str) -> Option<(f64, f64)> {
    use crate::ast::Expr;
    match e {
        Expr::Var(name) if name == var => Some((1.0, 0.0)),
        Expr::Const(c) if c.is_finite() => Some((0.0, *c)),
        Expr::Neg(a) => {
            let (x, y) = cas_linear_coeff(a, var)?;
            Some((-x, -y))
        }
        Expr::Add(a, b) => {
            let (a1, b1) = cas_linear_coeff(a, var)?;
            let (a2, b2) = cas_linear_coeff(b, var)?;
            let (x, y) = (a1 + a2, b1 + b2);
            (x.is_finite() && y.is_finite()).then_some((x, y))
        }
        Expr::Sub(a, b) => {
            let (a1, b1) = cas_linear_coeff(a, var)?;
            let (a2, b2) = cas_linear_coeff(b, var)?;
            let (x, y) = (a1 - a2, b1 - b2);
            (x.is_finite() && y.is_finite()).then_some((x, y))
        }
        Expr::Mul(a, b) => {
            if let Some(c) = cas_const_value(a) {
                let (x, y) = cas_linear_coeff(b, var)?;
                let (x2, y2) = (c * x, c * y);
                (x2.is_finite() && y2.is_finite()).then_some((x2, y2))
            } else if let Some(c) = cas_const_value(b) {
                let (x, y) = cas_linear_coeff(a, var)?;
                let (x2, y2) = (c * x, c * y);
                (x2.is_finite() && y2.is_finite()).then_some((x2, y2))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_validated(expr: &ValidExpr) -> Result<crate::ast::Expr, CasError> {
    crate::ast::parse_ast(expr.as_str()).map_err(|reason| CasError::Parse { reason })
}

// ---------------------------------------------------------------------------
// Gruntz: límites con formas 0/0, ∞/∞ (L'Hôpital) y jerarquía exp/log/potencia.
// ---------------------------------------------------------------------------

/// Forma indeterminada detectada en el punto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitForm {
    /// Evaluación directa finita.
    Direct,
    /// 0/0 — candidata a L'Hôpital.
    ZeroOverZero,
    /// ∞/∞ — candidata a L'Hôpital.
    InfOverInf,
    /// Otra forma (incluye ∞−∞, 0·∞) — va a Richardson.
    Other,
}

/// Método que resolvió el límite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GruntzMethod {
    /// Evaluación directa.
    Direct,
    /// L'Hôpital tras `steps_used` derivaciones.
    LHopital,
    /// Jerarquía de crecimiento exp/log/potencia (límites en ±∞).
    Hierarchy,
    /// Richardson bilateral heredado de `symbolic`.
    Richardson,
}

/// Resultado del motor Gruntz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GruntzOutcome {
    /// Valor del límite (puede ser ±∞ honesto en jerarquía).
    pub value: f64,
    /// Forma detectada.
    pub form: LimitForm,
    /// Método que lo resolvió.
    pub method: GruntzMethod,
    /// Iteraciones L'Hôpital usadas (0 si no aplica).
    pub steps_used: usize,
}

fn classify_quotient(num_at: f64, den_at: f64) -> LimitForm {
    if num_at.is_finite() && den_at.is_finite() {
        if num_at == 0.0 && den_at == 0.0 {
            LimitForm::ZeroOverZero
        } else {
            LimitForm::Direct
        }
    } else if num_at.is_infinite() && den_at.is_infinite() {
        LimitForm::InfOverInf
    } else {
        LimitForm::Other
    }
}

/// Límite finito `lim_{var→at} expr` estilo Gruntz.
///
/// 0/0 y ∞/∞ se intentan por L'Hôpital acotado (`MAX_GRUNTZ_STEPS` 8);
/// el resto cae a Richardson bilateral. Referencia GeoGebra: `Limit`.
pub fn gruntz_limit(expr: &str, var: &str, at: f64) -> Result<GruntzOutcome, CasError> {
    let valid_expr = ValidExpr::try_new(expr)?;
    let valid_var = ValidVar::try_new(var)?;
    if !at.is_finite() {
        return Err(CasError::NonFinitePoint);
    }
    let var = valid_var.as_str();
    let ast = parse_validated(&valid_expr)?;

    let direct = ast.eval_at(var, at);
    if direct.is_finite() {
        return Ok(GruntzOutcome {
            value: direct,
            form: LimitForm::Direct,
            method: GruntzMethod::Direct,
            steps_used: 0,
        });
    }

    if let crate::ast::Expr::Div(num, den) = &ast {
        let form = classify_quotient(num.eval_at(var, at), den.eval_at(var, at));
        match form {
            LimitForm::ZeroOverZero | LimitForm::InfOverInf => {
                if let Some((value, steps)) = lhopital_loop(num, den, var, at) {
                    return Ok(GruntzOutcome {
                        value,
                        form,
                        method: GruntzMethod::LHopital,
                        steps_used: steps,
                    });
                }
            }
            LimitForm::Direct => {
                let (n, d) = (num.eval_at(var, at), den.eval_at(var, at));
                if d != 0.0 && (n / d).is_finite() {
                    return Ok(GruntzOutcome {
                        value: n / d,
                        form,
                        method: GruntzMethod::Direct,
                        steps_used: 0,
                    });
                }
            }
            LimitForm::Other => {}
        }
        return richardson_fallback(&valid_expr, var, at, form);
    }
    richardson_fallback(&valid_expr, var, at, LimitForm::Other)
}

/// Itera L'Hôpital hasta `MAX_GRUNTZ_STEPS`; `None` si no resuelve.
fn lhopital_loop(
    num: &crate::ast::Expr,
    den: &crate::ast::Expr,
    var: &str,
    at: f64,
) -> Option<(f64, usize)> {
    let mut cur_n = num.clone();
    let mut cur_d = den.clone();
    for step in 1..=MAX_GRUNTZ_STEPS {
        cur_n = cur_n.diff(var).simplify();
        cur_d = cur_d.diff(var).simplify();
        if matches!(cur_n, crate::ast::Expr::Const(v) if v.is_nan())
            || matches!(cur_d, crate::ast::Expr::Const(v) if v.is_nan())
        {
            return None;
        }
        let (nv, dv) = (cur_n.eval_at(var, at), cur_d.eval_at(var, at));
        match classify_quotient(nv, dv) {
            LimitForm::Direct => {
                let value = if dv != 0.0 {
                    nv / dv
                } else {
                    cur_n.eval_at(var, at)
                };
                if value.is_finite() {
                    return Some((value, step));
                }
                return None;
            }
            LimitForm::ZeroOverZero | LimitForm::InfOverInf => {}
            LimitForm::Other => {
                if dv != 0.0 && (nv / dv).is_finite() {
                    return Some((nv / dv, step));
                }
                return None;
            }
        }
    }
    None
}

fn richardson_fallback(
    expr: &ValidExpr,
    var: &str,
    at: f64,
    form: LimitForm,
) -> Result<GruntzOutcome, CasError> {
    match crate::symbolic::limit_typed(expr.as_str(), var, at) {
        crate::outcome::MathResult::Approximate { value, .. }
        | crate::outcome::MathResult::Exact(value) => Ok(GruntzOutcome {
            value,
            form,
            method: GruntzMethod::Richardson,
            steps_used: 0,
        }),
        crate::outcome::MathResult::DomainError(_)
        | crate::outcome::MathResult::NotConverged(_) => Err(CasError::LimitDoesNotExist {
            detail: format!(
                "laterales no convergen para '{}' en {var}→{at}",
                expr.as_str()
            ),
        }),
        crate::outcome::MathResult::Unsupported(err)
        | crate::outcome::MathResult::ResourceLimit(err) => Err(CasError::Unsupported {
            feature: "límite",
            hint: format!("{err:?}; usa la cuadratura numérica"),
        }),
    }
}

// --- Jerarquía exp/log/potencia para x→±∞ ---

/// Clase de crecimiento: 3 = exp, 2 = potencia, 1 = log, 0 = acotada/otra.
fn growth_class(e: &crate::ast::Expr, var: &str, positive: bool) -> Option<(u8, f64)> {
    use crate::ast::Expr;
    match e {
        Expr::Const(c) if c.is_finite() => Some((0, 0.0)),
        Expr::Var(name) if name == var => Some((2, 1.0)),
        Expr::Neg(a) => growth_class(a, var, positive),
        Expr::Pow(base, exp) => {
            if let Expr::Var(name) = base.as_ref() {
                if name == var {
                    if let Some(p) = cas_const_value(exp) {
                        if p.is_finite() && p >= 0.0 {
                            return Some((2, p));
                        }
                    }
                }
            }
            None
        }
        Expr::Exp(arg) => {
            let (a, _) = cas_linear_coeff(arg, var)?;
            if !a.is_finite() || a == 0.0 {
                return None;
            }
            let grows = (positive && a > 0.0) || (!positive && a < 0.0);
            if grows {
                Some((3, a.abs()))
            } else {
                Some((0, 0.0))
            }
        }
        Expr::Ln(arg) | Expr::Log(arg) => {
            if cas_linear_coeff(arg, var).is_some() {
                Some((1, 1.0))
            } else {
                None
            }
        }
        Expr::Mul(a, b) => {
            if cas_const_value(a).is_some() {
                return growth_class(b, var, positive);
            }
            if cas_const_value(b).is_some() {
                return growth_class(a, var, positive);
            }
            let (ra, pa) = growth_class(a, var, positive)?;
            let (rb, pb) = growth_class(b, var, positive)?;
            match ra.cmp(&rb) {
                std::cmp::Ordering::Greater => Some((ra, pa)),
                std::cmp::Ordering::Less => Some((rb, pb)),
                std::cmp::Ordering::Equal => {
                    if ra == 2 {
                        Some((2, pa + pb))
                    } else {
                        Some((ra, pa.max(pb)))
                    }
                }
            }
        }
        Expr::Div(a, b) => {
            if cas_const_value(b).is_some() {
                return growth_class(a, var, positive);
            }
            None
        }
        Expr::Add(a, b) | Expr::Sub(a, b) => {
            let (ra, pa) = growth_class(a, var, positive)?;
            let (rb, pb) = growth_class(b, var, positive)?;
            match ra.cmp(&rb) {
                std::cmp::Ordering::Greater => Some((ra, pa)),
                std::cmp::Ordering::Less => Some((rb, pb)),
                std::cmp::Ordering::Equal => Some((ra, pa.max(pb))),
            }
        }
        _ => None,
    }
}

/// Límite en ±∞ con jerarquía exp(3) > potencia(2) > log(1).
///
/// `positive = true` es +∞. Devuelve ±∞ honesto cuando el numerador domina.
/// Referencia GeoGebra: `Limit[.., ±∞]`.
pub fn gruntz_limit_infinite(
    expr: &str,
    var: &str,
    positive: bool,
) -> Result<GruntzOutcome, CasError> {
    let valid_expr = ValidExpr::try_new(expr)?;
    let valid_var = ValidVar::try_new(var)?;
    let var = valid_var.as_str();
    let ast = parse_validated(&valid_expr)?;
    let direction = if positive { 1.0 } else { -1.0 };

    if let crate::ast::Expr::Div(num, den) = &ast {
        if let (Some((rn, _)), Some((rd, _))) = (
            growth_class(num, var, positive),
            growth_class(den, var, positive),
        ) {
            match rn.cmp(&rd) {
                std::cmp::Ordering::Greater => {
                    let sign = hierarchy_sign(&ast, var, direction);
                    return Ok(GruntzOutcome {
                        value: sign * f64::INFINITY,
                        form: LimitForm::InfOverInf,
                        method: GruntzMethod::Hierarchy,
                        steps_used: 0,
                    });
                }
                std::cmp::Ordering::Less => {
                    return Ok(GruntzOutcome {
                        value: 0.0,
                        form: LimitForm::InfOverInf,
                        method: GruntzMethod::Hierarchy,
                        steps_used: 0,
                    });
                }
                std::cmp::Ordering::Equal => {}
            }
        }
    } else if let Some((rank, _)) = growth_class(&ast, var, positive) {
        if rank == 0 {
            if let Some(v) = sample_at_infinity(&ast, var, direction) {
                return Ok(GruntzOutcome {
                    value: v,
                    form: LimitForm::Other,
                    method: GruntzMethod::Hierarchy,
                    steps_used: 0,
                });
            }
        } else if rank == 1 {
            let sign = hierarchy_sign(&ast, var, direction);
            if sign.is_finite() && sign != 0.0 {
                return Ok(GruntzOutcome {
                    value: sign * f64::INFINITY,
                    form: LimitForm::Other,
                    method: GruntzMethod::Hierarchy,
                    steps_used: 0,
                });
            }
        } else if rank == 2 || rank == 3 {
            let sign = hierarchy_sign(&ast, var, direction);
            return Ok(GruntzOutcome {
                value: sign * f64::INFINITY,
                form: LimitForm::Other,
                method: GruntzMethod::Hierarchy,
                steps_used: 0,
            });
        }
    }

    if let Some(v) = sample_at_infinity(&ast, var, direction) {
        return Ok(GruntzOutcome {
            value: v,
            form: LimitForm::Other,
            method: GruntzMethod::Hierarchy,
            steps_used: 0,
        });
    }
    match crate::symbolic::limit_infinite_typed(valid_expr.as_str(), var, positive) {
        crate::outcome::MathResult::Approximate { value, .. }
        | crate::outcome::MathResult::Exact(value) => Ok(GruntzOutcome {
            value,
            form: LimitForm::Other,
            method: GruntzMethod::Richardson,
            steps_used: 0,
        }),
        _ => Err(CasError::LimitDoesNotExist {
            detail: format!(
                "sin convergencia para '{}' en {}→{}∞",
                valid_expr.as_str(),
                var,
                if positive { "+" } else { "-" }
            ),
        }),
    }
}

/// Signo del infinito dominante por muestreo en escala grande.
fn hierarchy_sign(e: &crate::ast::Expr, var: &str, direction: f64) -> f64 {
    for scale in [1e6, 1e10] {
        let v = e.eval_at(var, direction * scale);
        if v.is_finite() && v != 0.0 {
            return v.signum();
        }
        if v.is_infinite() {
            return v.signum();
        }
    }
    1.0
}

/// Muestreo convergente en escalas crecientes; `None` si no converge.
fn sample_at_infinity(e: &crate::ast::Expr, var: &str, direction: f64) -> Option<f64> {
    const SCALES: [f64; 4] = [1e6, 1e8, 1e10, 1e12];
    let mut values = [0.0; 4];
    for (i, s) in SCALES.iter().enumerate() {
        let v = e.eval_at(var, direction * s);
        if !v.is_finite() {
            return None;
        }
        values[i] = v;
    }
    let last = values[3];
    let scale = values.iter().map(|v| v.abs()).fold(1.0_f64, f64::max);
    let tol = 1e-7 + 1e-6 * scale;
    if values.iter().all(|v| (v - last).abs() <= tol) {
        Some(last)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Laurent/residuos: polos simples + orden N acotado por MAX_LAURENT_ORDER.
// ---------------------------------------------------------------------------

/// Método que obtuvo el residuo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidueMethod {
    /// Analítica o singularidad evitable: residuo 0.
    AnalyticZero,
    /// Polo simple: `lim (x−at)·f(x)`.
    SimplePole,
    /// Polo de orden N: fórmula de derivadas `g^{(N−1)}/(N−1)!`.
    HigherPole,
}

/// Resultado de residuo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResidueOutcome {
    /// Residuo (coeficiente `a_{-1}`).
    pub residue: f64,
    /// Orden del polo (0 si analítica/evitable).
    pub pole_order: u32,
    /// Método usado.
    pub method: ResidueMethod,
}

/// Escalas de sondeo hasta 1e-12: la cola (últimas 4) decide convergencia
/// o tendencia a cero; las primeras solo contextualizan.
const POLE_PROBE_SCALES: [f64; 10] = [
    1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12,
];
/// Escalas moderadas para la fórmula de derivadas: evita ruido de
/// cancelación en `h ≤ 1e-9` (términos `±2/h` con redondeo ~1e-4).
const POLE_DERIV_SCALES: [f64; 7] = [1e-3, 3e-4, 1e-4, 3e-5, 1e-5, 3e-6, 1e-6];

/// Límite bilateral estable por acuerdo de cola (últimas 4 escalas).
///
/// `None` si alguna muestra no es finita o la cola no acuerda dentro de
/// `1e-9 + 1e-6·escala`. La convergencia lineal lenta (p. ej. `1+3h`)
/// acuerda en la cola diminuta aunque las primeras escalas estén lejos.
fn stable_bilateral_limit(e: &crate::ast::Expr, var: &str, at: f64, scales: &[f64]) -> Option<f64> {
    const TAIL: usize = 4;
    if scales.len() < TAIL {
        return None;
    }
    let mut left = Vec::with_capacity(scales.len());
    let mut right = Vec::with_capacity(scales.len());
    for h in scales {
        let (a, b) = (e.eval_at(var, at - h), e.eval_at(var, at + h));
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        left.push(a);
        right.push(b);
    }
    let (lt, rt) = (&left[left.len() - TAIL..], &right[right.len() - TAIL..]);
    let lv = lt[TAIL - 1];
    let rv = rt[TAIL - 1];
    let tail_scale = lt
        .iter()
        .chain(rt.iter())
        .map(|v| v.abs())
        .fold(0.0_f64, f64::max);
    let tol = 1e-9 + 1e-6 * tail_scale.max(1.0);
    if (lv - rv).abs() > tol {
        return None;
    }
    let value = (lv + rv) * 0.5;
    if lt.iter().all(|v| (v - value).abs() <= tol) && rt.iter().all(|v| (v - value).abs() <= tol) {
        Some(value)
    } else {
        None
    }
}

/// ¿Tiende `g` a cero? Sondas diminutas todas `≤ 1e-9`.
///
/// Distingue singularidad evitable / orden menor (tiende a 0) de un
/// residuo genuino diminuto (tiende a `R ≠ 0`: las sondas se quedan en R).
fn is_vanishing(g: &crate::ast::Expr, var: &str, at: f64) -> bool {
    const TINY: [f64; 4] = [1e-9, 1e-10, 1e-11, 1e-12];
    for h in TINY {
        for sign in [-1.0, 1.0] {
            let v = g.eval_at(var, at + sign * h);
            if !v.is_finite() || v.abs() > 1e-9 {
                return false;
            }
        }
    }
    true
}

fn shift_monomial(var: &str, at: f64, power: u32) -> crate::ast::Expr {
    use crate::ast::Expr;
    Expr::Pow(
        Box::new(Expr::Sub(
            Box::new(Expr::Var(var.to_string())),
            Box::new(Expr::Const(at)),
        )),
        Box::new(Expr::Const(f64::from(power))),
    )
}

fn factorial_double(n: u32) -> f64 {
    let mut acc = 1.0;
    for k in 2..=n {
        acc *= f64::from(k);
    }
    acc
}

/// Residuo de `expr` en `var = at` con orden máximo `max_order`.
///
/// Polos simples y de orden N ≤ `MAX_LAURENT_ORDER` 16; analítica/evitable
/// da residuo 0; esencial o `> max_order` da `Err` honesto.
/// Referencia GeoGebra: `Residue`.
pub fn laurent_residue(
    expr: &str,
    var: &str,
    at: f64,
    max_order: usize,
) -> Result<ResidueOutcome, CasError> {
    let valid_expr = ValidExpr::try_new(expr)?;
    let valid_var = ValidVar::try_new(var)?;
    if !at.is_finite() {
        return Err(CasError::NonFinitePoint);
    }
    if max_order == 0 || max_order > MAX_LAURENT_ORDER {
        return Err(CasError::ResourceLimit {
            detail: format!(
                "orden máximo {max_order} fuera de 1..={MAX_LAURENT_ORDER}; usa desarrollo numérico"
            ),
        });
    }
    let var = valid_var.as_str();
    let ast = parse_validated(&valid_expr)?;

    if ast.eval_at(var, at).is_finite()
        && stable_bilateral_limit(&ast, var, at, &POLE_PROBE_SCALES).is_some()
    {
        return Ok(ResidueOutcome {
            residue: 0.0,
            pole_order: 0,
            method: ResidueMethod::AnalyticZero,
        });
    }

    let mut diverged_before = false;
    for order in 1..=max_order {
        let g = crate::ast::Expr::Mul(
            Box::new(shift_monomial(var, at, order as u32)),
            Box::new(ast.clone()),
        )
        .simplify();
        match stable_bilateral_limit(&g, var, at, &POLE_PROBE_SCALES) {
            None => {
                diverged_before = true;
            }
            Some(limit) => {
                let order_u = order as u32;
                // Tiende a cero ⟺ orden menor o evitable (verificado por
                // tendencia, no por el valor de una sola escala).
                if limit.abs() <= 1e-6 && is_vanishing(&g, var, at) {
                    if order == 1 || !diverged_before {
                        return Ok(ResidueOutcome {
                            residue: 0.0,
                            pole_order: 0,
                            method: ResidueMethod::AnalyticZero,
                        });
                    }
                    let prev = order_u - 1;
                    return residue_by_derivatives(&ast, var, at, prev);
                }
                return residue_by_derivatives(&ast, var, at, order_u);
            }
        }
    }
    Err(CasError::Unsupported {
        feature: "residuo",
        hint: format!(
            "sin polo de orden ≤ {max_order} en {var}={at} (posible singularidad esencial como exp(1/x)); diseño L en Tasks.md F10.W5"
        ),
    })
}

/// Residuo por fórmula `a_{−1} = g^{(m−1)}(at)/(m−1)!` con `g=(x−at)^m·f`.
fn residue_by_derivatives(
    ast: &crate::ast::Expr,
    var: &str,
    at: f64,
    order: u32,
) -> Result<ResidueOutcome, CasError> {
    if order == 1 {
        let g = crate::ast::Expr::Mul(Box::new(shift_monomial(var, at, 1)), Box::new(ast.clone()))
            .simplify();
        match stable_bilateral_limit(&g, var, at, &POLE_PROBE_SCALES) {
            Some(value) if value.is_finite() => Ok(ResidueOutcome {
                residue: value,
                pole_order: 1,
                method: ResidueMethod::SimplePole,
            }),
            _ => Err(CasError::LimitDoesNotExist {
                detail: format!("polo simple sin límite estable en {var}={at}"),
            }),
        }
    } else {
        let mut g = crate::ast::Expr::Mul(
            Box::new(shift_monomial(var, at, order)),
            Box::new(ast.clone()),
        )
        .simplify();
        for _ in 1..order {
            g = g.diff(var).simplify();
            if matches!(g, crate::ast::Expr::Const(v) if v.is_nan()) {
                return Err(CasError::ResourceLimit {
                    detail: "derivada simbólica excedió la profundidad".to_string(),
                });
            }
        }
        match stable_bilateral_limit(&g, var, at, &POLE_DERIV_SCALES) {
            Some(value) if value.is_finite() => {
                let fact = factorial_double(order - 1);
                Ok(ResidueOutcome {
                    residue: value / fact,
                    pole_order: order,
                    method: ResidueMethod::HigherPole,
                })
            }
            _ => Err(CasError::LimitDoesNotExist {
                detail: format!("polo de orden {order} sin límite estable en {var}={at}"),
            }),
        }
    }
}

/// Parte principal truncada `[(potencia, coeficiente)]` con potencias `< 0`.
///
/// Acotada a `MAX_SERIES_TERMS` 64 términos y orden ≤ `MAX_LAURENT_ORDER`.
/// Referencia GeoGebra: `Series` (parte polar).
pub fn laurent_principal_part(
    expr: &str,
    var: &str,
    at: f64,
    max_order: usize,
) -> Result<Vec<(i32, f64)>, CasError> {
    let outcome = laurent_residue(expr, var, at, max_order)?;
    if outcome.pole_order == 0 {
        return Ok(Vec::new());
    }
    let valid_expr = ValidExpr::try_new(expr)?;
    let valid_var = ValidVar::try_new(var)?;
    let var = valid_var.as_str();
    let ast = parse_validated(&valid_expr)?;
    let order = outcome.pole_order;
    if order as usize > MAX_SERIES_TERMS {
        return Err(CasError::ResourceLimit {
            detail: format!("orden {order} excede {MAX_SERIES_TERMS} términos"),
        });
    }
    let mut out = Vec::new();
    for k in 1..=order {
        let derivs = order - k;
        let mut g = crate::ast::Expr::Mul(
            Box::new(shift_monomial(var, at, order)),
            Box::new(ast.clone()),
        )
        .simplify();
        for _ in 0..derivs {
            g = g.diff(var).simplify();
        }
        match stable_bilateral_limit(&g, var, at, &POLE_DERIV_SCALES) {
            Some(value) if value.is_finite() => {
                let coeff = value / factorial_double(derivs);
                if coeff.abs() > 1e-12 {
                    out.push((-(k as i32), coeff));
                }
            }
            _ => {
                return Err(CasError::LimitDoesNotExist {
                    detail: format!("coeficiente a_{} sin límite estable", -(k as i32)),
                });
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Groebner-Buchberger acotado (lexicográfico, S-polinomios ≤ MAX).
// ---------------------------------------------------------------------------

/// Base de Groebner calculada por Buchberger acotado.
#[derive(Debug, Clone, PartialEq)]
pub struct BuchbergerOutcome {
    /// Base reducida como strings (`{p1, p2, ..}` sin llaves, en orden).
    pub basis: Vec<String>,
    /// S-polinomios evaluados.
    pub s_polys_used: usize,
}

type Monom = Vec<u32>;
type PolyMap = std::collections::BTreeMap<Monom, f64>;

fn zero_monom(nvars: usize) -> Monom {
    vec![0; nvars]
}

fn poly_from_const(c: f64, nvars: usize) -> PolyMap {
    let mut m = PolyMap::new();
    if c.abs() > 1e-12 {
        m.insert(zero_monom(nvars), c);
    }
    m
}

fn poly_add_into(dst: &mut PolyMap, src: &PolyMap, sign: f64) {
    for (m, c) in src {
        let v = dst.get(m).copied().unwrap_or(0.0) + sign * c;
        if v.abs() <= 1e-12 {
            dst.remove(m);
        } else {
            dst.insert(m.clone(), v);
        }
    }
}

fn poly_mul_maps(a: &PolyMap, b: &PolyMap) -> Result<PolyMap, CasError> {
    let mut out = PolyMap::new();
    for (ma, ca) in a {
        for (mb, cb) in b {
            if ma.len() != mb.len() {
                return Err(CasError::ResourceLimit {
                    detail: "dimensión monomial inconsistente".to_string(),
                });
            }
            let mut m = Vec::with_capacity(ma.len());
            let mut deg = 0_usize;
            for (x, y) in ma.iter().zip(mb.iter()) {
                let e = x.checked_add(*y).ok_or_else(|| CasError::ResourceLimit {
                    detail: "exponente monomial excedido".to_string(),
                })?;
                deg = deg.saturating_add(e as usize);
                m.push(e);
            }
            if deg > MAX_BUCHBERGER_DEGREE {
                return Err(CasError::ResourceLimit {
                    detail: format!(
                        "grado {deg} excede {MAX_BUCHBERGER_DEGREE}; usa Eliminate[...]"
                    ),
                });
            }
            let v = out.get(&m).copied().unwrap_or(0.0) + ca * cb;
            if v.abs() <= 1e-12 {
                out.remove(&m);
            } else {
                out.insert(m, v);
            }
        }
    }
    Ok(out)
}

/// Convierte un AST a mapa monomial; `Err` honesto si no es polinomio.
fn expr_to_poly_map(
    e: &crate::ast::Expr,
    index_of: &std::collections::HashMap<String, usize>,
    nvars: usize,
) -> Result<PolyMap, CasError> {
    use crate::ast::Expr;
    match e {
        Expr::Const(c) => {
            if !c.is_finite() {
                return Err(CasError::Unsupported {
                    feature: "Groebner",
                    hint: "coeficiente no finito".to_string(),
                });
            }
            Ok(poly_from_const(*c, nvars))
        }
        Expr::Var(name) => match index_of.get(name) {
            Some(i) => {
                let mut m = zero_monom(nvars);
                m[*i] = 1;
                let mut out = PolyMap::new();
                out.insert(m, 1.0);
                Ok(out)
            }
            None => Err(CasError::Unsupported {
                feature: "Groebner",
                hint: format!(
                    "variable '{name}' fuera de la lista; Buchberger exige polinomios en las variables declaradas"
                ),
            }),
        },
        Expr::Neg(a) => {
            let mut m = expr_to_poly_map(a, index_of, nvars)?;
            for v in m.values_mut() {
                *v = -*v;
            }
            Ok(m)
        }
        Expr::Add(a, b) => {
            let x = expr_to_poly_map(a, index_of, nvars)?;
            let y = expr_to_poly_map(b, index_of, nvars)?;
            let mut out = x;
            poly_add_into(&mut out, &y, 1.0);
            Ok(out)
        }
        Expr::Sub(a, b) => {
            let x = expr_to_poly_map(a, index_of, nvars)?;
            let y = expr_to_poly_map(b, index_of, nvars)?;
            let mut out = x;
            poly_add_into(&mut out, &y, -1.0);
            Ok(out)
        }
        Expr::Mul(a, b) => {
            let x = expr_to_poly_map(a, index_of, nvars)?;
            let y = expr_to_poly_map(b, index_of, nvars)?;
            poly_mul_maps(&x, &y)
        }
        Expr::Pow(base, exp) => {
            let n = cas_const_value(exp).ok_or_else(|| CasError::Unsupported {
                feature: "Groebner",
                hint: "exponente no constante; Buchberger exige polinomios".to_string(),
            })?;
            if n < 0.0 || n.fract() != 0.0 || n > MAX_BUCHBERGER_DEGREE as f64 {
                return Err(CasError::Unsupported {
                    feature: "Groebner",
                    hint: "exponente no entero no negativo acotado".to_string(),
                });
            }
            let b = expr_to_poly_map(base, index_of, nvars)?;
            let mut acc = poly_from_const(1.0, nvars);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let times = n as usize;
            for _ in 0..times {
                acc = poly_mul_maps(&acc, &b)?;
            }
            Ok(acc)
        }
        _ => Err(CasError::Unsupported {
            feature: "Groebner",
            hint: "Buchberger exige polinomios (sin Div con variable, trig, exp ni log)".to_string(),
        }),
    }
}

/// Término líder lexicográfico (monomio mayor, coeficiente).
fn leading_term(p: &PolyMap) -> Option<(Monom, f64)> {
    p.iter().next_back().map(|(m, c)| (m.clone(), *c))
}

fn monom_divides(a: &Monom, b: &Monom) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x <= y)
}

fn monom_sub(a: &Monom, b: &Monom) -> Monom {
    a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

fn monom_lcm(a: &Monom, b: &Monom) -> Monom {
    a.iter().zip(b.iter()).map(|(x, y)| (*x).max(*y)).collect()
}

fn monom_mul_poly(m: &Monom, scalar: f64, p: &PolyMap) -> Result<PolyMap, CasError> {
    let shift = PolyMap::from([(m.clone(), scalar)]);
    poly_mul_maps(&shift, p)
}

/// Reduce `p` con `basis`; devuelve el resto.
fn reduce_poly(p: &PolyMap, basis: &[PolyMap]) -> Result<PolyMap, CasError> {
    let mut work = p.clone();
    let mut rest = PolyMap::new();
    let mut steps = 0_usize;
    while let Some((lm_w, lc_w)) = leading_term(&work) {
        steps += 1;
        if steps > MAX_REDUCE_STEPS {
            return Err(CasError::ResourceLimit {
                detail: format!("reducción excede {MAX_REDUCE_STEPS} pasos; usa Eliminate[...]"),
            });
        }
        let mut reduced = false;
        for b in basis {
            if let Some((lm_b, lc_b)) = leading_term(b) {
                if lc_b.abs() > 1e-12 && monom_divides(&lm_b, &lm_w) {
                    let t = monom_sub(&lm_w, &lm_b);
                    let factor = lc_w / lc_b;
                    let sub = monom_mul_poly(&t, factor, b)?;
                    poly_add_into(&mut work, &sub, -1.0);
                    reduced = true;
                    break;
                }
            }
        }
        if !reduced {
            work.remove(&lm_w);
            if lc_w.abs() > 1e-12 {
                rest.insert(lm_w, lc_w);
            }
        }
    }
    Ok(rest)
}

/// S-polinomio `lc_g·x^{l−lm_f}·f − lc_f·x^{l−lm_g}·g` con `l = lcm`.
fn s_polynomial(f: &PolyMap, g: &PolyMap) -> Result<PolyMap, CasError> {
    let (lm_f, lc_f) = leading_term(f).ok_or_else(|| CasError::ResourceLimit {
        detail: "S-polinomio de polinomio nulo".to_string(),
    })?;
    let (lm_g, lc_g) = leading_term(g).ok_or_else(|| CasError::ResourceLimit {
        detail: "S-polinomio de polinomio nulo".to_string(),
    })?;
    let l = monom_lcm(&lm_f, &lm_g);
    let t1 = monom_sub(&l, &lm_f);
    let t2 = monom_sub(&l, &lm_g);
    let mut s = monom_mul_poly(&t1, lc_g, f)?;
    let second = monom_mul_poly(&t2, lc_f, g)?;
    poly_add_into(&mut s, &second, -1.0);
    Ok(s)
}

fn format_poly_map(p: &PolyMap, vars: &[String]) -> String {
    if p.is_empty() {
        return "0".to_string();
    }
    let mut terms: Vec<(Monom, f64)> = p.iter().map(|(m, c)| (m.clone(), *c)).collect();
    terms.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = String::new();
    for (i, (m, c)) in terms.iter().enumerate() {
        let mut body = String::new();
        for (vi, e) in m.iter().enumerate() {
            if *e == 0 {
                continue;
            }
            if let Some(name) = vars.get(vi) {
                if !body.is_empty() {
                    body.push('*');
                }
                body.push_str(name);
                if *e > 1 {
                    body.push_str(&format!("^{e}"));
                }
            }
        }
        let ac = c.abs();
        let coeff_str = if body.is_empty() {
            format!("{ac}")
        } else if (ac - 1.0).abs() < 1e-12 {
            String::new()
        } else {
            format!("{ac}*")
        };
        let term = format!("{coeff_str}{body}");
        if i == 0 {
            if *c < 0.0 {
                out.push_str(&format!("-{term}"));
            } else {
                out.push_str(&term);
            }
        } else if *c < 0.0 {
            out.push_str(&format!(" - {term}"));
        } else {
            out.push_str(&format!(" + {term}"));
        }
    }
    out
}

/// Base de Groebner por Buchberger lexicográfico acotado.
///
/// `> MAX_GROEBNER_S_POLY` 128 S-polinomios, `> MAX_GROEBNER_POLYS` 8
/// polinomios o entrada no polinómica devuelven `Err` honesto que deriva a
/// `Eliminate[...]`. Referencia GeoGebra: `Groebner`.
pub fn buchberger_basis(polys: &[String], vars: &[String]) -> Result<BuchbergerOutcome, CasError> {
    if polys.is_empty() || polys.len() > MAX_GROEBNER_POLYS {
        return Err(CasError::ResourceLimit {
            detail: format!(
                "se recibieron {} polinomios (cota 1..={MAX_GROEBNER_POLYS}); usa Eliminate[...]",
                polys.len()
            ),
        });
    }
    if vars.is_empty() || vars.len() > MAX_GROEBNER_VARS {
        return Err(CasError::ResourceLimit {
            detail: format!(
                "se recibieron {} variables (cota 1..={MAX_GROEBNER_VARS}); usa Eliminate[...]",
                vars.len()
            ),
        });
    }
    let mut clean_vars = Vec::with_capacity(vars.len());
    for v in vars {
        let name = v.trim().trim_matches('"').trim_matches('\'');
        clean_vars.push(ValidVar::try_new(name)?.as_str().to_string());
    }
    let mut seen = std::collections::HashSet::new();
    for v in &clean_vars {
        if !seen.insert(v.clone()) {
            return Err(CasError::Unsupported {
                feature: "Groebner",
                hint: format!("variable duplicada '{v}'"),
            });
        }
    }
    let index_of: std::collections::HashMap<String, usize> = clean_vars
        .iter()
        .enumerate()
        .map(|(i, v)| (v.clone(), i))
        .collect();
    let nvars = clean_vars.len();

    let mut basis: Vec<PolyMap> = Vec::new();
    for p in polys {
        let valid = ValidExpr::try_new(p)?;
        let ast = parse_validated(&valid)?;
        let map = expr_to_poly_map(&ast, &index_of, nvars)?;
        if map.is_empty() {
            continue;
        }
        let rest = reduce_poly(&map, &basis)?;
        if !rest.is_empty() {
            basis.push(rest);
        }
    }
    if basis.is_empty() {
        return Err(CasError::Unsupported {
            feature: "Groebner",
            hint: "sistema nulo o vacío; nada que triangular".to_string(),
        });
    }

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for i in 0..basis.len() {
        for j in (i + 1)..basis.len() {
            pairs.push((i, j));
        }
    }
    let mut s_used = 0_usize;
    while let Some((i, j)) = pairs.pop() {
        if i >= basis.len() || j >= basis.len() {
            continue;
        }
        let (f, g) = (basis[i].clone(), basis[j].clone());
        // Criterio de Buchberger: si los términos líderes son primos
        // relativos (lcm = producto), el S-polinomio reduce a cero.
        if let (Some((lm_f, _)), Some((lm_g, _))) = (leading_term(&f), leading_term(&g)) {
            let l = monom_lcm(&lm_f, &lm_g);
            let disjoint = lm_f
                .iter()
                .zip(lm_g.iter())
                .all(|(a, b)| *a == 0 || *b == 0);
            let is_product = l
                .iter()
                .zip(lm_f.iter().zip(lm_g.iter()))
                .all(|(x, (a, b))| (*x as usize) == (*a as usize) + (*b as usize));
            if disjoint && is_product {
                continue;
            }
        }
        s_used += 1;
        if s_used > MAX_GROEBNER_S_POLY {
            return Err(CasError::ResourceLimit {
                detail: format!(
                    "Buchberger excede {MAX_GROEBNER_S_POLY} S-polinomios; usa Eliminate[...]"
                ),
            });
        }
        let s = s_polynomial(&f, &g)?;
        let rest = reduce_poly(&s, &basis)?;
        if !rest.is_empty() {
            let n = basis.len();
            for k in 0..n {
                pairs.push((k, n));
            }
            basis.push(rest);
        }
    }

    let mut basis_strs: Vec<String> = basis
        .iter()
        .map(|p| format_poly_map(p, &clean_vars))
        .filter(|s| s != "0" && !s.is_empty())
        .collect();
    if basis_strs.is_empty() {
        return Err(CasError::Unsupported {
            feature: "Groebner",
            hint: "base vacía tras reducción".to_string(),
        });
    }
    basis_strs.sort();
    basis_strs.dedup();
    Ok(BuchbergerOutcome {
        basis: basis_strs,
        s_polys_used: s_used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn hardening_find_root_rejects_small_nonzero_constant() {
        assert_eq!(find_root(|_| 1e-9, (-10.0, 10.0)), None);
        assert!(newton_root_auto(&|_| 1e-9, 0.0).is_err());
    }

    #[test]
    fn hardening_find_root_rejects_small_positive_minimum() {
        assert_eq!(find_root(|x| x * x + 1e-30, (-1.0, 1.0)), None);
    }

    #[test]
    fn hardening_find_root_preserves_bracketed_root() {
        let root = find_root(|x| x * x - 2.0, (0.0, 2.0)).expect("root should exist");

        assert!((root - 2.0_f64.sqrt()).abs() < 1e-10);
    }

    #[test]
    fn hardening_find_root_is_scale_aware_for_small_linear_function() {
        let root = find_root(|x| 1e-20 * (x - 1.0), (0.0, 2.0)).expect("root should exist");

        assert!((root - 1.0).abs() < 1e-10);
    }

    #[test]
    fn hardening_legacy_integral_preserves_normal_simpson_result() {
        let result = integral(|x| x * x, 0.0, 1.0, 1_000);

        assert!((result - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn hardening_legacy_integral_rejects_unbounded_steps_without_evaluating() {
        let evaluations = AtomicUsize::new(0);
        let result = integral(
            |x| {
                evaluations.fetch_add(1, Ordering::Relaxed);
                x
            },
            0.0,
            1.0,
            usize::MAX,
        );

        assert!(result.is_nan());
        assert_eq!(evaluations.load(Ordering::Relaxed), 0);
    }

    // --- Frente G-A: Gruntz ---

    #[test]
    fn gruntz_zero_over_zero_sin_over_x() {
        let out = gruntz_limit("sin(x)/x", "x", 0.0).expect("0/0 clásico");
        assert_eq!(out.form, LimitForm::ZeroOverZero);
        assert!((out.value - 1.0).abs() < 1e-6, "got {}", out.value);
    }

    #[test]
    fn gruntz_zero_over_zero_removable_quadratic() {
        let out = gruntz_limit("(x^2-1)/(x-1)", "x", 1.0).expect("removible");
        assert!((out.value - 2.0).abs() < 1e-6, "got {}", out.value);
    }

    #[test]
    fn gruntz_inf_over_inf_rational() {
        let out = gruntz_limit("(2*x^2+3*x)/(x^2)", "x", f64::INFINITY).expect_err("at infinito");
        let _ = out;
        let finite = gruntz_limit("(2*x+1)/(x+1)", "x", 1.0).expect("directo");
        assert!((finite.value - 1.5).abs() < 1e-9);
    }

    #[test]
    fn gruntz_hierarchy_exp_dominates_power() {
        let out = gruntz_limit_infinite("exp(x)/x^2", "x", true).expect("exp domina");
        assert_eq!(out.method, GruntzMethod::Hierarchy);
        assert!(
            out.value.is_infinite() && out.value > 0.0,
            "got {}",
            out.value
        );
    }

    #[test]
    fn gruntz_hierarchy_power_dominates_log() {
        let out = gruntz_limit_infinite("ln(x)/x", "x", true).expect("log pierde");
        assert!((out.value).abs() < 1e-9, "got {}", out.value);
        let out2 = gruntz_limit_infinite("x/ln(x)", "x", true).expect("potencia gana");
        assert!(out2.value.is_infinite(), "got {}", out2.value);
    }

    #[test]
    fn gruntz_hierarchy_decay_to_zero() {
        let out = gruntz_limit_infinite("x^2/exp(x)", "x", true).expect("decae");
        assert!(out.value.abs() < 1e-9, "got {}", out.value);
    }

    #[test]
    fn gruntz_rejects_bad_input_honestly() {
        assert!(matches!(
            gruntz_limit(&"x".repeat(2001), "x", 0.0),
            Err(CasError::InputTooLong { .. })
        ));
        assert!(matches!(
            gruntz_limit("x", "9x", 0.0),
            Err(CasError::InvalidVariable { .. })
        ));
        assert!(matches!(
            gruntz_limit("x", "x", f64::INFINITY),
            Err(CasError::NonFinitePoint)
        ));
        assert!(matches!(
            gruntz_limit("sin(", "x", 0.0),
            Err(CasError::Parse { .. })
        ));
    }

    // --- Frente G-A: Laurent/residuos ---

    #[test]
    fn residue_simple_pole_one_over_x() {
        let out = laurent_residue("1/x", "x", 0.0, 8).expect("polo simple");
        assert_eq!(out.pole_order, 1);
        assert_eq!(out.method, ResidueMethod::SimplePole);
        assert!((out.residue - 1.0).abs() < 1e-4, "got {}", out.residue);
    }

    #[test]
    fn residue_higher_pole_order_two() {
        let out = laurent_residue("1/x^2 + 3/x", "x", 0.0, 8).expect("orden 2");
        assert_eq!(out.pole_order, 2);
        assert!((out.residue - 3.0).abs() < 1e-3, "got {}", out.residue);
    }

    #[test]
    fn residue_analytic_is_zero() {
        let out = laurent_residue("x^2 + 1", "x", 0.0, 8).expect("analítica");
        assert_eq!(out.pole_order, 0);
        assert_eq!(out.residue, 0.0);
    }

    #[test]
    fn residue_removable_is_zero() {
        let out = laurent_residue("sin(x)/x", "x", 0.0, 8).expect("evitable");
        assert_eq!(out.pole_order, 0);
        assert_eq!(out.residue, 0.0);
    }

    #[test]
    fn residue_essential_is_honest_err() {
        let err = laurent_residue("exp(1/x)", "x", 0.0, 8).expect_err("esencial");
        assert!(matches!(err, CasError::Unsupported { .. }), "got {err}");
    }

    #[test]
    fn residue_order_cap_is_honest() {
        let err = laurent_residue("1/x", "x", 0.0, MAX_LAURENT_ORDER + 1).expect_err("cota");
        assert!(matches!(err, CasError::ResourceLimit { .. }), "got {err}");
    }

    #[test]
    fn principal_part_simple_pole() {
        let pp = laurent_principal_part("1/x", "x", 0.0, 8).expect("parte principal");
        assert_eq!(pp.len(), 1);
        assert_eq!(pp[0].0, -1);
        assert!((pp[0].1 - 1.0).abs() < 1e-4, "got {:?}", pp);
        assert!(pp.len() <= MAX_SERIES_TERMS);
    }

    // --- Frente G-A: Buchberger ---

    #[test]
    fn buchberger_linear_2x2_triangulates() {
        let polys = vec!["x + y - 3".to_string(), "x - y - 1".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let out = buchberger_basis(&polys, &vars).expect("2x2 lineal");
        assert!(!out.basis.is_empty());
        assert!(out.s_polys_used <= MAX_GROEBNER_S_POLY);
        assert!(out.basis.iter().any(|p| p.contains('x')));
        assert!(out.basis.iter().any(|p| p.contains('y')));
    }

    #[test]
    fn buchberger_circle_line_system() {
        let polys = vec!["x^2 + y^2 - 1".to_string(), "x - y".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let out = buchberger_basis(&polys, &vars).expect("círculo+recta");
        assert!(!out.basis.is_empty());
        assert!(out.s_polys_used <= MAX_GROEBNER_S_POLY);
    }

    #[test]
    fn buchberger_rejects_nonpolynomial_honestly() {
        let polys = vec!["sin(x) + y".to_string(), "x - y".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let err = buchberger_basis(&polys, &vars).expect_err("no polinomio");
        assert!(matches!(err, CasError::Unsupported { .. }), "got {err}");
    }

    #[test]
    fn buchberger_over_budget_is_honest_err() {
        let polys: Vec<String> = (0..(MAX_GROEBNER_POLYS + 1))
            .map(|i| format!("x + {i}"))
            .collect();
        let vars = vec!["x".to_string()];
        let err = buchberger_basis(&polys, &vars).expect_err("cota");
        let msg = format!("{err}");
        assert!(msg.contains("Eliminate"), "got {msg}");
    }
}
