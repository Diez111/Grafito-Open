use std::fmt;

/// Symbolic Expression AST for Grafito calculus engine.
/// Supports differentiation, simplification, display and numeric evaluation.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Const(f64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Tan(Box<Expr>),
    Asin(Box<Expr>),
    Acos(Box<Expr>),
    Atan(Box<Expr>),
    Exp(Box<Expr>),
    Ln(Box<Expr>),
    Log(Box<Expr>),  // log base 10
    Sqrt(Box<Expr>),
    Abs(Box<Expr>),
    Sinh(Box<Expr>),
    Cosh(Box<Expr>),
    Tanh(Box<Expr>),
}

impl Expr {
    /// Symbolic differentiation with respect to `var`.
    pub fn diff(&self, var: &str) -> Expr {
        use Expr::*;
        match self {
            Const(_) => Const(0.0),
            Var(v) => if v == var { Const(1.0) } else { Const(0.0) },

            Neg(u) => Neg(Box::new(u.diff(var))),

            Add(a, b) => Add(Box::new(a.diff(var)), Box::new(b.diff(var))),
            Sub(a, b) => Sub(Box::new(a.diff(var)), Box::new(b.diff(var))),

            // Product rule: (u*v)' = u'v + uv'
            Mul(u, v) => {
                let du = u.diff(var);
                let dv = v.diff(var);
                Add(
                    Box::new(Mul(Box::new(du), v.clone())),
                    Box::new(Mul(u.clone(), Box::new(dv))),
                )
            }

            // Quotient rule: (u/v)' = (u'v - uv') / v²
            Div(u, v) => {
                let du = u.diff(var);
                let dv = v.diff(var);
                Div(
                    Box::new(Sub(
                        Box::new(Mul(Box::new(du), v.clone())),
                        Box::new(Mul(u.clone(), Box::new(dv))),
                    )),
                    Box::new(Pow(v.clone(), Box::new(Const(2.0)))),
                )
            }

            // Power rule: if v is Const(n), use n*u^(n-1)*u'
            // else use general: (u^v)' = u^v * (v'*ln(u) + v*u'/u)
            Pow(u, v) => {
                let du = u.diff(var);
                match v.as_ref() {
                    Const(n) => {
                        let n = *n;
                        // n * u^(n-1) * u'
                        Mul(
                            Box::new(Mul(
                                Box::new(Const(n)),
                                Box::new(Pow(u.clone(), Box::new(Const(n - 1.0)))),
                            )),
                            Box::new(du),
                        )
                    }
                    _ => {
                        let dv = v.diff(var);
                        // u^v * (v'*ln(u) + v*u'/u)
                        Mul(
                            Box::new(self.clone()),
                            Box::new(Add(
                                Box::new(Mul(Box::new(dv), Box::new(Ln(u.clone())))),
                                Box::new(Mul(
                                    v.clone(),
                                    Box::new(Div(Box::new(du), u.clone())),
                                )),
                            )),
                        )
                    }
                }
            }

            // Chain rule: sin(u)' = cos(u)*u'
            Sin(u) => Mul(Box::new(Cos(u.clone())), Box::new(u.diff(var))),
            // cos(u)' = -sin(u)*u'
            Cos(u) => Mul(Box::new(Neg(Box::new(Sin(u.clone())))), Box::new(u.diff(var))),
            // tan(u)' = sec²(u)*u' = u'/cos²(u)
            Tan(u) => Mul(
                Box::new(Div(Box::new(Const(1.0)), Box::new(Pow(Box::new(Cos(u.clone())), Box::new(Const(2.0)))))),
                Box::new(u.diff(var)),
            ),
            // asin(u)' = u'/sqrt(1 - u²)
            Asin(u) => Div(
                Box::new(u.diff(var)),
                Box::new(Sqrt(Box::new(Sub(Box::new(Const(1.0)), Box::new(Pow(u.clone(), Box::new(Const(2.0)))))))),
            ),
            // acos(u)' = -u'/sqrt(1 - u²)
            Acos(u) => Div(
                Box::new(Neg(Box::new(u.diff(var)))),
                Box::new(Sqrt(Box::new(Sub(Box::new(Const(1.0)), Box::new(Pow(u.clone(), Box::new(Const(2.0)))))))),
            ),
            // atan(u)' = u'/(1 + u²)
            Atan(u) => Div(
                Box::new(u.diff(var)),
                Box::new(Add(Box::new(Const(1.0)), Box::new(Pow(u.clone(), Box::new(Const(2.0)))))),
            ),
            // exp(u)' = exp(u)*u'
            Exp(u) => Mul(Box::new(self.clone()), Box::new(u.diff(var))),
            // ln(u)' = u'/u
            Ln(u) => Div(Box::new(u.diff(var)), u.clone()),
            // log10(u)' = u'/(u*ln(10))
            Log(u) => Div(
                Box::new(u.diff(var)),
                Box::new(Mul(u.clone(), Box::new(Const(std::f64::consts::LN_10)))),
            ),
            // sqrt(u)' = u'/(2*sqrt(u))
            Sqrt(u) => Div(
                Box::new(u.diff(var)),
                Box::new(Mul(Box::new(Const(2.0)), Box::new(Sqrt(u.clone())))),
            ),
            // |u|' = sign(u)*u' (implemented as u/|u| * u')
            Abs(u) => Mul(
                Box::new(Div(u.clone(), Box::new(Abs(u.clone())))),
                Box::new(u.diff(var)),
            ),
            // sinh(u)' = cosh(u)*u'
            Sinh(u) => Mul(Box::new(Cosh(u.clone())), Box::new(u.diff(var))),
            // cosh(u)' = sinh(u)*u'
            Cosh(u) => Mul(Box::new(Sinh(u.clone())), Box::new(u.diff(var))),
            // tanh(u)' = sech²(u)*u' = u'/cosh²(u)
            Tanh(u) => Mul(
                Box::new(Div(Box::new(Const(1.0)), Box::new(Pow(Box::new(Cosh(u.clone())), Box::new(Const(2.0)))))),
                Box::new(u.diff(var)),
            ),
        }
    }

