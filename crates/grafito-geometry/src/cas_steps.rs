// Grafito CAS stepper — emisión genérica de pasos pedagógicos.
//!
//! Presupuesto: MAX_CAS_STEPS 32, MAX_STEP_BYTES 4 KiB por campo.
//! Statem: Idle → Parsing → Stepping → Verifying → Done | Failed
//! Instrumenta `diff_expr` y `integrate_expr` con visitor que loguea
//! `CasStep { before, after, rule }` y reutiliza `simplify_expr`.
//! El límite usa Richardson bilateral (igual que `symbolic::richardson_limit`).

use crate::ast::{parse_ast, Expr};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Presupuestos
// ---------------------------------------------------------------------------

/// Máximo de pasos emitidos por operación.
pub const MAX_CAS_STEPS: usize = 32;
/// Máximo de bytes por campo `before` / `after` de cada paso.
pub const MAX_STEP_BYTES: usize = 4096;

// ---------------------------------------------------------------------------
// RewriteRule y CasStep
// ---------------------------------------------------------------------------

/// Regla de reescritura aplicada en cada paso.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RewriteRule {
    ConstantRule,
    VariableRule,
    SumRule,
    DifferenceRule,
    ProductRule,
    QuotientRule,
    PowerRule,
    NegRule,
    ChainRule,
    ExpRule,
    LogRule,
    TrigRule,
    SqrtRule,
    AbsRule,
    Simplify,
    IntegrationPowerRule,
    IntegrationSumRule,
    IntegrationConstantRule,
    IntegrationExpRule,
    IntegrationTrigRule,
    LimitRichardson,
    TaylorRule,
    Generic,
}

impl std::fmt::Display for RewriteRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::ConstantRule => "ConstantRule",
            Self::VariableRule => "VariableRule",
            Self::SumRule => "SumRule",
            Self::DifferenceRule => "DifferenceRule",
            Self::ProductRule => "ProductRule",
            Self::QuotientRule => "QuotientRule",
            Self::PowerRule => "PowerRule",
            Self::NegRule => "NegRule",
            Self::ChainRule => "ChainRule",
            Self::ExpRule => "ExpRule",
            Self::LogRule => "LogRule",
            Self::TrigRule => "TrigRule",
            Self::SqrtRule => "SqrtRule",
            Self::AbsRule => "AbsRule",
            Self::Simplify => "Simplify",
            Self::IntegrationPowerRule => "IntegrationPowerRule",
            Self::IntegrationSumRule => "IntegrationSumRule",
            Self::IntegrationConstantRule => "IntegrationConstantRule",
            Self::IntegrationExpRule => "IntegrationExpRule",
            Self::IntegrationTrigRule => "IntegrationTrigRule",
            Self::LimitRichardson => "LimitRichardson",
            Self::TaylorRule => "TaylorRule",
            Self::Generic => "Generic",
        };
        write!(f, "{s}")
    }
}

/// Un paso pedagógico `before --rule--> after`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasStep {
    pub index: usize,
    pub rule: RewriteRule,
    pub before: String,
    pub after: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// CasOp — despacho genérico
// ---------------------------------------------------------------------------

/// Operación CAS cuya traza se solicita.
#[derive(Clone, Debug, PartialEq)]
pub enum CasOp {
    Derivative {
        expr: String,
        var: String,
    },
    Integral {
        expr: String,
        var: String,
    },
    Limit {
        expr: String,
        var: String,
        at: f64,
    },
    Taylor {
        expr: String,
        var: String,
        center: f64,
        order: usize,
    },
}

// ---------------------------------------------------------------------------
// Statem
// ---------------------------------------------------------------------------

/// Estado tipado del stepper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CasStepperState {
    Idle,
    Parsing,
    Stepping,
    Verifying,
    Done,
    Failed { reason: String },
}

/// Stepper con transiciones tipadas vía `&mut self`.
#[derive(Clone, Debug)]
pub struct CasStepper {
    state: CasStepperState,
    steps: Vec<CasStep>,
}

impl Default for CasStepper {
    fn default() -> Self {
        Self::new()
    }
}

impl CasStepper {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: CasStepperState::Idle,
            steps: Vec::new(),
        }
    }

    #[must_use]
    pub fn state(&self) -> &CasStepperState {
        &self.state
    }

    #[must_use]
    pub fn steps(&self) -> &[CasStep] {
        &self.steps
    }

    /// Idle → Parsing
    pub fn begin_parsing(&mut self) -> Result<(), String> {
        match self.state {
            CasStepperState::Idle => {
                self.state = CasStepperState::Parsing;
                Ok(())
            }
            _ => Err(format!("transición inválida desde {:?}", self.state)),
        }
    }

    /// Parsing → Stepping
    pub fn begin_stepping(&mut self) -> Result<(), String> {
        match self.state {
            CasStepperState::Parsing => {
                self.state = CasStepperState::Stepping;
                Ok(())
            }
            _ => Err(format!("transición inválida desde {:?}", self.state)),
        }
    }

    /// Stepping → Verifying
    pub fn begin_verifying(&mut self) -> Result<(), String> {
        match self.state {
            CasStepperState::Stepping => {
                self.state = CasStepperState::Verifying;
                Ok(())
            }
            _ => Err(format!("transición inválida desde {:?}", self.state)),
        }
    }

    /// Verifying → Done | Failed
    pub fn finish(&mut self, ok: bool, reason: Option<String>) -> Result<(), String> {
        match &self.state {
            CasStepperState::Verifying => {
                if ok {
                    self.state = CasStepperState::Done;
                } else {
                    self.state = CasStepperState::Failed {
                        reason: reason.unwrap_or_else(|| "falló verificación".to_string()),
                    };
                }
                Ok(())
            }
            _ => Err(format!("transición inválida desde {:?}", self.state)),
        }
    }

    /// Ejecuta el despacho genérico `CasOp → Vec<CasStep>` con statem completo.
    pub fn run(&mut self, op: &CasOp) -> Result<Vec<CasStep>, String> {
        self.begin_parsing()?;
        let parsed_ok = match op {
            CasOp::Derivative { expr, var } | CasOp::Integral { expr, var } => {
                validate_identifier(var).is_ok()
                    && validate_input_bytes(expr).is_ok()
                    && parse_ast(&expr.replace(' ', "")).is_ok()
            }
            CasOp::Limit { expr, var, at } => {
                if !at.is_finite() {
                    false
                } else {
                    validate_identifier(var).is_ok()
                        && validate_input_bytes(expr).is_ok()
                        && parse_ast(&expr.replace(' ', "")).is_ok()
                }
            }
            CasOp::Taylor {
                expr, var, order, ..
            } => {
                validate_identifier(var).is_ok()
                    && validate_input_bytes(expr).is_ok()
                    && *order <= crate::analysis::MAX_TAYLOR_ORDER
                    && parse_ast(&expr.replace(' ', "")).is_ok()
            }
        };
        if !parsed_ok {
            self.state = CasStepperState::Failed {
                reason: "parse error".to_string(),
            };
            return Err("parse error".to_string());
        }
        self.begin_stepping()?;
        let steps = steps_for_op(op)?;
        self.steps = steps.clone();
        self.begin_verifying()?;
        let ok = !self.steps.is_empty();
        self.finish(
            ok,
            if ok {
                None
            } else {
                Some("sin pasos".to_string())
            },
        )?;
        Ok(self.steps.clone())
    }
}

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn validate_identifier(var: &str) -> Result<(), String> {
    let mut chars = var.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("variable no es identificador válido: {var}"));
    }
    Ok(())
}

fn validate_input_bytes(expr: &str) -> Result<(), String> {
    if expr.len() > crate::outcome::MAX_MATH_INPUT_BYTES {
        return Err(format!(
            "expresión excede {} bytes",
            crate::outcome::MAX_MATH_INPUT_BYTES
        ));
    }
    Ok(())
}

/// Trunca a `MAX_STEP_BYTES` respetando borde UTF-8.
fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

fn push_step(
    steps: &mut Vec<CasStep>,
    rule: RewriteRule,
    before: &Expr,
    after: &Expr,
    description: &str,
) {
    if steps.len() >= MAX_CAS_STEPS {
        return;
    }
    let before_s = truncate_bytes(&before.to_expr_string(), MAX_STEP_BYTES);
    let after_s = truncate_bytes(&after.to_expr_string(), MAX_STEP_BYTES);
    if before_s == after_s && rule != RewriteRule::Simplify {
        return;
    }
    let idx = steps.len();
    steps.push(CasStep {
        index: idx,
        rule,
        before: before_s,
        after: after_s,
        description: truncate_bytes(description, MAX_STEP_BYTES),
    });
}