    /// Evaluate numerically by substituting var=value

    pub fn substitute_vars(&self, vars: &std::collections::HashMap<String, f64>, ignore: &[&str]) -> Expr {
        use Expr::*;
        match self {
            Const(c) => Const(*c),
            Var(v) => {
                if ignore.contains(&v.as_str()) {
                    Var(v.clone())
                } else if let Some(&val) = vars.get(v) {
                    Const(val)
                } else {
                    Var(v.clone())
                }
            }
            Neg(u) => Neg(Box::new(u.substitute_vars(vars, ignore))),
            Add(a, b) => Add(Box::new(a.substitute_vars(vars, ignore)), Box::new(b.substitute_vars(vars, ignore))),
            Sub(a, b) => Sub(Box::new(a.substitute_vars(vars, ignore)), Box::new(b.substitute_vars(vars, ignore))),
            Mul(a, b) => Mul(Box::new(a.substitute_vars(vars, ignore)), Box::new(b.substitute_vars(vars, ignore))),
            Div(a, b) => Div(Box::new(a.substitute_vars(vars, ignore)), Box::new(b.substitute_vars(vars, ignore))),
            Pow(a, b) => Pow(Box::new(a.substitute_vars(vars, ignore)), Box::new(b.substitute_vars(vars, ignore))),
            Sin(u) => Sin(Box::new(u.substitute_vars(vars, ignore))),
            Cos(u) => Cos(Box::new(u.substitute_vars(vars, ignore))),
            Tan(u) => Tan(Box::new(u.substitute_vars(vars, ignore))),
            Asin(u) => Asin(Box::new(u.substitute_vars(vars, ignore))),
            Acos(u) => Acos(Box::new(u.substitute_vars(vars, ignore))),
            Atan(u) => Atan(Box::new(u.substitute_vars(vars, ignore))),
            Exp(u) => Exp(Box::new(u.substitute_vars(vars, ignore))),
            Ln(u) => Ln(Box::new(u.substitute_vars(vars, ignore))),
            Log(u) => Log(Box::new(u.substitute_vars(vars, ignore))),
            Sqrt(u) => Sqrt(Box::new(u.substitute_vars(vars, ignore))),
            Abs(u) => Abs(Box::new(u.substitute_vars(vars, ignore))),
            Sinh(u) => Sinh(Box::new(u.substitute_vars(vars, ignore))),
            Cosh(u) => Cosh(Box::new(u.substitute_vars(vars, ignore))),
            Tanh(u) => Tanh(Box::new(u.substitute_vars(vars, ignore))),
        }
    }

    pub fn eval_2d(&self, var1: &str, val1: f64, var2: &str, val2: f64) -> f64 {
        use Expr::*;
        match self {
            Const(c) => *c,
            Var(v) => {
                if v == var1 { val1 } else if v == var2 { val2 } else { f64::NAN }
            }
            Neg(u) => -u.eval_2d(var1, val1, var2, val2),
            Add(a, b) => a.eval_2d(var1, val1, var2, val2) + b.eval_2d(var1, val1, var2, val2),
            Sub(a, b) => a.eval_2d(var1, val1, var2, val2) - b.eval_2d(var1, val1, var2, val2),
            Mul(a, b) => a.eval_2d(var1, val1, var2, val2) * b.eval_2d(var1, val1, var2, val2),
            Div(a, b) => {
                let den = b.eval_2d(var1, val1, var2, val2);
                if den.abs() < 1e-300 { f64::NAN } else { a.eval_2d(var1, val1, var2, val2) / den }
            }
            Pow(a, b) => a.eval_2d(var1, val1, var2, val2).powf(b.eval_2d(var1, val1, var2, val2)),
            Sin(u) => u.eval_2d(var1, val1, var2, val2).sin(),
            Cos(u) => u.eval_2d(var1, val1, var2, val2).cos(),
            Tan(u) => u.eval_2d(var1, val1, var2, val2).tan(),
            Asin(u) => u.eval_2d(var1, val1, var2, val2).asin(),
            Acos(u) => u.eval_2d(var1, val1, var2, val2).acos(),
            Atan(u) => u.eval_2d(var1, val1, var2, val2).atan(),
            Exp(u) => u.eval_2d(var1, val1, var2, val2).exp(),
            Ln(u) => u.eval_2d(var1, val1, var2, val2).ln(),
            Log(u) => u.eval_2d(var1, val1, var2, val2).log10(),
            Sqrt(u) => u.eval_2d(var1, val1, var2, val2).sqrt(),
            Abs(u) => u.eval_2d(var1, val1, var2, val2).abs(),
            Sinh(u) => u.eval_2d(var1, val1, var2, val2).sinh(),
            Cosh(u) => u.eval_2d(var1, val1, var2, val2).cosh(),
            Tanh(u) => u.eval_2d(var1, val1, var2, val2).tanh(),
        }
    }

    pub fn eval_at(&self, var: &str, value: f64) -> f64 {
        use Expr::*;
        match self {
            Const(c) => *c,
            Var(v) => if v == var { value } else { f64::NAN },
            Neg(u) => -u.eval_at(var, value),
            Add(a, b) => a.eval_at(var, value) + b.eval_at(var, value),
            Sub(a, b) => a.eval_at(var, value) - b.eval_at(var, value),
            Mul(a, b) => a.eval_at(var, value) * b.eval_at(var, value),
            Div(a, b) => {
                let den = b.eval_at(var, value);
                if den.abs() < 1e-300 { f64::NAN } else { a.eval_at(var, value) / den }
            }
            Pow(a, b) => a.eval_at(var, value).powf(b.eval_at(var, value)),
            Sin(u) => u.eval_at(var, value).sin(),
            Cos(u) => u.eval_at(var, value).cos(),
            Tan(u) => u.eval_at(var, value).tan(),
            Asin(u) => u.eval_at(var, value).asin(),
            Acos(u) => u.eval_at(var, value).acos(),
            Atan(u) => u.eval_at(var, value).atan(),
            Exp(u) => u.eval_at(var, value).exp(),
            Ln(u) => u.eval_at(var, value).ln(),
            Log(u) => u.eval_at(var, value).log10(),
            Sqrt(u) => u.eval_at(var, value).sqrt(),
            Abs(u) => u.eval_at(var, value).abs(),
            Sinh(u) => u.eval_at(var, value).sinh(),
            Cosh(u) => u.eval_at(var, value).cosh(),
            Tanh(u) => u.eval_at(var, value).tanh(),
        }
    }