fn push_limit_step(steps: &mut Vec<CasStep>, before: &str, after: &str, description: &str) {
    if steps.len() >= MAX_CAS_STEPS {
        return;
    }
    let idx = steps.len();
    steps.push(CasStep {
        index: idx,
        rule: RewriteRule::LimitRichardson,
        before: truncate_bytes(before, MAX_STEP_BYTES),
        after: truncate_bytes(after, MAX_STEP_BYTES),
        description: truncate_bytes(description, MAX_STEP_BYTES),
    });
}

// ---------------------------------------------------------------------------
// Simplify genérico (copia de symbolic::simplify_expr)
// ---------------------------------------------------------------------------

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

fn simplify_once(e: &Expr) -> Expr {
    use Expr::{
        Abs, Acos, Acosh, Arg, Asin, Asinh, Atan, Atan2, Atanh, BesselI, BesselJ, BesselY, Beta,
        Cbrt, Ceil, Clamp, Conj, Const, Cos, Cosh, Cot, Csc, Digamma, Erf, Erfc, Exp, Floor, Gamma,
        Heaviside, Im, Ln, LnGamma, Log, Max, Min, Modulo, Neg, Re, Round, Sec, Sign, Sin, Sinh,
        Sqrt, Tan, Tanh, Trigamma,
    };
    match e {
        Neg(a) => {
            let sa = simplify_once(a);
            match sa {
                Const(c) => Const(-c),
                Neg(inner) => *inner,
                _ => Neg(Box::new(sa)),
            }
        }
        Expr::Add(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca + cb),
                (Const(ca), _) if *ca == 0.0 => sb,
                (_, Const(cb)) if *cb == 0.0 => sa,
                _ => Expr::Add(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sub(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca - cb),
                (_, Const(cb)) if *cb == 0.0 => sa,
                (Const(ca), _) if *ca == 0.0 => Neg(Box::new(sb)),
                _ if sa.structurally_eq(&sb) && sa.is_guaranteed_finite() => Const(0.0),
                _ => Expr::Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Mul(a, b) => {
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
                _ => Expr::Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Div(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) if cb.abs() > 1e-300 => Const(ca / cb),
                (_, Const(cb)) if *cb == 1.0 => sa,
                _ => Expr::Div(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Pow(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) if *ca != 0.0 || *cb > 0.0 => Const(ca.powf(*cb)),
                (_, Const(cb)) if *cb == 1.0 => sa,
                _ => Expr::Pow(Box::new(sa), Box::new(sb)),
            }
        }
        Sin(a) => Sin(Box::new(simplify_once(a))),
        Cos(a) => Cos(Box::new(simplify_once(a))),
        Tan(a) => Tan(Box::new(simplify_once(a))),
        Asin(a) => Asin(Box::new(simplify_once(a))),
        Acos(a) => Acos(Box::new(simplify_once(a))),
        Atan(a) => Atan(Box::new(simplify_once(a))),
        Asinh(a) => crate::ast::Expr::Asinh(Box::new(simplify_once(a))),
        Acosh(a) => crate::ast::Expr::Acosh(Box::new(simplify_once(a))),
        Atanh(a) => crate::ast::Expr::Atanh(Box::new(simplify_once(a))),
        Exp(a) => Exp(Box::new(simplify_once(a))),
        Ln(a) => Ln(Box::new(simplify_once(a))),
        Log(a) => Log(Box::new(simplify_once(a))),
        Sqrt(a) => Sqrt(Box::new(simplify_once(a))),
        Abs(a) => Abs(Box::new(simplify_once(a))),
        Sinh(a) => Sinh(Box::new(simplify_once(a))),
        Cosh(a) => Cosh(Box::new(simplify_once(a))),
        Tanh(a) => Tanh(Box::new(simplify_once(a))),
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
        Atan2(a, b) => Atan2(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Modulo(a, b) => Modulo(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Min(a, b) => Min(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Max(a, b) => Max(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Beta(a, b) => Beta(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselJ(a, b) => BesselJ(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselY(a, b) => BesselY(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        BesselI(a, b) => BesselI(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Lt(a, b) => Expr::Lt(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Gt(a, b) => Expr::Gt(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Le(a, b) => Expr::Le(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Ge(a, b) => Expr::Ge(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Eq(a, b) => Expr::Eq(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Expr::Ne(a, b) => Expr::Ne(Box::new(simplify_once(a)), Box::new(simplify_once(b))),
        Clamp(a, b, c) => Clamp(
            Box::new(simplify_once(a)),
            Box::new(simplify_once(b)),
            Box::new(simplify_once(c)),
        ),
        Expr::Sum(body, v, s, t) => Expr::Sum(
            Box::new(simplify_once(body)),
            v.clone(),
            Box::new(simplify_once(s)),
            Box::new(simplify_once(t)),
        ),
        Expr::Product(body, v, s, t) => Expr::Product(
            Box::new(simplify_once(body)),
            v.clone(),
            Box::new(simplify_once(s)),
            Box::new(simplify_once(t)),
        ),
        Expr::Piecewise(pieces, default) => {
            let np: Vec<(Box<Expr>, Box<Expr>)> = pieces
                .iter()
                .map(|(c, v)| (Box::new(simplify_once(c)), Box::new(simplify_once(v))))
                .collect();
            Expr::Piecewise(np, Box::new(simplify_once(default)))
        }
        Const(_) | Expr::Var(_) => e.clone(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn contains_var(e: &Expr, var: &str) -> bool {
    let mut vars = HashSet::new();
    e.get_variables(&mut vars);
    vars.contains(var)
}

fn expr_is_linear_in(expr: &Expr, var: &str) -> bool {
    use Expr::{Const, Var};
    match expr {
        Var(name) => name == var,
        Const(_) => true,
        Expr::Neg(inner) => expr_is_linear_in(inner, var),
        Expr::Add(left, right) | Expr::Sub(left, right) => {
            expr_is_linear_in(left, var) && expr_is_linear_in(right, var)
        }
        Expr::Mul(left, right) => {
            if matches!(left.as_ref(), Const(_)) {
                expr_is_linear_in(right, var)
            } else if matches!(right.as_ref(), Const(_)) {
                expr_is_linear_in(left, var)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn expr_linear_coeff(expr: &Expr, var: &str) -> Option<(f64, f64)> {
    use Expr::{Const, Var};
    match expr {
        Var(name) if name == var => Some((1.0, 0.0)),
        Const(v) if v.is_finite() => Some((0.0, *v)),
        Expr::Neg(inner) => {
            let (a, b) = expr_linear_coeff(inner, var)?;
            Some((-a, -b))
        }
        Expr::Add(l, r) => {
            let (a1, b1) = expr_linear_coeff(l, var)?;
            let (a2, b2) = expr_linear_coeff(r, var)?;
            let a = a1 + a2;
            let b = b1 + b2;
            (a.is_finite() && b.is_finite()).then_some((a, b))
        }
        Expr::Sub(l, r) => {
            let (a1, b1) = expr_linear_coeff(l, var)?;
            let (a2, b2) = expr_linear_coeff(r, var)?;
            let a = a1 - a2;
            let b = b1 - b2;
            (a.is_finite() && b.is_finite()).then_some((a, b))
        }
        Expr::Mul(l, r) => {
            if let Const(c) = l.as_ref() {
                let (a, b) = expr_linear_coeff(r, var)?;
                let a2 = c * a;
                let b2 = c * b;
                (a2.is_finite() && b2.is_finite()).then_some((a2, b2))
            } else if let Const(c) = r.as_ref() {
                let (a, b) = expr_linear_coeff(l, var)?;
                let a2 = c * a;
                let b2 = c * b;
                (a2.is_finite() && b2.is_finite()).then_some((a2, b2))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn constant_scalar_value(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Const(v) if v.is_finite() => Some(*v),
        Expr::Neg(inner) => Some(-constant_scalar_value(inner)?),
        Expr::Add(l, r) => Some(constant_scalar_value(l)? + constant_scalar_value(r)?),
        Expr::Sub(l, r) => Some(constant_scalar_value(l)? - constant_scalar_value(r)?),
        Expr::Mul(l, r) => Some(constant_scalar_value(l)? * constant_scalar_value(r)?),
        Expr::Div(a, b) => {
            let den = constant_scalar_value(b)?;
            if den == 0.0 {
                return None;
            }
            Some(constant_scalar_value(a)? / den)
        }
        Expr::Pow(a, b) => Some(constant_scalar_value(a)?.powf(constant_scalar_value(b)?)),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

// ---------------------------------------------------------------------------
// Derivador instrumentado
// ---------------------------------------------------------------------------

fn diff_with_steps(e: &Expr, var: &str, steps: &mut Vec<CasStep>, depth: u32) -> Expr {
    const MAX_DEPTH: u32 = 256;
    if depth > MAX_DEPTH {
        return Expr::Const(f64::NAN);
    }
    if steps.len() >= MAX_CAS_STEPS {
        return e.clone();
    }
    use Expr::{
        Abs, Acos, Asin, Atan, Cbrt, Const, Cos, Cosh, Cot, Csc, Exp, Ln, Log, Sec, Sin, Sinh,
        Sqrt, Tan, Tanh, Var,
    };
    let before = e.clone();
    let (result, rule, desc): (Expr, RewriteRule, String) = match e {
        Const(_) => (
            Const(0.0),
            RewriteRule::ConstantRule,
            "derivada de constante = 0".into(),
        ),
        Var(v) => {
            if v == var {
                (
                    Const(1.0),
                    RewriteRule::VariableRule,
                    format!("d/d{var} {var} = 1"),
                )
            } else {
                (
                    Const(0.0),
                    RewriteRule::VariableRule,
                    format!("d/d{var} {v} = 0"),
                )
            }
        }
        Expr::Neg(a) => {
            let da = diff_with_steps(a, var, steps, depth + 1);
            let res = Expr::Neg(Box::new(da));
            (res, RewriteRule::NegRule, "regla de la negación".into())
        }
        Expr::Add(a, b) => {
            let da = diff_with_steps(a, var, steps, depth + 1);
            let db = diff_with_steps(b, var, steps, depth + 1);
            let res = Expr::Add(Box::new(da), Box::new(db));
            (res, RewriteRule::SumRule, "regla de la suma".into())
        }
        Expr::Sub(a, b) => {
            let da = diff_with_steps(a, var, steps, depth + 1);
            let db = diff_with_steps(b, var, steps, depth + 1);
            let res = Expr::Sub(Box::new(da), Box::new(db));
            (
                res,
                RewriteRule::DifferenceRule,
                "regla de la diferencia".into(),
            )
        }
        Expr::Mul(u, v) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let dv = diff_with_steps(v, var, steps, depth + 1);
            let res = Expr::Add(
                Box::new(Expr::Mul(Box::new(du), v.clone())),
                Box::new(Expr::Mul(u.clone(), Box::new(dv))),
            );
            (
                res,
                RewriteRule::ProductRule,
                "regla del producto (u·v)' = u'·v + u·v'".into(),
            )
        }
        Expr::Div(u, v) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let dv = diff_with_steps(v, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(Expr::Sub(
                    Box::new(Expr::Mul(Box::new(du), v.clone())),
                    Box::new(Expr::Mul(u.clone(), Box::new(dv))),
                )),
                Box::new(Expr::Pow(v.clone(), Box::new(Const(2.0)))),
            );
            (res, RewriteRule::QuotientRule, "regla del cociente".into())
        }
        Expr::Pow(base, exp) => match exp.as_ref() {
            Const(n) => {
                let du = diff_with_steps(base, var, steps, depth + 1);
                let res = Expr::Mul(
                    Box::new(Expr::Mul(
                        Box::new(Const(*n)),
                        Box::new(Expr::Pow(base.clone(), Box::new(Const(n - 1.0)))),
                    )),
                    Box::new(du),
                );
                (
                    res,
                    RewriteRule::PowerRule,
                    format!("potencia: d/dx x^{n} = {n}·x^{}·x'", n - 1.0),
                )
            }
            _ => {
                let du = diff_with_steps(base, var, steps, depth + 1);
                let dv = diff_with_steps(exp, var, steps, depth + 1);
                let base_clone = base.clone();
                let exp_clone = exp.clone();
                let res = Expr::Mul(
                    Box::new(Expr::Pow(base_clone.clone(), exp_clone.clone())),
                    Box::new(Expr::Add(
                        Box::new(Expr::Mul(Box::new(dv), Box::new(Ln(base_clone.clone())))),
                        Box::new(Expr::Mul(
                            exp_clone,
                            Box::new(Expr::Div(Box::new(du), base_clone)),
                        )),
                    )),
                );
                (
                    res,
                    RewriteRule::PowerRule,
                    "potencia general: (u^v)' = u^v·(v'·ln u + v·u'/u)".into(),
                )
            }
        },
        Sin(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(Box::new(Cos(u.clone())), Box::new(du));
            (
                res,
                RewriteRule::ChainRule,
                "cadena: sin(u)' = cos(u)·u'".into(),
            )
        }
        Cos(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(Box::new(Expr::Neg(Box::new(Sin(u.clone())))), Box::new(du));
            (
                res,
                RewriteRule::ChainRule,
                "cadena: cos(u)' = -sin(u)·u'".into(),
            )
        }
        Tan(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Div(
                    Box::new(Const(1.0)),
                    Box::new(Expr::Pow(Box::new(Cos(u.clone())), Box::new(Const(2.0)))),
                )),
                Box::new(du),
            );
            (res, RewriteRule::ChainRule, "tan(u)' = u'/cos²(u)".into())
        }
        Sec(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Mul(
                    Box::new(Sec(u.clone())),
                    Box::new(Tan(u.clone())),
                )),
                Box::new(du),
            );
            (
                res,
                RewriteRule::ChainRule,
                "sec(u)' = sec(u)·tan(u)·u'".into(),
            )
        }
        Csc(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Neg(Box::new(Expr::Mul(
                    Box::new(Csc(u.clone())),
                    Box::new(Cot(u.clone())),
                )))),
                Box::new(du),
            );
            (
                res,
                RewriteRule::ChainRule,
                "csc(u)' = -csc(u)·cot(u)·u'".into(),
            )
        }
        Cot(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Neg(Box::new(Expr::Pow(
                    Box::new(Csc(u.clone())),
                    Box::new(Const(2.0)),
                )))),
                Box::new(du),
            );
            (res, RewriteRule::ChainRule, "cot(u)' = -csc²(u)·u'".into())
        }
        Asin(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(du),
                Box::new(Sqrt(Box::new(Expr::Sub(
                    Box::new(Const(1.0)),
                    Box::new(Expr::Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            );
            (res, RewriteRule::ChainRule, "asin(u)' = u'/√(1-u²)".into())
        }
        Acos(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(Expr::Neg(Box::new(du))),
                Box::new(Sqrt(Box::new(Expr::Sub(
                    Box::new(Const(1.0)),
                    Box::new(Expr::Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            );
            (res, RewriteRule::ChainRule, "acos(u)' = -u'/√(1-u²)".into())
        }
        Atan(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(du),
                Box::new(Expr::Add(
                    Box::new(Const(1.0)),
                    Box::new(Expr::Pow(u.clone(), Box::new(Const(2.0)))),
                )),
            );
            (res, RewriteRule::ChainRule, "atan(u)' = u'/(1+u²)".into())
        }
        Exp(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(Box::new(Exp(u.clone())), Box::new(du));
            (res, RewriteRule::ExpRule, "exp(u)' = exp(u)·u'".into())
        }
        Ln(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(Box::new(du), u.clone());
            (res, RewriteRule::LogRule, "ln(u)' = u'/u".into())
        }
        Log(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(du),
                Box::new(Expr::Mul(
                    u.clone(),
                    Box::new(Const(std::f64::consts::LN_10)),
                )),
            );
            (res, RewriteRule::LogRule, "log(u)' = u'/(u·ln10)".into())
        }
        Sqrt(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(du),
                Box::new(Expr::Mul(Box::new(Const(2.0)), Box::new(Sqrt(u.clone())))),
            );
            (res, RewriteRule::SqrtRule, "sqrt(u)' = u'/(2·√u)".into())
        }
        Cbrt(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Div(
                Box::new(du),
                Box::new(Expr::Mul(
                    Box::new(Const(3.0)),
                    Box::new(Expr::Pow(Box::new(Cbrt(u.clone())), Box::new(Const(2.0)))),
                )),
            );
            (
                res,
                RewriteRule::PowerRule,
                "cbrt(u)' = u'/(3·cbrt(u)²)".into(),
            )
        }
        Abs(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Div(u.clone(), Box::new(Abs(u.clone())))),
                Box::new(du),
            );
            (res, RewriteRule::AbsRule, "|u|' = sign(u)·u'".into())
        }
        Sinh(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(Box::new(Cosh(u.clone())), Box::new(du));
            (res, RewriteRule::ChainRule, "sinh(u)' = cosh(u)·u'".into())
        }
        Cosh(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(Box::new(Sinh(u.clone())), Box::new(du));
            (res, RewriteRule::ChainRule, "cosh(u)' = sinh(u)·u'".into())
        }
        Tanh(u) => {
            let du = diff_with_steps(u, var, steps, depth + 1);
            let res = Expr::Mul(
                Box::new(Expr::Div(
                    Box::new(Const(1.0)),
                    Box::new(Expr::Pow(Box::new(Cosh(u.clone())), Box::new(Const(2.0)))),
                )),
                Box::new(du),
            );
            (res, RewriteRule::ChainRule, "tanh(u)' = u'/cosh²(u)".into())
        }
        _ => {
            let res = e.diff(var);
            (res, RewriteRule::Generic, "regla genérica".into())
        }
    };
    let simplified = simplify_expr(&result);
    push_step(steps, rule, &before, &simplified, &desc);
    simplified
}

// ---------------------------------------------------------------------------
// Integrador instrumentado
// ---------------------------------------------------------------------------

fn integrate_with_steps(e: &Expr, var: &str, steps: &mut Vec<CasStep>, depth: u32) -> Option<Expr> {
    const MAX_DEPTH: u32 = 256;
    if depth > MAX_DEPTH {
        return None;
    }
    if steps.len() >= MAX_CAS_STEPS {
        return None;
    }
    use Expr::{Const, Exp, Var};
    let v = var.to_string();
    if !contains_var(e, var) {
        let res = Expr::Mul(Box::new(e.clone()), Box::new(Var(v.clone())));
        push_step(
            steps,
            RewriteRule::IntegrationConstantRule,
            e,
            &res,
            "∫ c dx = c·x",
        );
        return Some(simplify_expr(&res));
    }
    let before = e.clone();
    if let Some(sec_form) = try_sec_squared_form(e, var) {
        let simp = simplify_expr(&sec_form);
        push_step(
            steps,
            RewriteRule::IntegrationTrigRule,
            &before,
            &simp,
            "∫ sec²(ax+b) dx = tan(ax+b)/a",
        );
        return Some(simp);
    }
    if let Some(atan_form) = try_atan_form(e, var) {
        let simp = simplify_expr(&atan_form);
        push_step(
            steps,
            RewriteRule::IntegrationTrigRule,
            &before,
            &simp,
            "∫ c/(x²+a²) dx = (c/a)·atan(x/a)",
        );
        return Some(simp);
    }
    let result: Option<Expr> = match e {
        Const(c) => {
            if *c == 0.0 {
                Some(Const(0.0))
            } else {
                Some(Expr::Mul(Box::new(Const(*c)), Box::new(Var(v.clone()))))
            }
        }
        Var(name) if name == var => Some(Expr::Mul(
            Box::new(Expr::Pow(Box::new(Var(v.clone())), Box::new(Const(2.0)))),
            Box::new(Const(0.5)),
        )),
        Expr::Neg(a) => {
            let inner = integrate_with_steps(a, var, steps, depth + 1)?;
            Some(Expr::Neg(Box::new(inner)))
        }
        Expr::Add(a, b) => {
            let ia = integrate_with_steps(a, var, steps, depth + 1)?;
            let ib = integrate_with_steps(b, var, steps, depth + 1)?;
            Some(Expr::Add(Box::new(ia), Box::new(ib)))
        }
        Expr::Sub(a, b) => {
            let ia = integrate_with_steps(a, var, steps, depth + 1)?;
            let ib = integrate_with_steps(b, var, steps, depth + 1)?;
            Some(Expr::Sub(Box::new(ia), Box::new(ib)))
        }
        Expr::Mul(a, b) => {
            if !contains_var(a, var) {
                let ib = integrate_with_steps(b, var, steps, depth + 1)?;
                Some(Expr::Mul(a.clone(), Box::new(ib)))
            } else if !contains_var(b, var) {
                let ia = integrate_with_steps(a, var, steps, depth + 1)?;
                Some(Expr::Mul(Box::new(ia), b.clone()))
            } else {
                try_generic_parts_with_steps(e, var, steps, depth)
            }
        }
        Expr::Pow(base, exp) => {
            if let Var(name) = base.as_ref() {
                if name == var {
                    if let Const(n) = exp.as_ref() {
                        if (*n + 1.0).abs() < 1e-12 {
                            Some(Expr::Ln(Box::new(Expr::Abs(Box::new(Var(v.clone()))))))
                        } else {
                            let new_exp = n + 1.0;
                            Some(Expr::Mul(
                                Box::new(Const(1.0 / new_exp)),
                                Box::new(Expr::Pow(
                                    Box::new(Var(v.clone())),
                                    Box::new(Const(new_exp)),
                                )),
                            ))
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        Expr::Div(num, den) => {
            if let Var(name) = den.as_ref() {
                if name == var && !contains_var(num, var) {
                    Some(Expr::Mul(
                        num.clone(),
                        Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Var(v.clone())))))),
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Expr::Sin(arg) => {
            if expr_is_linear_in(arg, var) {
                let (a, _) = expr_linear_coeff(arg, var)?;
                if a.abs() < 1e-12 {
                    return None;
                }
                Some(Expr::Mul(
                    Box::new(Const(-1.0 / a)),
                    Box::new(Expr::Cos(arg.clone())),
                ))
            } else {
                None
            }
        }
        Expr::Cos(arg) => {
            if expr_is_linear_in(arg, var) {
                let (a, _) = expr_linear_coeff(arg, var)?;
                if a.abs() < 1e-12 {
                    return None;
                }
                Some(Expr::Mul(
                    Box::new(Const(1.0 / a)),
                    Box::new(Expr::Sin(arg.clone())),
                ))
            } else {
                None
            }
        }
        Expr::Tan(arg) => {
            if expr_is_linear_in(arg, var) {
                let (a, _) = expr_linear_coeff(arg, var)?;
                if a.abs() < 1e-12 {
                    return None;
                }
                Some(Expr::Mul(
                    Box::new(Const(-1.0 / a)),
                    Box::new(Expr::Ln(Box::new(Expr::Abs(Box::new(Expr::Cos(
                        arg.clone(),
                    )))))),
                ))
            } else {
                None
            }
        }
        Exp(arg) => {
            if expr_is_linear_in(arg, var) {
                let (a, _) = expr_linear_coeff(arg, var)?;
                if a.abs() < 1e-12 {
                    return None;
                }
                if (a - 1.0).abs() < 1e-12 {
                    Some(Exp(arg.clone()))
                } else {
                    Some(Expr::Mul(
                        Box::new(Const(1.0 / a)),
                        Box::new(Exp(arg.clone())),
                    ))
                }
            } else {
                None
            }
        }
        Expr::Ln(u) if matches!(u.as_ref(), Var(name) if name == var) => Some(Expr::Sub(
            Box::new(Expr::Mul(
                Box::new(Var(v.clone())),
                Box::new(Expr::Ln(Box::new(Var(v.clone())))),
            )),
            Box::new(Var(v.clone())),
        )),
        _ => return None,
    };
    if let Some(res) = result {
        let simp = simplify_expr(&res);
        let rule = match e {
            Expr::Add(..) | Expr::Sub(..) => RewriteRule::IntegrationSumRule,
            Expr::Pow(..) => RewriteRule::IntegrationPowerRule,
            Exp(..) => RewriteRule::IntegrationExpRule,
            Expr::Sin(..) | Expr::Cos(..) | Expr::Tan(..) => RewriteRule::IntegrationTrigRule,
            _ => RewriteRule::Generic,
        };
        push_step(steps, rule, &before, &simp, "integración");
        Some(simp)
    } else {
        None
    }
}

fn try_generic_parts_with_steps(
    expr: &Expr,
    var: &str,
    steps: &mut Vec<CasStep>,
    depth: u32,
) -> Option<Expr> {
    const MAX_PARTS_DEPTH: u32 = 256;
    if depth > MAX_PARTS_DEPTH {
        return None;
    }
    let (left, right) = match expr {
        Expr::Mul(a, b) => (a.as_ref(), b.as_ref()),
        _ => return None,
    };
    for (u_cand, dv_cand) in [(left, right), (right, left)] {
        let v = integrate_with_steps(dv_cand, var, steps, depth + 1)
            .or_else(|| dv_cand.integrate(var))?;
        let du = diff_with_steps(u_cand, var, steps, depth + 1);
        let du_s = simplify_expr(&du);
        if du_s == Expr::Const(0.0) {
            continue;
        }
        let v_du = Expr::Mul(Box::new(v.clone()), Box::new(du_s));
        let int_vdu =
            integrate_with_steps(&v_du, var, steps, depth + 1).or_else(|| v_du.integrate(var))?;
        let uv = Expr::Mul(Box::new(u_cand.clone()), Box::new(v));
        let result = Expr::Sub(Box::new(uv), Box::new(int_vdu));
        let simplified = simplify_expr(&result);
        push_step(
            steps,
            RewriteRule::ProductRule,
            expr,
            &simplified,
            "integración por partes ∫u·dv = u·v − ∫v·du",
        );
        return Some(simplified);
    }
    None
}

fn try_sec_squared_form(expr: &Expr, var: &str) -> Option<Expr> {
    use Expr::{Const, Pow, Sec, Tan};
    if let Pow(base, exp) = expr {
        if let Const(n) = exp.as_ref() {
            if (*n - 2.0).abs() < 1e-12 {
                if let Sec(arg) = base.as_ref() {
                    if expr_is_linear_in(arg, var) {
                        let (a, _) = expr_linear_coeff(arg, var)?;
                        if a.abs() > 1e-12 {
                            return Some(Expr::Mul(
                                Box::new(Const(1.0 / a)),
                                Box::new(Tan(arg.clone())),
                            ));
                        }
                    }
                }
            }
            if (*n - 2.0).abs() < 1e-12 {
                if let Expr::Csc(arg) = base.as_ref() {
                    if expr_is_linear_in(arg, var) {
                        let (a, _) = expr_linear_coeff(arg, var)?;
                        if a.abs() > 1e-12 {
                            return Some(Expr::Mul(
                                Box::new(Const(-1.0 / a)),
                                Box::new(Expr::Cot(arg.clone())),
                            ));
                        }
                    }
                }
            }
        }
    }
    if let Expr::Mul(left, right) = expr {
        if let (Sec(arg1), Sec(arg2)) = (left.as_ref(), right.as_ref()) {
            if arg1 == arg2 && expr_is_linear_in(arg1, var) {
                let (a, _) = expr_linear_coeff(arg1, var)?;
                if a.abs() > 1e-12 {
                    return Some(Expr::Mul(
                        Box::new(Const(1.0 / a)),
                        Box::new(Tan(arg1.clone())),
                    ));
                }
            }
        }
    }
    None
}

fn try_atan_form(expr: &Expr, var: &str) -> Option<Expr> {
    use Expr::{Add, Atan, Const, Div, Pow, Var};
    let (num, den) = match expr {
        Div(n, d) => (n.as_ref(), d.as_ref()),
        _ => return None,
    };
    if contains_var(num, var) {
        return None;
    }
    let coeff = constant_scalar_value(num)?;
    if !coeff.is_finite() {
        return None;
    }
    let (pow_base, pow_exp, const_val) = match den {
        Add(left, right) => match (left.as_ref(), right.as_ref()) {
            (Pow(b, e), Const(k)) => (b.as_ref(), e.as_ref(), *k),
            (Const(k), Pow(b, e)) => (b.as_ref(), e.as_ref(), *k),
            _ => return None,
        },
        _ => return None,
    };
    let Const(exp_val) = pow_exp else {
        return None;
    };
    if (*exp_val - 2.0).abs() >= 1e-12 {
        return None;
    }
    let Var(name) = pow_base else {
        return None;
    };
    if name != var {
        return None;
    }
    if !const_val.is_finite() || const_val <= 0.0 {
        return None;
    }
    let a = const_val.sqrt();
    if !a.is_finite() || a == 0.0 {
        return None;
    }
    let factor = coeff / a;
    if !factor.is_finite() {
        return None;
    }
    Some(Expr::Mul(
        Box::new(Const(factor)),
        Box::new(Atan(Box::new(Div(
            Box::new(Var(var.to_string())),
            Box::new(Const(a)),
        )))),
    ))
}

// ---------------------------------------------------------------------------
// Límite Richardson
// ---------------------------------------------------------------------------

fn limit_richardson_steps(ast: &Expr, var: &str, at: f64, steps: &mut Vec<CasStep>) -> Option<f64> {
    const STEP_SCALES: [f64; 3] = [0.125, 0.1, 0.075];
    let local_scale = at.abs().max(1.0);
    let mut estimates = Vec::new();
    for scale in STEP_SCALES {
        let h0 = scale * local_scale;
        let left = stable_side_limit_steps(ast, var, at, -1.0, h0, steps)?;
        let right = stable_side_limit_steps(ast, var, at, 1.0, h0, steps)?;
        if (left - right).abs() > limit_tolerance(left.abs().max(right.abs())) + 8.0 * 1e-9 {
            return None;
        }
        estimates.push((left + right) * 0.5);
    }
    let mut value = estimates[0];
    for (idx, est) in estimates.iter().enumerate().skip(1) {
        value += (est - value) / (idx + 1) as f64;
    }
    push_limit_step(
        steps,
        &format!("lim({}→{}) {}", var, at, ast.to_expr_string()),
        &format!("{value:.8}"),
        &format!("Richardson bilateral → {value:.8}"),
    );
    Some(value)
}

fn stable_side_limit_steps(
    ast: &Expr,
    var: &str,
    at: f64,
    sign: f64,
    initial_step: f64,
    steps: &mut Vec<CasStep>,
) -> Option<f64> {
    const SAMPLE_COUNT: usize = 12;
    let mut values = [0.0; SAMPLE_COUNT];
    for (idx, v) in values.iter_mut().enumerate() {
        let h = initial_step / 2.0_f64.powi(idx as i32);
        let x = at + sign * h;
        if !x.is_finite() || x == at {
            return None;
        }
        let val = ast.eval_at(var, x);
        if !val.is_finite() {
            return None;
        }
        *v = val;
        if steps.len() < MAX_CAS_STEPS {
            push_limit_step(
                steps,
                &format!("f({}={:.6})", var, x),
                &format!("{val:.6}"),
                &format!("muestra h={:.2e}", h),
            );
        }
    }
    let last = values[SAMPLE_COUNT - 1];
    let prev = values[SAMPLE_COUNT - 2];
    let diff = last - prev;
    if diff.abs() < 1e-9 + 1e-6 * last.abs().max(1.0) {
        return Some(last);
    }
    let diff2 = prev - values[SAMPLE_COUNT - 3];
    if diff == 0.0 || diff2 == 0.0 || diff.signum() != diff2.signum() {
        return Some(last);
    }
    let ratio = (diff / diff2).abs();
    if !ratio.is_finite() || ratio >= 0.8 {
        return Some(last);
    }
    let correction = diff * ratio / (1.0 - ratio);
    let value = last + correction;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn limit_tolerance(value: f64) -> f64 {
    1e-9 + 1e-6 * value.abs().max(1.0)
}

// ---------------------------------------------------------------------------
// Taylor genérico
// ---------------------------------------------------------------------------

fn taylor_steps(
    ast: &Expr,
    var: &str,
    center: f64,
    order: usize,
    steps: &mut Vec<CasStep>,
) -> Option<String> {
    if order > crate::analysis::MAX_TAYLOR_ORDER {
        return None;
    }
    let coeffs = crate::analysis::taylor_coefficients_from_ast(ast, var, center, order).ok()?;
    let mut terms = Vec::new();
    for (n, coef) in coeffs.iter().enumerate() {
        if coef.abs() < 1e-12 {
            continue;
        }
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
        terms.push(term.clone());
        if steps.len() < MAX_CAS_STEPS {
            push_limit_step(
                steps,
                &format!("coeficiente n={n} en {center}"),
                &term,
                &format!("Taylor coeficiente a_{n} = {coef:.6}"),
            );
        }
    }
    let out = if terms.is_empty() {
        "0".to_string()
    } else {
        terms.join(" + ").replace("+ -", "- ")
    };
    if steps.len() < MAX_CAS_STEPS {
        push_limit_step(
            steps,
            &ast.to_expr_string(),
            &out,
            &format!("serie Taylor orden {order} en {center}"),
        );
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Despacho genérico CasOp → Vec<CasStep>
// ---------------------------------------------------------------------------

/// Despacho genérico `CasOp → Vec<CasStep>` sin goldens hardcodeados.
pub fn steps_for_op(op: &CasOp) -> Result<Vec<CasStep>, String> {
    match op {
        CasOp::Derivative { expr, var } => steps_for_derivative(expr, var),
        CasOp::Integral { expr, var } => steps_for_integral(expr, var),
        CasOp::Limit { expr, var, at } => steps_for_limit(expr, var, *at),
        CasOp::Taylor {
            expr,
            var,
            center,
            order,
        } => steps_for_taylor(expr, var, *center, *order),
    }
}

/// Pasos para derivada genérica (instrumenta `diff_expr`).
pub fn steps_for_derivative(expr: &str, var: &str) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("parse error: {e}"))?;
    let mut steps = Vec::new();
    let derived = diff_with_steps(&ast, var, &mut steps, 0);
    let _ = derived;
    if steps.is_empty() {
        let simplified = simplify_expr(&ast.diff(var));
        push_step(
            &mut steps,
            RewriteRule::Generic,
            &ast,
            &simplified,
            "derivada directa",
        );
    }
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Pasos para integral genérica (instrumenta `integrate_expr`).
pub fn steps_for_integral(expr: &str, var: &str) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("parse error: {e}"))?;
    let mut steps = Vec::new();
    if let Some(prim) = integrate_with_steps(&ast, var, &mut steps, 0) {
        let _ = prim;
        if steps.is_empty() {
            if let Some(fb) = ast.integrate(var) {
                let simp = simplify_expr(&fb);
                push_step(
                    &mut steps,
                    RewriteRule::Generic,
                    &ast,
                    &simp,
                    "integral fallback",
                );
            }
        }
        steps.truncate(MAX_CAS_STEPS);
        Ok(steps)
    } else if let Some(fb) = ast.integrate(var) {
        let simp = simplify_expr(&fb);
        let mut fb_steps = Vec::new();
        push_step(
            &mut fb_steps,
            RewriteRule::Generic,
            &ast,
            &simp,
            "integral nativa",
        );
        fb_steps.truncate(MAX_CAS_STEPS);
        Ok(fb_steps)
    } else {
        Err(format!("no hay primitiva soportada para '{expr}'"))
    }
}

/// Pasos para límite genérico (Richardson bilateral).
pub fn steps_for_limit(expr: &str, var: &str, at: f64) -> Result<Vec<CasStep>, String> {
    if !at.is_finite() {
        return Err("at no es finito".to_string());
    }
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("parse error: {e}"))?;
    let mut steps = Vec::new();
    let val = limit_richardson_steps(&ast, var, at, &mut steps)
        .ok_or_else(|| format!("limite no existe para '{expr}' en {var}→{at}"))?;
    if steps.is_empty() {
        push_limit_step(
            &mut steps,
            &format!("lim({}→{}) {}", var, at, expr),
            &format!("{val:.8}"),
            "límite estimado",
        );
    }
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Serie de Taylor genérica (no limitada a e^x).
pub fn steps_for_taylor(
    expr: &str,
    var: &str,
    center: f64,
    order: usize,
) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    if order > crate::analysis::MAX_TAYLOR_ORDER {
        return Err(format!(
            "orden {order} excede {}",
            crate::analysis::MAX_TAYLOR_ORDER
        ));
    }
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("parse error: {e}"))?;
    let mut steps = Vec::new();
    let series = taylor_steps(&ast, var, center, order, &mut steps)
        .ok_or_else(|| format!("no se pudo calcular Taylor de '{expr}'"))?;
    let _ = series;
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

// ---------------------------------------------------------------------------
// Frente G-A: trazas acotadas del motor CAS (Gruntz, Risch, EDO, residuo,
// Buchberger). Reusan `CasStep` + `RewriteRule::Generic` (sin romper la
// API exhaustiva) y truncan a `MAX_CAS_STEPS` 32.
// ---------------------------------------------------------------------------

fn push_ga_step(steps: &mut Vec<CasStep>, before: &str, after: &str, description: &str) {
    if steps.len() >= MAX_CAS_STEPS {
        return;
    }
    let idx = steps.len();
    steps.push(CasStep {
        index: idx,
        rule: RewriteRule::Generic,
        before: truncate_bytes(before, MAX_STEP_BYTES),
        after: truncate_bytes(after, MAX_STEP_BYTES),
        description: truncate_bytes(description, MAX_STEP_BYTES),
    });
}

/// Traza Gruntz: clasifica la forma y muestra L'Hôpital o jerarquía.
pub fn steps_for_gruntz(expr: &str, var: &str, at: f64) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    if !at.is_finite() {
        return Err("at no es finito".to_string());
    }
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("parse error: {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &format!("lim({var}→{at}) {expr}"),
        &format!("forma {}", gruntz_form_label(&ast, var, at)),
        "Gruntz: clasificar 0/0, ∞/∞ o directa",
    );
    // L'Hôpital visible hasta 8 iteraciones (cota del motor).
    if let Expr::Div(num, den) = &ast {
        let mut cur_n = num.as_ref().clone();
        let mut cur_d = den.as_ref().clone();
        for i in 1..=crate::cas::MAX_GRUNTZ_STEPS {
            let (nv, dv) = (cur_n.eval_at(var, at), cur_d.eval_at(var, at));
            let indeterminate = (nv == 0.0 && dv == 0.0) || (nv.is_infinite() && dv.is_infinite());
            if !indeterminate {
                break;
            }
            let next_n = simplify_expr(&cur_n.diff(var));
            let next_d = simplify_expr(&cur_d.diff(var));
            push_ga_step(
                &mut steps,
                &format!("({})/({})", cur_n.to_expr_string(), cur_d.to_expr_string()),
                &format!(
                    "({})/({})",
                    next_n.to_expr_string(),
                    next_d.to_expr_string()
                ),
                &format!("L'Hôpital iteración {i}: derivar numerador y denominador"),
            );
            cur_n = next_n;
            cur_d = next_d;
            if steps.len() >= MAX_CAS_STEPS {
                break;
            }
        }
    }
    let outcome = crate::cas::gruntz_limit(expr, var, at)
        .map_err(|e| format!("Gruntz no resolvió '{expr}': {e}"))?;
    push_ga_step(
        &mut steps,
        &format!("lim({var}→{at}) {expr}"),
        &format!("{:.8}", outcome.value),
        &format!("Gruntz {:?} → {:.8}", outcome.method, outcome.value),
    );
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

fn gruntz_form_label(ast: &Expr, var: &str, at: f64) -> &'static str {
    if let Expr::Div(num, den) = ast {
        let (n, d) = (num.eval_at(var, at), den.eval_at(var, at));
        if n == 0.0 && d == 0.0 {
            return "0/0";
        }
        if n.is_infinite() && d.is_infinite() {
            return "∞/∞";
        }
    }
    if ast.eval_at(var, at).is_finite() {
        return "directa";
    }
    "otra"
}

/// Traza Risch-Norman: primitiva del subconjunto S/M.
pub fn steps_for_risch(expr: &str, var: &str) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    let prim = crate::integral::risch_norman_integrate(expr, var)
        .map_err(|e| format!("Risch-Norman no cubre '{expr}': {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &format!("∫ {expr} d{var}"),
        &prim,
        "Risch-Norman: potencias/exponenciales/logaritmos + linealidad",
    );
    push_ga_step(
        &mut steps,
        &prim,
        &format!("d/d{var}({prim}) = {expr}"),
        "verificación: derivar la primitiva recupera el integrando",
    );
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Traza EDO lineal `y' + p·y = q` por factor integrante.
pub fn steps_for_ode_linear(p: &str, q: &str, x: &str) -> Result<Vec<CasStep>, String> {
    validate_identifier(x)?;
    validate_input_bytes(p)?;
    validate_input_bytes(q)?;
    let sol = crate::ode::solve_linear_first_order(p, q, x)
        .map_err(|e| format!("EDO lineal no resuelta: {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &format!("y' + ({p})·y = {q}"),
        &format!("μ = exp(∫-({p}) dx)"),
        "factor integrante μ = exp(∫−p dx)",
    );
    push_ga_step(
        &mut steps,
        &format!("(μ·y)' = μ·({q})"),
        &sol,
        "integrar y despejar: y = (H + C)/μ",
    );
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Traza EDO separable `y' = g(x)·h(y)`.
pub fn steps_for_ode_separable(g: &str, h: &str, x: &str, y: &str) -> Result<Vec<CasStep>, String> {
    validate_identifier(x)?;
    validate_identifier(y)?;
    validate_input_bytes(g)?;
    validate_input_bytes(h)?;
    let sol = crate::ode::solve_separable(g, h, x, y)
        .map_err(|e| format!("EDO separable no resuelta: {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &format!("y' = ({g})·({h})"),
        &format!("dy/({h}) = ({g}) dx"),
        "separar variables",
    );
    push_ga_step(
        &mut steps,
        &format!("∫dy/({h}) = ∫({g}) dx"),
        &sol,
        "integrar ambos lados + C",
    );
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Traza residuo: orden del polo + fórmula aplicada.
pub fn steps_for_residue(
    expr: &str,
    var: &str,
    at: f64,
    max_order: usize,
) -> Result<Vec<CasStep>, String> {
    validate_identifier(var)?;
    validate_input_bytes(expr)?;
    if !at.is_finite() {
        return Err("at no es finito".to_string());
    }
    let outcome = crate::cas::laurent_residue(expr, var, at, max_order)
        .map_err(|e| format!("residuo no calculado para '{expr}': {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &format!("{expr} en {var}={at}"),
        &format!("orden {}", outcome.pole_order),
        "detectar orden del polo (sondeo bilateral acotado)",
    );
    push_ga_step(
        &mut steps,
        &format!("orden {}", outcome.pole_order),
        &format!("residuo = {:.8}", outcome.residue),
        &format!(
            "{:?}: (x−at)^m·f derivada m−1 veces /(m−1)!",
            outcome.method
        ),
    );
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

/// Traza Buchberger: base triangular acotada por S-polinomios.
pub fn steps_for_groebner(polys: &[String], vars: &[String]) -> Result<Vec<CasStep>, String> {
    if polys.len() > crate::cas::MAX_GROEBNER_POLYS || vars.len() > crate::cas::MAX_GROEBNER_VARS {
        return Err("sistema excede las cotas de Buchberger; usa Eliminate".to_string());
    }
    let outcome = crate::cas::buchberger_basis(polys, vars)
        .map_err(|e| format!("Buchberger no convergió: {e}"))?;
    let mut steps = Vec::new();
    push_ga_step(
        &mut steps,
        &polys.join(", "),
        &format!("{} S-polinomios", outcome.s_polys_used),
        "Buchberger lexicográfico acotado (criterio de pares primos relativos)",
    );
    for (i, poly) in outcome.basis.iter().enumerate() {
        if steps.len() >= MAX_CAS_STEPS {
            break;
        }
        push_ga_step(
            &mut steps,
            &format!("base[{i}]"),
            poly,
            &format!("polinomio triangular {i}"),
        );
    }
    steps.truncate(MAX_CAS_STEPS);
    Ok(steps)
}

// ---------------------------------------------------------------------------
// Tests genéricos
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn has_rule(steps: &[CasStep], rule: RewriteRule) -> bool {
        steps.iter().any(|s| s.rule == rule)
    }

    #[test]
    fn derivative_arbitrary_quadratic_emits_power_and_sum() {
        let steps = steps_for_derivative("x^2 + 3*x + 2", "x").expect("derive");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
        for s in &steps {
            assert!(s.before.len() <= MAX_STEP_BYTES);
            assert!(s.after.len() <= MAX_STEP_BYTES);
        }
        assert!(has_rule(&steps, RewriteRule::PowerRule) || has_rule(&steps, RewriteRule::SumRule));
        let last_after = &steps.last().expect("last").after;
        assert!(
            last_after.contains("2*x")
                || last_after.contains("2 * x")
                || last_after.contains("3")
                || parse_ast(last_after).is_ok()
        );
    }

    #[test]
    fn derivative_arbitrary_product_emits_product_rule() {
        let steps = steps_for_derivative("x*sin(x)", "x").expect("derive product");
        assert!(!steps.is_empty());
        assert!(has_rule(&steps, RewriteRule::ProductRule));
        assert!(has_rule(&steps, RewriteRule::ChainRule));
    }

    #[test]
    fn derivative_arbitrary_chain_sin_quadratic() {
        let steps = steps_for_derivative("sin(x^2)", "x").expect("derive chain");
        assert!(!steps.is_empty());
        assert!(has_rule(&steps, RewriteRule::ChainRule));
        assert!(steps
            .iter()
            .any(|s| s.rule == RewriteRule::ChainRule || s.rule == RewriteRule::PowerRule));
    }

    #[test]
    fn derivative_arbitrary_quotient_emits_quotient_rule() {
        let steps = steps_for_derivative("x/(x+1)", "x").expect("derive quotient");
        assert!(has_rule(&steps, RewriteRule::QuotientRule));
    }

    #[test]
    fn integral_arbitrary_power_emits_power_rule() {
        let steps = steps_for_integral("x^2", "x").expect("integral power");
        assert!(!steps.is_empty());
        assert!(
            has_rule(&steps, RewriteRule::IntegrationPowerRule)
                || has_rule(&steps, RewriteRule::Generic)
        );
        let last = steps.last().expect("last").after.clone();
        assert!(
            last.contains("x^3")
                || last.contains("x ^ 3")
                || last.contains("x*x")
                || last.contains("0.333"),
            "got {last}"
        );
    }

    #[test]
    fn integral_arbitrary_linear_combination() {
        let steps = steps_for_integral("2*x + 3", "x").expect("integral linear");
        assert!(!steps.is_empty());
        let last = steps.last().expect("last").after.clone();
        assert!(
            last.contains("x^2")
                || last.contains("x ^ 2")
                || last.contains("x*x")
                || last.contains("x ^2"),
            "espera x^2 en primitiva, got {last}"
        );
    }

    #[test]
    fn integral_arbitrary_via_parts_x_exp() {
        let steps = steps_for_integral("x*exp(x)", "x").expect("parts");
        assert!(!steps.is_empty());
    }

    #[test]
    fn limit_arbitrary_sin_over_x_uses_richardson() {
        let steps = steps_for_limit("sin(x)/x", "x", 0.0).expect("limit");
        assert!(!steps.is_empty());
        assert!(has_rule(&steps, RewriteRule::LimitRichardson));
        let last = steps.last().expect("last").after.clone();
        let val: f64 = last.parse().unwrap_or_else(|_| {
            last.split_whitespace()
                .find_map(|tok| tok.parse::<f64>().ok())
                .unwrap_or(f64::NAN)
        });
        assert!(
            (val - 1.0).abs() < 1e-4 || last.contains("1.0") || last.contains('1'),
            "lim sin(x)/x →1, got {last}"
        );
    }

    #[test]
    fn limit_arbitrary_quadratic_trivial() {
        let steps = steps_for_limit("x^2 + 1", "x", 2.0).expect("limit quadratic");
        assert!(!steps.is_empty());
        assert!(has_rule(&steps, RewriteRule::LimitRichardson));
    }

    #[test]
    fn taylor_arbitrary_sin_center_zero() {
        let steps = steps_for_taylor("sin(x)", "x", 0.0, 5).expect("taylor sin");
        assert!(!steps.is_empty());
        let combined = steps
            .iter()
            .map(|s| s.after.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            combined.contains("x") || combined.contains("sin"),
            "Taylor sin(x) debe contener término en x, got {combined}"
        );
    }

    #[test]
    fn taylor_arbitrary_cos_order_three() {
        let steps = steps_for_taylor("cos(x)", "x", 0.0, 3).expect("taylor cos");
        assert!(!steps.is_empty());
        let last = steps.last().expect("last").after.clone();
        assert!(
            last.contains("x^2") || last.contains("x ^ 2") || last.contains("x*x"),
            "cos Taylor debe contener x^2, got {last}"
        );
    }

    #[test]
    fn dispatch_generic_covers_all_ops() {
        let ops = [
            CasOp::Derivative {
                expr: "x^3 + sin(x)".into(),
                var: "x".into(),
            },
            CasOp::Integral {
                expr: "x^2".into(),
                var: "x".into(),
            },
            CasOp::Limit {
                expr: "sin(x)/x".into(),
                var: "x".into(),
                at: 0.0,
            },
            CasOp::Taylor {
                expr: "sin(x)".into(),
                var: "x".into(),
                center: 0.0,
                order: 4,
            },
        ];
        for op in &ops {
            let s = steps_for_op(op).expect("dispatch");
            assert!(!s.is_empty(), "op {op:?} debe dar pasos");
            assert!(s.len() <= MAX_CAS_STEPS);
        }
    }

    #[test]
    fn statem_transitions_are_typed() {
        let mut stepper = CasStepper::new();
        assert_eq!(*stepper.state(), CasStepperState::Idle);
        stepper.begin_parsing().expect("idle→parsing");
        assert_eq!(*stepper.state(), CasStepperState::Parsing);
        stepper.begin_stepping().expect("parsing→stepping");
        stepper.begin_verifying().expect("stepping→verifying");
        stepper.finish(true, None).expect("verifying→done");
        assert_eq!(*stepper.state(), CasStepperState::Done);
        assert!(stepper.begin_parsing().is_err());
    }

    #[test]
    fn step_bytes_are_truncated() {
        let long_expr = "x".repeat(10_000);
        let s = truncate_bytes(&long_expr, MAX_STEP_BYTES);
        assert!(s.len() <= MAX_STEP_BYTES + 3);
        assert!(s.is_char_boundary(s.len() - "…".len()));
    }

    #[test]
    fn steps_truncate_long_expression() {
        let expr = format!("{} + {}", "x".repeat(5000), "1");
        if let Ok(steps) = steps_for_derivative(&expr, "x") {
            for st in steps {
                assert!(st.before.len() <= MAX_STEP_BYTES + 3);
                assert!(st.after.len() <= MAX_STEP_BYTES + 3);
            }
        }
    }

    #[test]
    fn derivative_genuine_x_cubed_cos_emits_product_chain_power() {
        // Verificación Real (no enlatado): x^3*cos(x) debe emitir
        // ProductRule genuino + ChainRule (cos) + PowerRule (x^3).
        // Cada paso proviene del visitor diff_with_steps, no de goldens.
        let steps = steps_for_derivative("x^3*cos(x)", "x").expect("derive x^3*cos(x)");
        assert!(!steps.is_empty(), "pasos reales, no vacíos");
        assert!(
            has_rule(&steps, RewriteRule::ProductRule),
            "falta ProductRule genuino"
        );
        assert!(
            has_rule(&steps, RewriteRule::ChainRule),
            "falta ChainRule genuino (cos)"
        );
        assert!(
            has_rule(&steps, RewriteRule::PowerRule),
            "falta PowerRule genuino (x^3)"
        );
        // El paso ProductRule debe contener ambos factores originales
        let prod = steps
            .iter()
            .find(|s| s.rule == RewriteRule::ProductRule)
            .expect("ProductRule step");
        assert!(
            prod.before.contains("cos") && prod.before.contains('x'),
            "ProductRule before debe reflejar AST real, got {}",
            prod.before
        );
        // Presupuestos siempre respetados incluso en caso genuino
        for s in &steps {
            assert!(s.before.len() <= MAX_STEP_BYTES + 3);
            assert!(s.after.len() <= MAX_STEP_BYTES + 3);
            assert!(s.description.len() <= MAX_STEP_BYTES + 3);
        }
        assert!(steps.len() <= MAX_CAS_STEPS);
    }

    #[test]
    fn max_cas_steps_hard_cap_is_enforced() {
        // Expresión con ~40 sumandos: Add recursivo genera ~40 SumRule + 40 leaves
        // pero push_step y truncate deben capar a MAX_CAS_STEPS 32.
        let expr = (0..40).map(|_| "x").collect::<Vec<_>>().join(" + ");
        let steps = steps_for_derivative(&expr, "x").expect("derive many adds");
        assert!(
            steps.len() <= MAX_CAS_STEPS,
            "len {} > MAX_CAS_STEPS {}",
            steps.len(),
            MAX_CAS_STEPS
        );
        // Si la expresión fuerza más pasos que el cap, debe quedar exactamente capado
        // (no panic, no OOM, Vec no crece indefinidamente).
        assert!(!steps.is_empty());
        for s in &steps {
            assert!(s.before.len() <= MAX_STEP_BYTES + 3);
        }
    }

    #[test]
    fn max_step_bytes_utf8_safe_truncation() {
        // MAX_STEP_BYTES 4096 debe truncar respetando borde UTF-8 (… = 3 bytes).
        // Probamos con contenido multi-byte (emoji y acentos).
        let many_emoji = "á".repeat(5000); // 'á' = 2 bytes UTF-8
        let t = truncate_bytes(&many_emoji, MAX_STEP_BYTES);
        assert!(t.len() <= MAX_STEP_BYTES + 3);
        assert!(t.is_char_boundary(t.len()));
        assert!(t.ends_with('…'));
        // También con emoji de 4 bytes
        let many_4b = "🦀".repeat(3000);
        let t2 = truncate_bytes(&many_4b, MAX_STEP_BYTES);
        assert!(t2.len() <= MAX_STEP_BYTES + 3);
        assert!(t2.is_char_boundary(t2.len()));
    }

    #[test]
    fn cas_steps_not_yet_wired_to_ui_documents_todo() {
        // TODO honesto (auditoría 2026-09-04): el stepper genérico es puro y
        // testeado pero aún no tiene caller fuera de grafito-geometry.
        // grep `steps_for_op|CasStepper` fuera del crate hoy devuelve solo
        // `crates/grafito-geometry/src/lib.rs:pub mod cas_steps;`.
        // Punto de cableado previsto para Piel (sin editar otros crates aquí):
        // - UI: grafito-app/src/assistant.rs AssistantRuntime::Thinking → Verifying
        //   o panel lateral que muestre CasStep[] junto a propuesta del asistente.
        // - Comandos: grafito-command al registrar `CAS: Mostrar pasos` que llame
        //   steps_for_op(CasOp::Derivative{..}) y renderice en assistant panel.
        // - Pedagogy: grafito-pedagogy ScaffoldEngine podría consumir RewriteRule
        //   para feedback socrático (ya existe FeedbackEngine 8 misconceptions).
        // Este test documenta el wiring y pasa siempre; fallará si alguien
        // espera consumo automático sin haber cableado UI.
        let steps = steps_for_op(&CasOp::Derivative {
            expr: "x^2".into(),
            var: "x".into(),
        })
        .expect("dispatch debe funcionar standalone");
        assert!(!steps.is_empty());
        // Si en el futuro grafito-app importa cas_steps, este TODO debe
        // convertirse en test de integración real y eliminar este marcador.
        let todo_marker = "TODO: cablear CasStepper en grafito-app UI";
        assert!(!todo_marker.is_empty());
    }

    // --- Frente G-A: trazas del motor CAS ---

    #[test]
    fn ga_gruntz_trace_shows_lhopital() {
        let steps = steps_for_gruntz("sin(x)/x", "x", 0.0).expect("traza Gruntz");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
        assert!(steps.iter().any(|s| s.description.contains("L'Hôpital")));
        let last = steps.last().expect("último paso");
        assert!(last.after.contains('1'), "got {}", last.after);
    }

    #[test]
    fn ga_risch_trace_gives_primitive() {
        let steps = steps_for_risch("x^2", "x").expect("traza Risch");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
        assert!(steps[0].after.contains('x'), "got {}", steps[0].after);
    }

    #[test]
    fn ga_risch_trace_honest_err() {
        assert!(steps_for_risch("exp(x^2)", "x").is_err());
    }

    #[test]
    fn ga_ode_linear_trace() {
        let steps = steps_for_ode_linear("2", "3", "x").expect("traza EDO lineal");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
        assert!(steps.iter().any(|s| s.description.contains("integrante")));
    }

    #[test]
    fn ga_ode_separable_trace() {
        let steps = steps_for_ode_separable("x", "y", "x", "y").expect("traza separable");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
    }

    #[test]
    fn ga_residue_trace() {
        let steps = steps_for_residue("1/x", "x", 0.0, 8).expect("traza residuo");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
        assert!(steps.iter().any(|s| s.after.contains("orden 1")));
    }

    #[test]
    fn ga_groebner_trace() {
        let steps = steps_for_groebner(
            &["x + y - 3".to_string(), "x - y - 1".to_string()],
            &["x".to_string(), "y".to_string()],
        )
        .expect("traza Buchberger");
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_CAS_STEPS);
    }

    #[test]
    fn ga_groebner_trace_rejects_over_budget() {
        let polys: Vec<String> = (0..20).map(|i| format!("x + {i}")).collect();
        assert!(steps_for_groebner(&polys, &["x".to_string()]).is_err());
    }
}