    /// Simplify expression (constant folding + algebraic identities).
    pub fn simplify(&self) -> Expr {
        // Recursive simplify
        let s = self.simplify_once();
        // Run a second pass for deeper simplification
        s.simplify_once()
    }

    fn simplify_once(&self) -> Expr {
        use Expr::*;
        match self {
            Neg(a) => {
                let sa = a.simplify_once();
                match sa {
                    Const(c) => Const(-c),
                    Neg(inner) => *inner,
                    _ => Neg(Box::new(sa)),
                }
            }
            Add(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca + cb),
                    (Const(ca), _) if *ca == 0.0 => sb,
                    (_, Const(cb)) if *cb == 0.0 => sa,
                    _ => Add(Box::new(sa), Box::new(sb)),
                }
            }
            Sub(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca - cb),
                    (_, Const(cb)) if *cb == 0.0 => sa,
                    (Const(ca), _) if *ca == 0.0 => Neg(Box::new(sb)),
                    _ => Sub(Box::new(sa), Box::new(sb)),
                }
            }
            Mul(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca * cb),
                    (Const(ca), _) if *ca == 0.0 => Const(0.0),
                    (_, Const(cb)) if *cb == 0.0 => Const(0.0),
                    (Const(ca), _) if *ca == 1.0 => sb,
                    (_, Const(cb)) if *cb == 1.0 => sa,
                    (Const(ca), _) if *ca == -1.0 => Neg(Box::new(sb)),
                    (_, Const(cb)) if *cb == -1.0 => Neg(Box::new(sa)),
                    // Combine constants: (c * f) * g = c * (f * g) doesn't help much
                    // Combine const*const at inner level
                    (Mul(x, y), _) => {
                        if let Const(c1) = x.as_ref() {
                            if let Const(c2) = sb {
                                return Mul(Box::new(Const(c1 * c2)), y.clone());
                            }
                        }
                        Mul(Box::new(sa), Box::new(sb))
                    }
                    _ => Mul(Box::new(sa), Box::new(sb)),
                }
            }
            Div(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) if cb.abs() > 1e-300 => Const(ca / cb),
                    (Const(ca), _) if *ca == 0.0 => Const(0.0),
                    (_, Const(cb)) if *cb == 1.0 => sa,
                    _ => Div(Box::new(sa), Box::new(sb)),
                }
            }
            Pow(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca.powf(*cb)),
                    (_, Const(cb)) if *cb == 0.0 => Const(1.0),
                    (_, Const(cb)) if *cb == 1.0 => sa,
                    (Const(ca), _) if *ca == 1.0 => Const(1.0),
                    (Const(ca), _) if *ca == 0.0 => Const(0.0),
                    _ => Pow(Box::new(sa), Box::new(sb)),
                }
            }
            Sin(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.sin()) } else { Sin(Box::new(sa)) } }
            Cos(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.cos()) } else { Cos(Box::new(sa)) } }
            Tan(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.tan()) } else { Tan(Box::new(sa)) } }
            Asin(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.asin()) } else { Asin(Box::new(sa)) } }
            Acos(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.acos()) } else { Acos(Box::new(sa)) } }
            Atan(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.atan()) } else { Atan(Box::new(sa)) } }
            Exp(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.exp()) } else { Exp(Box::new(sa)) } }
            Ln(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.ln()) } else { Ln(Box::new(sa)) } }
            Log(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.log10()) } else { Log(Box::new(sa)) } }
            Sqrt(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.sqrt()) } else { Sqrt(Box::new(sa)) } }
            Abs(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.abs()) } else { Abs(Box::new(sa)) } }
            Sinh(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.sinh()) } else { Sinh(Box::new(sa)) } }
            Cosh(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.cosh()) } else { Cosh(Box::new(sa)) } }
            Tanh(a) => { let sa = a.simplify_once(); if let Const(c) = sa { Const(c.tanh()) } else { Tanh(Box::new(sa)) } }
            _ => self.clone(),
        }
    }

    /// Convert AST back to a clean math string (for display in Grafito).
    pub fn to_expr_string(&self) -> String {
        use Expr::*;
        match self {
            Const(c) => {
                // Show as integer if possible
                if (c.fract()).abs() < 1e-10 && c.abs() < 1e15 {
                    format!("{}", *c as i64)
                } else {
                    format!("{:.6}", c).trim_end_matches('0').trim_end_matches('.').to_string()
                }
            }
            Var(v) => v.clone(),
            Neg(u) => format!("-({})", u.to_expr_string()),
            Add(a, b) => format!("{} + {}", a.to_expr_string_paren(1), b.to_expr_string_paren(1)),
            Sub(a, b) => format!("{} - {}", a.to_expr_string_paren(1), b.to_expr_string_paren(2)),
            Mul(a, b) => format!("{} * {}", a.to_expr_string_paren(2), b.to_expr_string_paren(3)),
            Div(a, b) => format!("{} / {}", a.to_expr_string_paren(2), b.to_expr_string_paren(3)),
            Pow(a, b) => format!("{} ^ {}", a.to_expr_string_paren(3), b.to_expr_string_paren(4)),
            Sin(u) => format!("sin({})", u.to_expr_string()),
            Cos(u) => format!("cos({})", u.to_expr_string()),
            Tan(u) => format!("tan({})", u.to_expr_string()),
            Asin(u) => format!("asin({})", u.to_expr_string()),
            Acos(u) => format!("acos({})", u.to_expr_string()),
            Atan(u) => format!("atan({})", u.to_expr_string()),
            Exp(u) => format!("exp({})", u.to_expr_string()),
            Ln(u) => format!("ln({})", u.to_expr_string()),
            Log(u) => format!("log({})", u.to_expr_string()),
            Sqrt(u) => format!("sqrt({})", u.to_expr_string()),
            Abs(u) => format!("abs({})", u.to_expr_string()),
            Sinh(u) => format!("sinh({})", u.to_expr_string()),
            Cosh(u) => format!("cosh({})", u.to_expr_string()),
            Tanh(u) => format!("tanh({})", u.to_expr_string()),
        }
    }

    /// Helper: wrap in parentheses if operator priority is lower than `min_prec`.
    fn to_expr_string_paren(&self, min_prec: u8) -> String {
        use Expr::*;
        let prec = match self {
            Const(_) | Var(_) => 10u8,
            Sin(_) | Cos(_) | Tan(_) | Asin(_) | Acos(_) | Atan(_)
            | Exp(_) | Ln(_) | Log(_) | Sqrt(_) | Abs(_) | Sinh(_) | Cosh(_) | Tanh(_) => 10,
            Pow(_, _) => 4,
            Mul(_, _) | Div(_, _) => 2,
            Add(_, _) | Sub(_, _) => 1,
            Neg(_) => 3,
        };
        if prec < min_prec {
            format!("({})", self.to_expr_string())
        } else {
            self.to_expr_string()
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_expr_string())
    }
}

// ============================================================
// Parser: text -> AST
// ============================================================

pub fn parse_ast(expr: &str) -> Result<Expr, String> {
    // Preprocess: replace common math notations
    let expr = preprocess(expr);
    let mut tokens = tokenize(&expr);
    let result = parse_add_sub(&mut tokens)?;
    if !tokens.is_empty() {
        return Err(format!("Unexpected tokens remaining: {:?}", tokens));
    }
    Ok(result)
}

fn preprocess(expr: &str) -> String {
    let expr = expr.trim().to_string();
    // Replace π with pi literal value
    let expr = expr.replace("π", "3.141592653589793");
    let expr = replace_standalone(&expr, "pi", "3.141592653589793");
    let expr = replace_standalone(&expr, "e", "2.718281828459045");
    // Handle implicit multiplication: 2x -> 2*x, x2 -> x^2? No, keep simple
    expr
}

/// Replace `pattern` with `replacement` only when it's a standalone token
/// (not part of a larger identifier).
fn replace_standalone(expr: &str, pattern: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(expr.len() + replacement.len());
    let mut chars = expr.chars().peekable();
    let mut prev_char: Option<char> = None;
    let mut byte_offset = 0;
    
    while let Some(c) = chars.next() {
        let c_byte_len = c.len_utf8();
        if expr[byte_offset..].starts_with(pattern) {
            let pattern_len = pattern.len();
            let after = byte_offset + pattern_len;
            let next_char = if after < expr.len() { expr[after..].chars().next() } else { None };
            
            let prev_is_ident = prev_char.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
            let next_is_ident = next_char.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false);
            
            if !prev_is_ident && !next_is_ident {
                result.push_str(replacement);
                let pattern_char_count = pattern.chars().count();
                for _ in 0..pattern_char_count {
                    if let Some(skipped) = chars.next() {
                        byte_offset += skipped.len_utf8();
                    }
                }
                prev_char = pattern.chars().last();
                continue;
            }
        }
        
        result.push(c);
        prev_char = Some(c);
        byte_offset += c_byte_len;
    }
    
    result
}

fn tokenize(expr: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            i += 1;
            continue;
        }
        if "+-*/^(),".contains(c) {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(c.to_string());
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            // If we're mid-number, push number first
            if !current.is_empty() && current.chars().all(|ch| ch.is_numeric() || ch == '.') {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(c);
            i += 1;
        } else if c.is_numeric() || c == '.' {
            // If we're mid-identifier, push it first
            if !current.is_empty() && current.chars().all(|ch| ch.is_alphabetic() || ch == '_') {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(c);
            i += 1;
        } else {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            i += 1;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_add_sub(tokens: &mut Vec<String>) -> Result<Expr, String> {
    let mut lhs = parse_mul_div(tokens)?;
    while !tokens.is_empty() {
        match tokens[0].as_str() {
            "+" => { tokens.remove(0); let rhs = parse_mul_div(tokens)?; lhs = Expr::Add(Box::new(lhs), Box::new(rhs)); }
            "-" => { tokens.remove(0); let rhs = parse_mul_div(tokens)?; lhs = Expr::Sub(Box::new(lhs), Box::new(rhs)); }
            _ => break,
        }
    }
    Ok(lhs)
}

fn parse_mul_div(tokens: &mut Vec<String>) -> Result<Expr, String> {
    let mut lhs = parse_unary(tokens)?;
    while !tokens.is_empty() {
        match tokens[0].as_str() {
            "*" => { tokens.remove(0); let rhs = parse_unary(tokens)?; lhs = Expr::Mul(Box::new(lhs), Box::new(rhs)); }
            "/" => { tokens.remove(0); let rhs = parse_unary(tokens)?; lhs = Expr::Div(Box::new(lhs), Box::new(rhs)); }
            _ => break,
        }
    }
    Ok(lhs)
}

fn parse_unary(tokens: &mut Vec<String>) -> Result<Expr, String> {
    if !tokens.is_empty() && tokens[0] == "-" {
        tokens.remove(0);
        let inner = parse_pow(tokens)?;
        return Ok(Expr::Neg(Box::new(inner)));
    }
    if !tokens.is_empty() && tokens[0] == "+" {
        tokens.remove(0);
    }
    parse_pow(tokens)
}

fn parse_pow(tokens: &mut Vec<String>) -> Result<Expr, String> {
    let base = parse_primary(tokens)?;
    if !tokens.is_empty() && tokens[0] == "^" {
        tokens.remove(0);
        // Right-associative
        let exp = parse_unary(tokens)?;
        return Ok(Expr::Pow(Box::new(base), Box::new(exp)));
    }
    Ok(base)
}

fn parse_primary(tokens: &mut Vec<String>) -> Result<Expr, String> {
    if tokens.is_empty() {
        return Err("Unexpected end of expression".into());
    }
    let token = tokens.remove(0);
    // Parenthesized expression
    if token == "(" {
        let inner = parse_add_sub(tokens)?;
        if tokens.is_empty() || tokens[0] != ")" {
            return Err("Missing closing parenthesis".into());
        }
        tokens.remove(0);
        return Ok(inner);
    }
    // Numeric constant
    if let Ok(val) = token.parse::<f64>() {
        return Ok(Expr::Const(val));
    }
    // Named constant or function or variable
    if token.chars().all(|c| c.is_alphanumeric() || c == '_') {
        // Check if it's a function call (next token is "(")
        if !tokens.is_empty() && tokens[0] == "(" {
            tokens.remove(0); // consume "("
            let arg = parse_add_sub(tokens)?;
            // Optional: comma-separated extra args (ignored for now)
            while !tokens.is_empty() && tokens[0] == "," {
                tokens.remove(0);
                let _ = parse_add_sub(tokens)?; // ignore extra args for single-arg funcs
            }
            if tokens.is_empty() || tokens[0] != ")" {
                return Err(format!("Missing closing parenthesis for function '{}'", token));
            }
            tokens.remove(0);
            return Ok(match token.to_lowercase().as_str() {
                "sin" => Expr::Sin(Box::new(arg)),
                "cos" => Expr::Cos(Box::new(arg)),
                "tan" => Expr::Tan(Box::new(arg)),
                "asin" | "arcsin" => Expr::Asin(Box::new(arg)),
                "acos" | "arccos" => Expr::Acos(Box::new(arg)),
                "atan" | "arctan" => Expr::Atan(Box::new(arg)),
                "exp" => Expr::Exp(Box::new(arg)),
                "ln" => Expr::Ln(Box::new(arg)),
                "log" | "log10" => Expr::Log(Box::new(arg)),
                "log2" => Expr::Div(
                    Box::new(Expr::Ln(Box::new(arg))),
                    Box::new(Expr::Ln(Box::new(Expr::Const(2.0)))),
                ),
                "sqrt" => Expr::Sqrt(Box::new(arg)),
                "abs" => Expr::Abs(Box::new(arg)),
                "sinh" => Expr::Sinh(Box::new(arg)),
                "cosh" => Expr::Cosh(Box::new(arg)),
                "tanh" => Expr::Tanh(Box::new(arg)),
                _ => return Err(format!("Unknown function: {}", token)),
            });
        }
        // Variable
        return Ok(Expr::Var(token));
    }
    Err(format!("Unexpected token: '{}'", token))
}

// ============================================================
// Calculus Helpers: Numerical integration, limits
// ============================================================

/// Numerical definite integral using adaptive Gauss-Legendre 5-point quadrature.
pub fn integrate_numeric(expr: &str, _var: &str, a: f64, b: f64) -> f64 {
    // Gauss-Legendre 5-point nodes and weights on [-1,1]
    let nodes = [-0.906179845938664, -0.538469310105683, 0.0, 0.538469310105683, 0.906179845938664];
    let weights = [0.236926885056189, 0.478628670499366, 0.568888888888889, 0.478628670499366, 0.236926885056189];
    
    let mid = (a + b) / 2.0;
    let half = (b - a) / 2.0;
    let mut sum = 0.0;
    for (&xi, &wi) in nodes.iter().zip(weights.iter()) {
        let t = mid + half * xi;
        let val = crate::expr::eval_function(expr, t).unwrap_or(0.0);
        if val.is_finite() {
            sum += wi * val;
        }
    }
    sum * half
}

/// Adaptive integration: subdivide interval for better precision.
pub fn integrate_adaptive(expr: &str, var: &str, a: f64, b: f64, depth: u32) -> f64 {
    if depth == 0 {
        return integrate_numeric(expr, var, a, b);
    }
    let mid = (a + b) / 2.0;
    integrate_adaptive(expr, var, a, mid, depth - 1)
        + integrate_adaptive(expr, var, mid, b, depth - 1)
}

/// Compute limit numerically by approaching from left and right.
pub fn compute_limit(expr: &str, var: &str, at: f64) -> Option<f64> {
    let h_values = [1e-4, 1e-5, 1e-6, 1e-7, 1e-8];
    let mut left_vals = Vec::new();
    let mut right_vals = Vec::new();
    
    for &h in &h_values {
        let left = crate::expr::eval_function_var(expr, var, at - h).unwrap_or(f64::NAN);
        let right = crate::expr::eval_function_var(expr, var, at + h).unwrap_or(f64::NAN);
        if left.is_finite() { left_vals.push(left); }
        if right.is_finite() { right_vals.push(right); }
    }
    
    if left_vals.is_empty() || right_vals.is_empty() {
        return None;
    }
    
    let left_lim = left_vals.last().copied().unwrap_or(f64::NAN);
    let right_lim = right_vals.last().copied().unwrap_or(f64::NAN);
    
    if !left_lim.is_finite() || !right_lim.is_finite() {
        return None;
    }
    
    // Check if both sides agree (within tolerance)
    let tol = 1e-4;
    if (left_lim - right_lim).abs() < tol {
        Some((left_lim + right_lim) / 2.0)
    } else {
        None // Limit doesn't exist (or is one-sided)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_polynomial() {
        // x^3 -> 3*x^2
        let expr = parse_ast("x^3").unwrap();
        let d = expr.diff("x").simplify();
        // Numerically verify at x=2: should be 12
        let val = d.eval_at("x", 2.0);
        assert!((val - 12.0).abs() < 1e-9, "Expected 12, got {}", val);
    }

    #[test]
    fn test_diff_sin() {
        // sin(x) -> cos(x)
        let expr = parse_ast("sin(x)").unwrap();
        let d = expr.diff("x").simplify();
        let val = d.eval_at("x", 0.0);
        assert!((val - 1.0).abs() < 1e-9, "Expected 1 (cos 0), got {}", val);
    }

    #[test]
    fn test_diff_product() {
        // x*sin(x) -> sin(x) + x*cos(x)
        let expr = parse_ast("x*sin(x)").unwrap();
        let d = expr.diff("x").simplify();
        // At x=0: sin(0) + 0*cos(0) = 0
        let val = d.eval_at("x", 0.0);
        assert!((val - 0.0).abs() < 1e-9, "Expected 0, got {}", val);
        // At x=pi/2: sin(pi/2) + pi/2*cos(pi/2) = 1 + 0 = 1
        let pi = std::f64::consts::PI;
        let val2 = d.eval_at("x", pi / 2.0);
        assert!((val2 - 1.0).abs() < 1e-6, "Expected 1 at pi/2, got {}", val2);
    }

    #[test]
    fn test_integral_sin() {
        // ∫sin(x)dx from 0 to pi = 2
        let result = integrate_adaptive("sin(x)", "x", 0.0, std::f64::consts::PI, 6);
        assert!((result - 2.0).abs() < 1e-6, "Expected 2, got {}", result);
    }

    #[test]
    fn test_limit_sinc() {
        // lim x->0 sin(x)/x = 1
        let result = compute_limit("sin(x)/x", "x", 0.0);
        assert!(result.is_some());
        assert!((result.unwrap() - 1.0).abs() < 1e-4, "Expected 1, got {:?}", result);
    }
}
