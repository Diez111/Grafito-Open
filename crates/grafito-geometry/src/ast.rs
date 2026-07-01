use std::fmt;

// Symbolic Expression AST for Grafito calculus engine.
// Supports differentiation, simplification, display and numeric evaluation.

// Reduce a large angle to the [0, 2π) range to avoid precision loss in sin/cos/tan.
fn reduce_angle(a: f64) -> f64 {
    if a.is_finite() {
        a.rem_euclid(std::f64::consts::TAU)
    } else {
        a
    }
}

/// Convierte un f64 a orden de Bessel (i32) de forma segura.
/// Redondea al entero más cercano, y devuelve 0 para NaN/Infinito.
pub fn bessel_order(f: f64) -> i32 {
    if !f.is_finite() {
        return 0;
    }
    let rounded = f.round();
    if rounded > 1000.0 {
        1000
    } else if rounded < -1000.0 {
        -1000
    } else {
        rounded as i32
    }
}

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
    Log(Box<Expr>), // log base 10
    Sqrt(Box<Expr>),
    Abs(Box<Expr>),
    Sinh(Box<Expr>),
    Cosh(Box<Expr>),
    Tanh(Box<Expr>),
    // Rounding
    Floor(Box<Expr>),
    Ceil(Box<Expr>),
    Round(Box<Expr>),
    // Reciprocal trig
    Sec(Box<Expr>),
    Csc(Box<Expr>),
    Cot(Box<Expr>),
    // Inverse hyperbolic
    Asinh(Box<Expr>),
    Acosh(Box<Expr>),
    Atanh(Box<Expr>),
    // Misc
    Sign(Box<Expr>),      // signum
    Heaviside(Box<Expr>), // unit step
    Cbrt(Box<Expr>),      // cube root
    // Two-arg
    Atan2(Box<Expr>, Box<Expr>),
    Modulo(Box<Expr>, Box<Expr>),
    Min(Box<Expr>, Box<Expr>),
    Max(Box<Expr>, Box<Expr>),
    Clamp(Box<Expr>, Box<Expr>, Box<Expr>), // clamp(x, lo, hi)
    // Complex
    Re(Box<Expr>),
    Im(Box<Expr>),
    Arg(Box<Expr>),
    Conj(Box<Expr>),
    // Special functions (1-arg)
    Erf(Box<Expr>),
    Erfc(Box<Expr>),
    Gamma(Box<Expr>),
    LnGamma(Box<Expr>),
    Digamma(Box<Expr>),
    // Special functions (2-arg)
    Beta(Box<Expr>, Box<Expr>),
    BesselJ(Box<Expr>, Box<Expr>),
    BesselY(Box<Expr>, Box<Expr>),
    BesselI(Box<Expr>, Box<Expr>),
    // Iteration (native sum/product)
    Sum(Box<Expr>, String, Box<Expr>, Box<Expr>), // (body, var, start, end)
    Product(Box<Expr>, String, Box<Expr>, Box<Expr>),
    // Piecewise
    Piecewise(Vec<(Box<Expr>, Box<Expr>)>, Box<Expr>), // conditions+values, default
    // Comparison / relational operators (evaluate to 0.0 or 1.0)
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn get_variables(&self, vars: &mut std::collections::HashSet<String>) {
        use Expr::*;
        match self {
            Const(_) => {}
            Var(v) => {
                vars.insert(v.clone());
            }
            Neg(u) | Sin(u) | Cos(u) | Tan(u) | Asin(u) | Acos(u) | Atan(u) | Exp(u) | Ln(u)
            | Log(u) | Sqrt(u) | Abs(u) | Sinh(u) | Cosh(u) | Tanh(u) | Floor(u) | Ceil(u)
            | Round(u) | Sec(u) | Csc(u) | Cot(u) | Asinh(u) | Acosh(u) | Atanh(u) | Sign(u)
            | Heaviside(u) | Cbrt(u) | Re(u) | Im(u) | Arg(u) | Conj(u) | Erf(u) | Erfc(u)
            | Gamma(u) | LnGamma(u) | Digamma(u) => {
                u.get_variables(vars);
            }
            Add(a, b)
            | Sub(a, b)
            | Mul(a, b)
            | Div(a, b)
            | Pow(a, b)
            | Atan2(a, b)
            | Modulo(a, b)
            | Min(a, b)
            | Max(a, b)
            | Beta(a, b)
            | BesselJ(a, b)
            | BesselY(a, b)
            | BesselI(a, b)
            | Lt(a, b)
            | Gt(a, b)
            | Le(a, b)
            | Ge(a, b)
            | Eq(a, b)
            | Ne(a, b) => {
                a.get_variables(vars);
                b.get_variables(vars);
            }
            Clamp(x, lo, hi) => {
                x.get_variables(vars);
                lo.get_variables(vars);
                hi.get_variables(vars);
            }
            Sum(body, loop_var, start, end) | Product(body, loop_var, start, end) => {
                let mut body_vars = std::collections::HashSet::new();
                body.get_variables(&mut body_vars);
                body_vars.remove(loop_var);
                vars.extend(body_vars);
                start.get_variables(vars);
                end.get_variables(vars);
            }
            Piecewise(pieces, default) => {
                for (c, v) in pieces {
                    c.get_variables(vars);
                    v.get_variables(vars);
                }
                default.get_variables(vars);
            }
        }
    }

    /// Symbolic differentiation with respect to `var`.
    pub fn diff(&self, var: &str) -> Expr {
        self.diff_depth(var, 0)
    }

    fn diff_depth(&self, var: &str, depth: u32) -> Expr {
        const MAX_DIFF_DEPTH: u32 = 256;
        if depth > MAX_DIFF_DEPTH {
            return Expr::Const(f64::NAN);
        }
        use Expr::*;
        match self {
            Const(_) => Const(0.0),
            Var(v) => {
                if v == var {
                    Const(1.0)
                } else {
                    Const(0.0)
                }
            }

            Neg(u) => Neg(Box::new(u.diff_depth(var, depth + 1))),

            Add(a, b) => Add(
                Box::new(a.diff_depth(var, depth + 1)),
                Box::new(b.diff_depth(var, depth + 1)),
            ),
            Sub(a, b) => Sub(
                Box::new(a.diff_depth(var, depth + 1)),
                Box::new(b.diff_depth(var, depth + 1)),
            ),

            // Product rule: (u*v)' = u'v + uv'
            Mul(u, v) => {
                let du = u.diff_depth(var, depth + 1);
                let dv = v.diff_depth(var, depth + 1);
                Add(
                    Box::new(Mul(Box::new(du), v.clone())),
                    Box::new(Mul(u.clone(), Box::new(dv))),
                )
            }

            // Quotient rule: (u/v)' = (u'v - uv') / v²
            Div(u, v) => {
                let du = u.diff_depth(var, depth + 1);
                let dv = v.diff_depth(var, depth + 1);
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
                let du = u.diff_depth(var, depth + 1);
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
                        let dv = v.diff_depth(var, depth + 1);
                        // u^v * (v'*ln(u) + v*u'/u)
                        Mul(
                            Box::new(self.clone()),
                            Box::new(Add(
                                Box::new(Mul(Box::new(dv), Box::new(Ln(u.clone())))),
                                Box::new(Mul(v.clone(), Box::new(Div(Box::new(du), u.clone())))),
                            )),
                        )
                    }
                }
            }

            // Chain rule: sin(u)' = cos(u)*u'
            Sin(u) => Mul(
                Box::new(Cos(u.clone())),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // cos(u)' = -sin(u)*u'
            Cos(u) => Mul(
                Box::new(Neg(Box::new(Sin(u.clone())))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // tan(u)' = sec²(u)*u' = u'/cos²(u)
            Tan(u) => Mul(
                Box::new(Div(
                    Box::new(Const(1.0)),
                    Box::new(Pow(Box::new(Cos(u.clone())), Box::new(Const(2.0)))),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // asin(u)' = u'/sqrt(1 - u²)
            Asin(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            ),
            // acos(u)' = -u'/sqrt(1 - u²)
            Acos(u) => Div(
                Box::new(Neg(Box::new(u.diff_depth(var, depth + 1)))),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )))),
            ),
            // atan(u)' = u'/(1 + u²)
            Atan(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Add(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )),
            ),
            // exp(u)' = exp(u)*u'
            Exp(u) => Mul(
                Box::new(self.clone()),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // ln(u)' = u'/u
            Ln(u) => Div(Box::new(u.diff_depth(var, depth + 1)), u.clone()),
            // log10(u)' = u'/(u*ln(10))
            Log(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Mul(u.clone(), Box::new(Const(std::f64::consts::LN_10)))),
            ),
            // sqrt(u)' = u'/(2*sqrt(u))
            Sqrt(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Mul(Box::new(Const(2.0)), Box::new(Sqrt(u.clone())))),
            ),
            // |u|' = sign(u)*u' (implemented as u/|u| * u')
            Abs(u) => Mul(
                Box::new(Div(u.clone(), Box::new(Abs(u.clone())))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // sinh(u)' = cosh(u)*u'
            Sinh(u) => Mul(
                Box::new(Cosh(u.clone())),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // cosh(u)' = sinh(u)*u'
            Cosh(u) => Mul(
                Box::new(Sinh(u.clone())),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // tanh(u)' = sech²(u)*u' = u'/cosh²(u)
            Tanh(u) => Mul(
                Box::new(Div(
                    Box::new(Const(1.0)),
                    Box::new(Pow(Box::new(Cosh(u.clone())), Box::new(Const(2.0)))),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // floor/ceil/round: zero almost everywhere
            Floor(_u) => Const(0.0),
            Ceil(_u) => Const(0.0),
            Round(_u) => Const(0.0),
            // sec(u)' = sec(u)*tan(u)*u'
            Sec(u) => Mul(
                Box::new(Mul(Box::new(Sec(u.clone())), Box::new(Tan(u.clone())))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // csc(u)' = -csc(u)*cot(u)*u'
            Csc(u) => Mul(
                Box::new(Neg(Box::new(Mul(
                    Box::new(Csc(u.clone())),
                    Box::new(Cot(u.clone())),
                )))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // cot(u)' = -csc²(u)*u'
            Cot(u) => Mul(
                Box::new(Neg(Box::new(Pow(
                    Box::new(Csc(u.clone())),
                    Box::new(Const(2.0)),
                )))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // asinh(u)' = u'/sqrt(u²+1)
            Asinh(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Sqrt(Box::new(Add(
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                    Box::new(Const(1.0)),
                )))),
            ),
            // acosh(u)' = u'/sqrt(u²-1)
            Acosh(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Sqrt(Box::new(Sub(
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                    Box::new(Const(1.0)),
                )))),
            ),
            // atanh(u)' = u'/(1-u²)
            Atanh(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Sub(
                    Box::new(Const(1.0)),
                    Box::new(Pow(u.clone(), Box::new(Const(2.0)))),
                )),
            ),
            Sign(_) => Const(0.0),
            Heaviside(_) => Const(0.0),
            // cbrt(u)' = u'/(3*cbrt(u)²)
            Cbrt(u) => Div(
                Box::new(u.diff_depth(var, depth + 1)),
                Box::new(Mul(
                    Box::new(Const(3.0)),
                    Box::new(Pow(Box::new(Cbrt(u.clone())), Box::new(Const(2.0)))),
                )),
            ),
            // atan2(y,x) partial derivatives
            Atan2(y, x) => Div(
                Box::new(Sub(
                    Box::new(Mul(x.clone(), Box::new(y.diff_depth(var, depth + 1)))),
                    Box::new(Mul(y.clone(), Box::new(x.diff_depth(var, depth + 1)))),
                )),
                Box::new(Add(
                    Box::new(Pow(x.clone(), Box::new(Const(2.0)))),
                    Box::new(Pow(y.clone(), Box::new(Const(2.0)))),
                )),
            ),
            Modulo(_, _) => Const(0.0),
            Min(a, b) => {
                let da = a.diff_depth(var, depth + 1);
                let db = b.diff_depth(var, depth + 1);
                Expr::Piecewise(
                    vec![(Box::new(Expr::Sub(a.clone(), b.clone())), Box::new(da))],
                    Box::new(db),
                )
            }
            Max(a, b) => {
                let da = a.diff_depth(var, depth + 1);
                let db = b.diff_depth(var, depth + 1);
                Expr::Piecewise(
                    vec![(Box::new(Expr::Sub(b.clone(), a.clone())), Box::new(db))],
                    Box::new(da),
                )
            }
            Clamp(x, lo, hi) => {
                let dx = x.diff_depth(var, depth + 1);
                Expr::Piecewise(
                    vec![
                        (
                            Box::new(Expr::Sub(hi.clone(), x.clone())),
                            Box::new(Const(0.0)),
                        ),
                        (
                            Box::new(Expr::Sub(lo.clone(), x.clone())),
                            Box::new(Const(0.0)),
                        ),
                    ],
                    Box::new(dx),
                )
            }
            Re(u) => u.diff_depth(var, depth + 1), // re(x) = x for real x
            Im(_) => Const(0.0),                   // im(x) = 0 for real x
            Arg(_) => Const(0.0),                  // arg(x) = 0 for real x
            Conj(u) => u.diff_depth(var, depth + 1), // conj(x) = x for real x
            // erf'(u) = (2/sqrt(pi))*exp(-u²)*u'
            Erf(u) => Mul(
                Box::new(Mul(
                    Box::new(Const(2.0 / std::f64::consts::PI.sqrt())),
                    Box::new(Exp(Box::new(Neg(Box::new(Pow(
                        u.clone(),
                        Box::new(Const(2.0)),
                    )))))),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // erfc'(u) = -(2/sqrt(pi))*exp(-u²)*u'
            Erfc(u) => Mul(
                Box::new(Neg(Box::new(Mul(
                    Box::new(Const(2.0 / std::f64::consts::PI.sqrt())),
                    Box::new(Exp(Box::new(Neg(Box::new(Pow(
                        u.clone(),
                        Box::new(Const(2.0)),
                    )))))),
                )))),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            // gamma'(u) = gamma(u)*digamma(u)*u'
            Gamma(u) => Mul(
                Box::new(Mul(
                    Box::new(Gamma(u.clone())),
                    Box::new(Digamma(u.clone())),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            LnGamma(u) => Mul(
                Box::new(Digamma(u.clone())),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            Digamma(_) => Const(0.0), // polygamma would be needed
            Beta(a, b) => {
                let da = a.diff_depth(var, depth + 1);
                let db = b.diff_depth(var, depth + 1);
                // beta'(a,b) = beta(a,b)*(digamma(a)*a' - digamma(a+b)*(a'+b'))
                Mul(
                    Box::new(Beta(a.clone(), b.clone())),
                    Box::new(Sub(
                        Box::new(Mul(Box::new(Digamma(a.clone())), Box::new(da.clone()))),
                        Box::new(Mul(
                            Box::new(Digamma(Box::new(Add(a.clone(), b.clone())))),
                            Box::new(Add(Box::new(da), Box::new(db))),
                        )),
                    )),
                )
            }
            BesselJ(n, u) => Mul(
                Box::new(Sub(
                    Box::new(Div(
                        Box::new(Mul(n.clone(), Box::new(BesselJ(n.clone(), u.clone())))),
                        u.clone(),
                    )),
                    Box::new(BesselJ(
                        Box::new(Add(n.clone(), Box::new(Const(1.0)))),
                        u.clone(),
                    )),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            BesselY(n, u) => Mul(
                Box::new(Sub(
                    Box::new(Div(
                        Box::new(Mul(n.clone(), Box::new(BesselY(n.clone(), u.clone())))),
                        u.clone(),
                    )),
                    Box::new(BesselY(
                        Box::new(Add(n.clone(), Box::new(Const(1.0)))),
                        u.clone(),
                    )),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            BesselI(n, u) => Mul(
                Box::new(Add(
                    Box::new(BesselI(
                        Box::new(Add(n.clone(), Box::new(Const(1.0)))),
                        u.clone(),
                    )),
                    Box::new(Div(
                        Box::new(Mul(n.clone(), Box::new(BesselI(n.clone(), u.clone())))),
                        u.clone(),
                    )),
                )),
                Box::new(u.diff_depth(var, depth + 1)),
            ),
            Sum(body, v, start, end) => {
                // derivative of sum: sum of derivatives
                Sum(
                    Box::new(body.diff_depth(var, depth + 1)),
                    v.clone(),
                    start.clone(),
                    end.clone(),
                )
            }
            Product(body, v, start, end) => {
                // derivative of product: product * sum(expr'/expr)
                let body_ref = body.clone();
                Mul(
                    Box::new(Product(
                        body_ref.clone(),
                        v.clone(),
                        start.clone(),
                        end.clone(),
                    )),
                    Box::new(Sum(
                        Box::new(Div(Box::new(body.diff_depth(var, depth + 1)), body_ref)),
                        v.clone(),
                        start.clone(),
                        end.clone(),
                    )),
                )
            }
            Lt(_, _) | Gt(_, _) | Le(_, _) | Ge(_, _) | Eq(_, _) | Ne(_, _) => Const(0.0),
            Piecewise(pieces, default) => Piecewise(
                pieces
                    .iter()
                    .map(|(cond, val)| (cond.clone(), Box::new(val.diff_depth(var, depth + 1))))
                    .collect(),
                Box::new(default.diff_depth(var, depth + 1)),
            ),
        }
    }

    /// Evaluate numerically by substituting var=value
    pub fn substitute_vars(
        &self,
        vars: &std::collections::HashMap<String, f64>,
        ignore: &[&str],
    ) -> Expr {
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
            Add(a, b) => Add(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Sub(a, b) => Sub(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Mul(a, b) => Mul(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Div(a, b) => Div(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Pow(a, b) => Pow(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Lt(a, b) => Lt(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Gt(a, b) => Gt(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Le(a, b) => Le(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Ge(a, b) => Ge(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Eq(a, b) => Eq(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Ne(a, b) => Ne(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
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
            Floor(u) => Floor(Box::new(u.substitute_vars(vars, ignore))),
            Ceil(u) => Ceil(Box::new(u.substitute_vars(vars, ignore))),
            Round(u) => Round(Box::new(u.substitute_vars(vars, ignore))),
            Sec(u) => Sec(Box::new(u.substitute_vars(vars, ignore))),
            Csc(u) => Csc(Box::new(u.substitute_vars(vars, ignore))),
            Cot(u) => Cot(Box::new(u.substitute_vars(vars, ignore))),
            Asinh(u) => Asinh(Box::new(u.substitute_vars(vars, ignore))),
            Acosh(u) => Acosh(Box::new(u.substitute_vars(vars, ignore))),
            Atanh(u) => Atanh(Box::new(u.substitute_vars(vars, ignore))),
            Sign(u) => Sign(Box::new(u.substitute_vars(vars, ignore))),
            Heaviside(u) => Heaviside(Box::new(u.substitute_vars(vars, ignore))),
            Cbrt(u) => Cbrt(Box::new(u.substitute_vars(vars, ignore))),
            Atan2(a, b) => Atan2(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Modulo(a, b) => Modulo(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Min(a, b) => Min(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Max(a, b) => Max(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            Clamp(x, lo, hi) => Clamp(
                Box::new(x.substitute_vars(vars, ignore)),
                Box::new(lo.substitute_vars(vars, ignore)),
                Box::new(hi.substitute_vars(vars, ignore)),
            ),
            Re(u) => Re(Box::new(u.substitute_vars(vars, ignore))),
            Im(u) => Im(Box::new(u.substitute_vars(vars, ignore))),
            Arg(u) => Arg(Box::new(u.substitute_vars(vars, ignore))),
            Conj(u) => Conj(Box::new(u.substitute_vars(vars, ignore))),
            Erf(u) => Erf(Box::new(u.substitute_vars(vars, ignore))),
            Erfc(u) => Erfc(Box::new(u.substitute_vars(vars, ignore))),
            Gamma(u) => Gamma(Box::new(u.substitute_vars(vars, ignore))),
            LnGamma(u) => LnGamma(Box::new(u.substitute_vars(vars, ignore))),
            Digamma(u) => Digamma(Box::new(u.substitute_vars(vars, ignore))),
            Beta(a, b) => Beta(
                Box::new(a.substitute_vars(vars, ignore)),
                Box::new(b.substitute_vars(vars, ignore)),
            ),
            BesselJ(n, u) => BesselJ(
                Box::new(n.substitute_vars(vars, ignore)),
                Box::new(u.substitute_vars(vars, ignore)),
            ),
            BesselY(n, u) => BesselY(
                Box::new(n.substitute_vars(vars, ignore)),
                Box::new(u.substitute_vars(vars, ignore)),
            ),
            BesselI(n, u) => BesselI(
                Box::new(n.substitute_vars(vars, ignore)),
                Box::new(u.substitute_vars(vars, ignore)),
            ),
            Sum(body, v, start, end) => {
                let new_v = v.clone();
                Sum(
                    Box::new(body.substitute_vars(vars, ignore)),
                    new_v,
                    Box::new(start.substitute_vars(vars, ignore)),
                    Box::new(end.substitute_vars(vars, ignore)),
                )
            }
            Product(body, v, start, end) => {
                let new_v = v.clone();
                Product(
                    Box::new(body.substitute_vars(vars, ignore)),
                    new_v,
                    Box::new(start.substitute_vars(vars, ignore)),
                    Box::new(end.substitute_vars(vars, ignore)),
                )
            }
            Piecewise(pieces, default) => Piecewise(
                pieces
                    .iter()
                    .map(|(c, v)| {
                        (
                            Box::new(c.substitute_vars(vars, ignore)),
                            Box::new(v.substitute_vars(vars, ignore)),
                        )
                    })
                    .collect(),
                Box::new(default.substitute_vars(vars, ignore)),
            ),
        }
    }

    pub fn eval_2d(&self, var1: &str, val1: f64, var2: &str, val2: f64) -> f64 {
        self.eval_2d_depth(var1, val1, var2, val2, 0)
    }

    fn eval_2d_depth(&self, var1: &str, val1: f64, var2: &str, val2: f64, depth: u32) -> f64 {
        const MAX_EVAL_2D_DEPTH: u32 = 256;
        if depth > MAX_EVAL_2D_DEPTH {
            return f64::NAN;
        }
        use Expr::*;
        match self {
            Const(c) => *c,
            Var(v) => {
                if v == var1 {
                    val1
                } else if v == var2 {
                    val2
                } else {
                    f64::NAN
                }
            }
            Neg(u) => -u.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            Add(a, b) => {
                a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    + b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
            }
            Sub(a, b) => {
                a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    - b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
            }
            Mul(a, b) => {
                a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    * b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
            }
            Div(a, b) => {
                let den = b.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if den.abs() < 1e-300 {
                    f64::NAN
                } else {
                    a.eval_2d_depth(var1, val1, var2, val2, depth + 1) / den
                }
            }
            Pow(a, b) => {
                let a_val = a.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                let b_val = b.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if (a_val < 0.0 && b_val != b_val.trunc()) || (a_val == 0.0 && b_val < 0.0) {
                    f64::NAN
                } else {
                    a_val.powf(b_val)
                }
            }
            Sin(u) => reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).sin(),
            Cos(u) => reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).cos(),
            Tan(u) => reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).tan(),
            Asin(u) => {
                let v = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if v.abs() > 1.0 {
                    f64::NAN
                } else {
                    v.asin()
                }
            }
            Acos(u) => {
                let v = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if v.abs() > 1.0 {
                    f64::NAN
                } else {
                    v.acos()
                }
            }
            Atan(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).atan(),
            Exp(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).exp(),
            Ln(u) => {
                let v = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if v <= 0.0 {
                    f64::NAN
                } else {
                    v.ln()
                }
            }
            Log(u) => {
                let v = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if v <= 0.0 {
                    f64::NAN
                } else {
                    v.log10()
                }
            }
            Sqrt(u) => {
                let v = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if v < 0.0 {
                    f64::NAN
                } else {
                    v.sqrt()
                }
            }
            Abs(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).abs(),
            Sinh(u) => {
                let a = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.sinh()
                }
            }
            Cosh(u) => {
                let a = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.cosh()
                }
            }
            Tanh(u) => {
                let a = u.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.tanh()
                }
            }
            Floor(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).floor(),
            Ceil(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).ceil(),
            Round(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).round(),
            Sec(u) => {
                let c = reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).cos();
                if c.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / c
                }
            }
            Csc(u) => {
                let s = reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).sin();
                if s.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / s
                }
            }
            Cot(u) => {
                let t = reduce_angle(u.eval_2d_depth(var1, val1, var2, val2, depth + 1)).tan();
                if t.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / t
                }
            }
            Asinh(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).asinh(),
            Acosh(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).acosh(),
            Atanh(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).atanh(),
            Sign(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).signum(),
            Heaviside(u) => {
                if u.eval_2d_depth(var1, val1, var2, val2, depth + 1) < 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Cbrt(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1).cbrt(),
            Atan2(a, b) => a
                .eval_2d_depth(var1, val1, var2, val2, depth + 1)
                .atan2(b.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
            Modulo(a, b) => {
                a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    % b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
            }
            Min(a, b) => a
                .eval_2d_depth(var1, val1, var2, val2, depth + 1)
                .min(b.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
            Max(a, b) => a
                .eval_2d_depth(var1, val1, var2, val2, depth + 1)
                .max(b.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
            Clamp(x, lo, hi) => x.eval_2d_depth(var1, val1, var2, val2, depth + 1).clamp(
                lo.eval_2d_depth(var1, val1, var2, val2, depth + 1),
                hi.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            ),
            Re(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1), // re(x) = x for real
            Im(_) => 0.0,                                                // im(x) = 0 for real
            Arg(u) => {
                if u.eval_2d_depth(var1, val1, var2, val2, depth + 1) >= 0.0 {
                    0.0
                } else {
                    std::f64::consts::PI
                }
            }
            Conj(u) => u.eval_2d_depth(var1, val1, var2, val2, depth + 1), // conj(x) = x for real
            Erf(u) => {
                crate::special_functions::erf(u.eval_2d_depth(var1, val1, var2, val2, depth + 1))
            }
            Erfc(u) => {
                crate::special_functions::erfc(u.eval_2d_depth(var1, val1, var2, val2, depth + 1))
            }
            Gamma(u) => {
                crate::special_functions::gamma(u.eval_2d_depth(var1, val1, var2, val2, depth + 1))
            }
            LnGamma(u) => crate::special_functions::ln_gamma(u.eval_2d_depth(
                var1,
                val1,
                var2,
                val2,
                depth + 1,
            )),
            Digamma(u) => crate::special_functions::digamma(u.eval_2d_depth(
                var1,
                val1,
                var2,
                val2,
                depth + 1,
            )),
            Beta(a, b) => crate::special_functions::beta(
                a.eval_2d_depth(var1, val1, var2, val2, depth + 1),
                b.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            ),
            BesselJ(n, u) => crate::special_functions::bessel_j(
                bessel_order(n.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
                u.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            ),
            BesselY(n, u) => crate::special_functions::bessel_y(
                bessel_order(n.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
                u.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            ),
            BesselI(n, u) => crate::special_functions::bessel_i(
                bessel_order(n.eval_2d_depth(var1, val1, var2, val2, depth + 1)),
                u.eval_2d_depth(var1, val1, var2, val2, depth + 1),
            ),
            Sum(_, _, _, _) => f64::NAN, // expanded by preprocess_expr before AST eval
            Product(_, _, _, _) => f64::NAN,
            Piecewise(pieces, default) => {
                for (cond, val) in pieces {
                    if cond.eval_2d_depth(var1, val1, var2, val2, depth + 1) != 0.0 {
                        return val.eval_2d_depth(var1, val1, var2, val2, depth + 1);
                    }
                }
                default.eval_2d_depth(var1, val1, var2, val2, depth + 1)
            }
            Lt(a, b) => {
                if a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    < b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Gt(a, b) => {
                if a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    > b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Le(a, b) => {
                if a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    <= b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ge(a, b) => {
                if a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    >= b.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Eq(a, b) => {
                if (a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    - b.eval_2d_depth(var1, val1, var2, val2, depth + 1))
                .abs()
                    < 1e-12
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ne(a, b) => {
                if (a.eval_2d_depth(var1, val1, var2, val2, depth + 1)
                    - b.eval_2d_depth(var1, val1, var2, val2, depth + 1))
                .abs()
                    < 1e-12
                {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }

    pub fn eval_3d(
        &self,
        var1: &str,
        val1: f64,
        var2: &str,
        val2: f64,
        var3: &str,
        val3: f64,
    ) -> f64 {
        self.eval_3d_depth(var1, val1, var2, val2, var3, val3, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_3d_depth(
        &self,
        var1: &str,
        val1: f64,
        var2: &str,
        val2: f64,
        var3: &str,
        val3: f64,
        depth: u32,
    ) -> f64 {
        const MAX_EVAL_3D_DEPTH: u32 = 256;
        if depth > MAX_EVAL_3D_DEPTH {
            return f64::NAN;
        }
        use Expr::*;
        match self {
            Const(c) => *c,
            Var(v) => {
                if v == var1 {
                    val1
                } else if v == var2 {
                    val2
                } else if v == var3 {
                    val3
                } else {
                    f64::NAN
                }
            }
            Neg(u) => -u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
            Add(a, b) => {
                a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    + b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
            }
            Sub(a, b) => {
                a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    - b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
            }
            Mul(a, b) => {
                a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    * b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
            }
            Div(a, b) => {
                let den = b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1);
                if den.abs() < 1e-300 {
                    f64::NAN
                } else {
                    a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1) / den
                }
            }
            Pow(a, b) => a
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .powf(b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
            Sin(u) => {
                reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)).sin()
            }
            Cos(u) => {
                reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)).cos()
            }
            Tan(u) => {
                reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)).tan()
            }
            Asin(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .asin(),
            Acos(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .acos(),
            Atan(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .atan(),
            Exp(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .exp(),
            Ln(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .ln(),
            Log(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .log10(),
            Sqrt(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .sqrt(),
            Abs(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .abs(),
            Sinh(u) => {
                let a = u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.sinh()
                }
            }
            Cosh(u) => {
                let a = u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.cosh()
                }
            }
            Tanh(u) => {
                let a = u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.tanh()
                }
            }
            Floor(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .floor(),
            Ceil(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .ceil(),
            Round(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .round(),
            Sec(u) => {
                let c =
                    reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1))
                        .cos();
                if c.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / c
                }
            }
            Csc(u) => {
                let s =
                    reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1))
                        .sin();
                if s.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / s
                }
            }
            Cot(u) => {
                let t =
                    reduce_angle(u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1))
                        .tan();
                if t.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / t
                }
            }
            Asinh(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .asinh(),
            Acosh(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .acosh(),
            Atanh(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .atanh(),
            Sign(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .signum(),
            Heaviside(u) => {
                if u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1) < 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Cbrt(u) => u
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .cbrt(),
            Atan2(a, b) => a
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .atan2(b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
            Modulo(a, b) => {
                a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    % b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
            }
            Min(a, b) => a
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .min(b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
            Max(a, b) => a
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .max(b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
            Clamp(x, lo, hi) => x
                .eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                .clamp(
                    lo.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
                    hi.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
                ),
            Re(u) => u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1), // re(x) = x for real
            Im(_) => 0.0, // im(x) = 0 for real
            Arg(u) => {
                if u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1) >= 0.0 {
                    0.0
                } else {
                    std::f64::consts::PI
                }
            }
            Conj(u) => u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1), // conj(x) = x for real
            Erf(u) => crate::special_functions::erf(u.eval_3d_depth(
                var1,
                val1,
                var2,
                val2,
                var3,
                val3,
                depth + 1,
            )),
            Erfc(u) => crate::special_functions::erfc(u.eval_3d_depth(
                var1,
                val1,
                var2,
                val2,
                var3,
                val3,
                depth + 1,
            )),
            Gamma(u) => crate::special_functions::gamma(u.eval_3d_depth(
                var1,
                val1,
                var2,
                val2,
                var3,
                val3,
                depth + 1,
            )),
            LnGamma(u) => crate::special_functions::ln_gamma(u.eval_3d_depth(
                var1,
                val1,
                var2,
                val2,
                var3,
                val3,
                depth + 1,
            )),
            Digamma(u) => crate::special_functions::digamma(u.eval_3d_depth(
                var1,
                val1,
                var2,
                val2,
                var3,
                val3,
                depth + 1,
            )),
            Beta(a, b) => crate::special_functions::beta(
                a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
                b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
            ),
            BesselJ(n, u) => crate::special_functions::bessel_j(
                bessel_order(n.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
                u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
            ),
            BesselY(n, u) => crate::special_functions::bessel_y(
                bessel_order(n.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
                u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
            ),
            BesselI(n, u) => crate::special_functions::bessel_i(
                bessel_order(n.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)),
                u.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1),
            ),
            Lt(a, b) => {
                if a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    < b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Gt(a, b) => {
                if a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    > b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Le(a, b) => {
                if a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    <= b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ge(a, b) => {
                if a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    >= b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Eq(a, b) => {
                if (a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    - b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1))
                .abs()
                    < 1e-12
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ne(a, b) => {
                if (a.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
                    - b.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1))
                .abs()
                    < 1e-12
                {
                    0.0
                } else {
                    1.0
                }
            }
            Sum(_, _, _, _) => f64::NAN, // expanded by preprocess_expr before AST eval
            Product(_, _, _, _) => f64::NAN,
            Piecewise(pieces, default) => {
                for (cond, val) in pieces {
                    if cond.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1) != 0.0 {
                        return val.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1);
                    }
                }
                default.eval_3d_depth(var1, val1, var2, val2, var3, val3, depth + 1)
            }
        }
    }

    pub fn eval_at(&self, var: &str, value: f64) -> f64 {
        self.eval_at_depth(var, value, 0)
    }

    fn eval_at_depth(&self, var: &str, value: f64, depth: u32) -> f64 {
        const MAX_EVAL_DEPTH: u32 = 256;
        if depth > MAX_EVAL_DEPTH {
            return f64::NAN;
        }
        use Expr::*;
        match self {
            Const(c) => *c,
            Var(v) => {
                if v == var {
                    value
                } else {
                    f64::NAN
                }
            }
            Neg(u) => -u.eval_at_depth(var, value, depth + 1),
            Add(a, b) => {
                a.eval_at_depth(var, value, depth + 1) + b.eval_at_depth(var, value, depth + 1)
            }
            Sub(a, b) => {
                a.eval_at_depth(var, value, depth + 1) - b.eval_at_depth(var, value, depth + 1)
            }
            Mul(a, b) => {
                a.eval_at_depth(var, value, depth + 1) * b.eval_at_depth(var, value, depth + 1)
            }
            Div(a, b) => {
                let den = b.eval_at_depth(var, value, depth + 1);
                if den.abs() < 1e-300 {
                    f64::NAN
                } else {
                    a.eval_at_depth(var, value, depth + 1) / den
                }
            }
            Pow(a, b) => {
                a.eval_at_depth(var, value, depth + 1)
                    .powf(b.eval_at_depth(var, value, depth + 1))
            }
            Sin(u) => reduce_angle(u.eval_at_depth(var, value, depth + 1)).sin(),
            Cos(u) => reduce_angle(u.eval_at_depth(var, value, depth + 1)).cos(),
            Tan(u) => reduce_angle(u.eval_at_depth(var, value, depth + 1)).tan(),
            Asin(u) => u.eval_at_depth(var, value, depth + 1).asin(),
            Acos(u) => u.eval_at_depth(var, value, depth + 1).acos(),
            Atan(u) => u.eval_at_depth(var, value, depth + 1).atan(),
            Exp(u) => u.eval_at_depth(var, value, depth + 1).exp(),
            Ln(u) => u.eval_at_depth(var, value, depth + 1).ln(),
            Log(u) => u.eval_at_depth(var, value, depth + 1).log10(),
            Sqrt(u) => u.eval_at_depth(var, value, depth + 1).sqrt(),
            Abs(u) => u.eval_at_depth(var, value, depth + 1).abs(),
            Sinh(u) => {
                let a = u.eval_at_depth(var, value, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.sinh()
                }
            }
            Cosh(u) => {
                let a = u.eval_at_depth(var, value, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.cosh()
                }
            }
            Tanh(u) => {
                let a = u.eval_at_depth(var, value, depth + 1);
                if a.abs() > 1e9 {
                    0.0
                } else {
                    a.tanh()
                }
            }
            Floor(u) => u.eval_at_depth(var, value, depth + 1).floor(),
            Ceil(u) => u.eval_at_depth(var, value, depth + 1).ceil(),
            Round(u) => u.eval_at_depth(var, value, depth + 1).round(),
            Sec(u) => {
                let c = reduce_angle(u.eval_at_depth(var, value, depth + 1)).cos();
                if c.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / c
                }
            }
            Csc(u) => {
                let s = reduce_angle(u.eval_at_depth(var, value, depth + 1)).sin();
                if s.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / s
                }
            }
            Cot(u) => {
                let t = reduce_angle(u.eval_at_depth(var, value, depth + 1)).tan();
                if t.abs() < 1e-15 {
                    f64::NAN
                } else {
                    1.0 / t
                }
            }
            Asinh(u) => u.eval_at_depth(var, value, depth + 1).asinh(),
            Acosh(u) => u.eval_at_depth(var, value, depth + 1).acosh(),
            Atanh(u) => u.eval_at_depth(var, value, depth + 1).atanh(),
            Sign(u) => u.eval_at_depth(var, value, depth + 1).signum(),
            Heaviside(u) => {
                if u.eval_at_depth(var, value, depth + 1) < 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Cbrt(u) => u.eval_at_depth(var, value, depth + 1).cbrt(),
            Atan2(a, b) => a
                .eval_at_depth(var, value, depth + 1)
                .atan2(b.eval_at_depth(var, value, depth + 1)),
            Modulo(a, b) => {
                a.eval_at_depth(var, value, depth + 1) % b.eval_at_depth(var, value, depth + 1)
            }
            Min(a, b) => {
                a.eval_at_depth(var, value, depth + 1)
                    .min(b.eval_at_depth(var, value, depth + 1))
            }
            Max(a, b) => {
                a.eval_at_depth(var, value, depth + 1)
                    .max(b.eval_at_depth(var, value, depth + 1))
            }
            Clamp(x, lo, hi) => x.eval_at_depth(var, value, depth + 1).clamp(
                lo.eval_at_depth(var, value, depth + 1),
                hi.eval_at_depth(var, value, depth + 1),
            ),
            Re(u) => u.eval_at_depth(var, value, depth + 1),
            Im(_) => 0.0,
            Arg(u) => {
                if u.eval_at_depth(var, value, depth + 1) >= 0.0 {
                    0.0
                } else {
                    std::f64::consts::PI
                }
            }
            Conj(u) => u.eval_at_depth(var, value, depth + 1),
            Erf(u) => crate::special_functions::erf(u.eval_at_depth(var, value, depth + 1)),
            Erfc(u) => crate::special_functions::erfc(u.eval_at_depth(var, value, depth + 1)),
            Gamma(u) => crate::special_functions::gamma(u.eval_at_depth(var, value, depth + 1)),
            LnGamma(u) => {
                crate::special_functions::ln_gamma(u.eval_at_depth(var, value, depth + 1))
            }
            Digamma(u) => crate::special_functions::digamma(u.eval_at_depth(var, value, depth + 1)),
            Beta(a, b) => crate::special_functions::beta(
                a.eval_at_depth(var, value, depth + 1),
                b.eval_at_depth(var, value, depth + 1),
            ),
            BesselJ(n, u) => crate::special_functions::bessel_j(
                bessel_order(n.eval_at_depth(var, value, depth + 1)),
                u.eval_at_depth(var, value, depth + 1),
            ),
            BesselY(n, u) => crate::special_functions::bessel_y(
                bessel_order(n.eval_at_depth(var, value, depth + 1)),
                u.eval_at_depth(var, value, depth + 1),
            ),
            BesselI(n, u) => crate::special_functions::bessel_i(
                bessel_order(n.eval_at_depth(var, value, depth + 1)),
                u.eval_at_depth(var, value, depth + 1),
            ),
            Lt(a, b) => {
                if a.eval_at_depth(var, value, depth + 1) < b.eval_at_depth(var, value, depth + 1) {
                    1.0
                } else {
                    0.0
                }
            }
            Gt(a, b) => {
                if a.eval_at_depth(var, value, depth + 1) > b.eval_at_depth(var, value, depth + 1) {
                    1.0
                } else {
                    0.0
                }
            }
            Le(a, b) => {
                if a.eval_at_depth(var, value, depth + 1) <= b.eval_at_depth(var, value, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ge(a, b) => {
                if a.eval_at_depth(var, value, depth + 1) >= b.eval_at_depth(var, value, depth + 1)
                {
                    1.0
                } else {
                    0.0
                }
            }
            Eq(a, b) => {
                if (a.eval_at_depth(var, value, depth + 1) - b.eval_at_depth(var, value, depth + 1))
                    .abs()
                    < 1e-12
                {
                    1.0
                } else {
                    0.0
                }
            }
            Ne(a, b) => {
                if (a.eval_at_depth(var, value, depth + 1) - b.eval_at_depth(var, value, depth + 1))
                    .abs()
                    < 1e-12
                {
                    0.0
                } else {
                    1.0
                }
            }
            Sum(_, _, _, _) => f64::NAN,
            Product(_, _, _, _) => f64::NAN,
            Piecewise(pieces, default) => {
                for (cond, val) in pieces {
                    if cond.eval_at_depth(var, value, depth + 1) != 0.0 {
                        return val.eval_at_depth(var, value, depth + 1);
                    }
                }
                default.eval_at_depth(var, value, depth + 1)
            }
        }
    }

    /// Simplify expression (constant folding + algebraic + trig identities).
    pub fn simplify(&self) -> Expr {
        let s = self.simplify_once();
        let s = s.simplify_once();
        s.trig_simplify()
    }

    /// Cuenta el número de nodos en el AST (recursivo, exhaustivo).
    /// Útil para dimensionar la estrategia de evaluación: ASTs grandes
    /// pueden justificar strides mayores o path de eval distintos.
    pub fn node_count(&self) -> usize {
        use Expr::*;
        match self {
            Const(_) | Var(_) => 1,
            // Unarios
            Neg(a) | Sin(a) | Cos(a) | Tan(a) | Asin(a) | Acos(a) | Atan(a) | Exp(a) | Ln(a)
            | Log(a) | Sqrt(a) | Abs(a) | Sinh(a) | Cosh(a) | Tanh(a) | Asinh(a) | Acosh(a)
            | Atanh(a) | Sec(a) | Csc(a) | Cot(a) | Floor(a) | Ceil(a) | Round(a) | Sign(a)
            | Heaviside(a) | Cbrt(a) | Re(a) | Im(a) | Arg(a) | Conj(a) | Erf(a) | Erfc(a)
            | Gamma(a) | LnGamma(a) | Digamma(a) => 1 + a.node_count(),
            // Binarios
            Add(a, b)
            | Sub(a, b)
            | Mul(a, b)
            | Div(a, b)
            | Pow(a, b)
            | Atan2(a, b)
            | Modulo(a, b)
            | Min(a, b)
            | Max(a, b) => 1 + a.node_count() + b.node_count(),
            // Variantes sin hijos (no debería haber)
            _ => 1,
        }
    }

    fn trig_simplify(&self) -> Expr {
        use Expr::*;
        match self {
            Add(a, b) => {
                let sa = a.trig_simplify();
                let sb = b.trig_simplify();
                if let (Pow(base1, exp1), Pow(base2, exp2)) = (&sa, &sb) {
                    if let (Sin(inner1), Const(2.0), Cos(inner2), Const(2.0)) =
                        (base1.as_ref(), exp1.as_ref(), base2.as_ref(), exp2.as_ref())
                    {
                        if inner1.to_expr_string() == inner2.to_expr_string() {
                            return Const(1.0);
                        }
                    }
                    if let (Cos(inner1), Const(2.0), Sin(inner2), Const(2.0)) =
                        (base1.as_ref(), exp1.as_ref(), base2.as_ref(), exp2.as_ref())
                    {
                        if inner1.to_expr_string() == inner2.to_expr_string() {
                            return Const(1.0);
                        }
                    }
                }
                Add(Box::new(sa), Box::new(sb))
            }
            _ => self.clone(),
        }
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
            Sin(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.sin())
                    }
                } else {
                    Sin(Box::new(sa))
                }
            }
            Cos(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.cos())
                    }
                } else {
                    Cos(Box::new(sa))
                }
            }
            Tan(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.tan())
                    }
                } else {
                    Tan(Box::new(sa))
                }
            }
            Asin(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.asin())
                } else {
                    Asin(Box::new(sa))
                }
            }
            Acos(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.acos())
                } else {
                    Acos(Box::new(sa))
                }
            }
            Atan(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.atan())
                } else {
                    Atan(Box::new(sa))
                }
            }
            Exp(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.exp())
                } else {
                    Exp(Box::new(sa))
                }
            }
            Ln(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.ln())
                } else {
                    Ln(Box::new(sa))
                }
            }
            Log(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.log10())
                } else {
                    Log(Box::new(sa))
                }
            }
            Sqrt(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.sqrt())
                } else {
                    Sqrt(Box::new(sa))
                }
            }
            Abs(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.abs())
                } else {
                    Abs(Box::new(sa))
                }
            }
            Sinh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.sinh())
                    }
                } else {
                    Sinh(Box::new(sa))
                }
            }
            Cosh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.cosh())
                    }
                } else {
                    Cosh(Box::new(sa))
                }
            }
            Tanh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(c.tanh())
                    }
                } else {
                    Tanh(Box::new(sa))
                }
            }
            Floor(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.floor())
                } else {
                    Floor(Box::new(sa))
                }
            }
            Ceil(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.ceil())
                } else {
                    Ceil(Box::new(sa))
                }
            }
            Round(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.round())
                } else {
                    Round(Box::new(sa))
                }
            }
            Sec(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(1.0 / c.cos())
                    }
                } else {
                    Sec(Box::new(sa))
                }
            }
            Csc(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(1.0 / c.sin())
                    }
                } else {
                    Csc(Box::new(sa))
                }
            }
            Cot(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    if c.abs() > 1e9 {
                        Const(0.0)
                    } else {
                        Const(1.0 / c.tan())
                    }
                } else {
                    Cot(Box::new(sa))
                }
            }
            Asinh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.asinh())
                } else {
                    Asinh(Box::new(sa))
                }
            }
            Acosh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.acosh())
                } else {
                    Acosh(Box::new(sa))
                }
            }
            Atanh(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.atanh())
                } else {
                    Atanh(Box::new(sa))
                }
            }
            Sign(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.signum())
                } else {
                    Sign(Box::new(sa))
                }
            }
            Heaviside(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(if c < 0.0 { 0.0 } else { 1.0 })
                } else {
                    Heaviside(Box::new(sa))
                }
            }
            Cbrt(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c.cbrt())
                } else {
                    Cbrt(Box::new(sa))
                }
            }
            Atan2(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca.atan2(*cb)),
                    _ => Atan2(Box::new(sa), Box::new(sb)),
                }
            }
            Modulo(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) if cb.abs() > 1e-300 => Const(ca % cb),
                    _ => Modulo(Box::new(sa), Box::new(sb)),
                }
            }
            Min(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca.min(*cb)),
                    _ => Min(Box::new(sa), Box::new(sb)),
                }
            }
            Max(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(ca.max(*cb)),
                    _ => Max(Box::new(sa), Box::new(sb)),
                }
            }
            Clamp(x, lo, hi) => {
                let sx = x.simplify_once();
                let sl = lo.simplify_once();
                let sh = hi.simplify_once();
                match (&sx, &sl, &sh) {
                    (Const(cx), Const(cl), Const(ch)) => Const(cx.clamp(*cl, *ch)),
                    _ => Clamp(Box::new(sx), Box::new(sl), Box::new(sh)),
                }
            }
            Re(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c)
                } else {
                    Re(Box::new(sa))
                }
            }
            Im(_) => Const(0.0),
            Arg(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(if c >= 0.0 { 0.0 } else { std::f64::consts::PI })
                } else {
                    Arg(Box::new(sa))
                }
            }
            Conj(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(c)
                } else {
                    Conj(Box::new(sa))
                }
            }
            Erf(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(crate::special_functions::erf(c))
                } else {
                    Erf(Box::new(sa))
                }
            }
            Erfc(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(crate::special_functions::erfc(c))
                } else {
                    Erfc(Box::new(sa))
                }
            }
            Gamma(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(crate::special_functions::gamma(c))
                } else {
                    Gamma(Box::new(sa))
                }
            }
            LnGamma(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(crate::special_functions::ln_gamma(c))
                } else {
                    LnGamma(Box::new(sa))
                }
            }
            Digamma(a) => {
                let sa = a.simplify_once();
                if let Const(c) = sa {
                    Const(crate::special_functions::digamma(c))
                } else {
                    Digamma(Box::new(sa))
                }
            }
            Beta(a, b) => {
                let sa = a.simplify_once();
                let sb = b.simplify_once();
                match (&sa, &sb) {
                    (Const(ca), Const(cb)) => Const(crate::special_functions::beta(*ca, *cb)),
                    _ => Beta(Box::new(sa), Box::new(sb)),
                }
            }
            BesselJ(n, a) => {
                let sn = n.simplify_once();
                let sa = a.simplify_once();
                match (&sn, &sa) {
                    (Const(cn), Const(ca)) => {
                        Const(crate::special_functions::bessel_j(bessel_order(*cn), *ca))
                    }
                    _ => BesselJ(Box::new(sn), Box::new(sa)),
                }
            }
            BesselY(n, a) => {
                let sn = n.simplify_once();
                let sa = a.simplify_once();
                match (&sn, &sa) {
                    (Const(cn), Const(ca)) => {
                        Const(crate::special_functions::bessel_y(bessel_order(*cn), *ca))
                    }
                    _ => BesselY(Box::new(sn), Box::new(sa)),
                }
            }
            BesselI(n, a) => {
                let sn = n.simplify_once();
                let sa = a.simplify_once();
                match (&sn, &sa) {
                    (Const(cn), Const(ca)) => {
                        Const(crate::special_functions::bessel_i(bessel_order(*cn), *ca))
                    }
                    _ => BesselI(Box::new(sn), Box::new(sa)),
                }
            }
            Sum(body, v, start, end) => {
                // Try to expand if bounds are const
                let ss = start.simplify_once();
                let se = end.simplify_once();
                Sum(
                    Box::new(body.simplify_once()),
                    v.clone(),
                    Box::new(ss),
                    Box::new(se),
                )
            }
            Product(body, v, start, end) => {
                let ss = start.simplify_once();
                let se = end.simplify_once();
                Product(
                    Box::new(body.simplify_once()),
                    v.clone(),
                    Box::new(ss),
                    Box::new(se),
                )
            }
            Piecewise(pieces, default) => Piecewise(
                pieces
                    .iter()
                    .map(|(c, v)| (Box::new(c.simplify_once()), Box::new(v.simplify_once())))
                    .collect(),
                Box::new(default.simplify_once()),
            ),
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
                    format!("{:.6}", c)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                }
            }
            Var(v) => v.clone(),
            Neg(u) => format!("-({})", u.to_expr_string()),
            Add(a, b) => format!(
                "{} + {}",
                a.to_expr_string_paren(1),
                b.to_expr_string_paren(1)
            ),
            Sub(a, b) => format!(
                "{} - {}",
                a.to_expr_string_paren(1),
                b.to_expr_string_paren(2)
            ),
            Mul(a, b) => format!(
                "{} * {}",
                a.to_expr_string_paren(2),
                b.to_expr_string_paren(3)
            ),
            Div(a, b) => format!(
                "{} / {}",
                a.to_expr_string_paren(2),
                b.to_expr_string_paren(3)
            ),
            Pow(a, b) => format!(
                "{} ^ {}",
                a.to_expr_string_paren(3),
                b.to_expr_string_paren(4)
            ),
            Lt(a, b) => format!(
                "{} < {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
            Gt(a, b) => format!(
                "{} > {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
            Le(a, b) => format!(
                "{} <= {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
            Ge(a, b) => format!(
                "{} >= {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
            Eq(a, b) => format!(
                "{} == {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
            Ne(a, b) => format!(
                "{} != {}",
                a.to_expr_string_paren(0),
                b.to_expr_string_paren(0)
            ),
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
            Floor(u) => format!("floor({})", u.to_expr_string()),
            Ceil(u) => format!("ceil({})", u.to_expr_string()),
            Round(u) => format!("round({})", u.to_expr_string()),
            Sec(u) => format!("sec({})", u.to_expr_string()),
            Csc(u) => format!("csc({})", u.to_expr_string()),
            Cot(u) => format!("cot({})", u.to_expr_string()),
            Asinh(u) => format!("asinh({})", u.to_expr_string()),
            Acosh(u) => format!("acosh({})", u.to_expr_string()),
            Atanh(u) => format!("atanh({})", u.to_expr_string()),
            Sign(u) => format!("sign({})", u.to_expr_string()),
            Heaviside(u) => format!("heaviside({})", u.to_expr_string()),
            Cbrt(u) => format!("cbrt({})", u.to_expr_string()),
            Atan2(a, b) => format!("atan2({}, {})", a.to_expr_string(), b.to_expr_string()),
            Modulo(a, b) => format!("mod({}, {})", a.to_expr_string(), b.to_expr_string()),
            Min(a, b) => format!("min({}, {})", a.to_expr_string(), b.to_expr_string()),
            Max(a, b) => format!("max({}, {})", a.to_expr_string(), b.to_expr_string()),
            Clamp(x, lo, hi) => format!(
                "clamp({}, {}, {})",
                x.to_expr_string(),
                lo.to_expr_string(),
                hi.to_expr_string()
            ),
            Re(u) => format!("re({})", u.to_expr_string()),
            Im(u) => format!("im({})", u.to_expr_string()),
            Arg(u) => format!("arg({})", u.to_expr_string()),
            Conj(u) => format!("conj({})", u.to_expr_string()),
            Erf(u) => format!("erf({})", u.to_expr_string()),
            Erfc(u) => format!("erfc({})", u.to_expr_string()),
            Gamma(u) => format!("gamma({})", u.to_expr_string()),
            LnGamma(u) => format!("lngamma({})", u.to_expr_string()),
            Digamma(u) => format!("digamma({})", u.to_expr_string()),
            Beta(a, b) => format!("beta({}, {})", a.to_expr_string(), b.to_expr_string()),
            BesselJ(n, u) => format!("besselj({}, {})", n.to_expr_string(), u.to_expr_string()),
            BesselY(n, u) => format!("bessely({}, {})", n.to_expr_string(), u.to_expr_string()),
            BesselI(n, u) => format!("besseli({}, {})", n.to_expr_string(), u.to_expr_string()),
            Sum(body, v, start, end) => format!(
                "sum({}, {}, {}, {})",
                body.to_expr_string(),
                v,
                start.to_expr_string(),
                end.to_expr_string()
            ),
            Product(body, v, start, end) => format!(
                "product({}, {}, {}, {})",
                body.to_expr_string(),
                v,
                start.to_expr_string(),
                end.to_expr_string()
            ),
            Piecewise(pieces, default) => {
                let mut s = "piecewise(".to_string();
                for (i, (cond, val)) in pieces.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&format!(
                        "{} if {}",
                        val.to_expr_string(),
                        cond.to_expr_string()
                    ));
                }
                s.push_str(&format!(", {})", default.to_expr_string()));
                s
            }
        }
    }

    /// Helper: wrap in parentheses if operator priority is lower than `min_prec`.
    fn to_expr_string_paren(&self, min_prec: u8) -> String {
        use Expr::*;
        let prec = match self {
            Const(_) | Var(_) => 10u8,
            Sin(_)
            | Cos(_)
            | Tan(_)
            | Asin(_)
            | Acos(_)
            | Atan(_)
            | Exp(_)
            | Ln(_)
            | Log(_)
            | Sqrt(_)
            | Abs(_)
            | Sinh(_)
            | Cosh(_)
            | Tanh(_)
            | Floor(_)
            | Ceil(_)
            | Round(_)
            | Sec(_)
            | Csc(_)
            | Cot(_)
            | Asinh(_)
            | Acosh(_)
            | Atanh(_)
            | Sign(_)
            | Heaviside(_)
            | Cbrt(_)
            | Re(_)
            | Im(_)
            | Arg(_)
            | Conj(_)
            | Erf(_)
            | Erfc(_)
            | Gamma(_)
            | LnGamma(_)
            | Digamma(_)
            | Atan2(_, _)
            | Modulo(_, _)
            | Min(_, _)
            | Max(_, _)
            | Clamp(_, _, _)
            | Beta(_, _)
            | BesselJ(_, _)
            | BesselY(_, _)
            | BesselI(_, _)
            | Sum(_, _, _, _)
            | Product(_, _, _, _)
            | Piecewise(_, _) => 10,
            Pow(_, _) => 4,
            Mul(_, _) | Div(_, _) => 2,
            Add(_, _) | Sub(_, _) => 1,
            Lt(_, _) | Gt(_, _) | Le(_, _) | Ge(_, _) | Eq(_, _) | Ne(_, _) => 0,
            Neg(_) => 3,
        };
        if prec < min_prec {
            format!("({})", self.to_expr_string())
        } else {
            self.to_expr_string()
        }
    }

    pub fn integrate(&self, var: &str) -> Option<Expr> {
        use Expr::*;
        Some(
            match self {
                Const(c) => {
                    if *c == 0.0 {
                        Const(0.0)
                    } else {
                        Mul(Box::new(Const(*c)), Box::new(Var(var.to_string())))
                    }
                }
                Var(v) if v == var => Mul(
                    Box::new(Pow(Box::new(Var(var.to_string())), Box::new(Const(2.0)))),
                    Box::new(Const(0.5)),
                ),
                Neg(a) => Neg(Box::new(a.integrate(var)?)),
                Add(a, b) => Add(Box::new(a.integrate(var)?), Box::new(b.integrate(var)?)),
                Sub(a, b) => Sub(Box::new(a.integrate(var)?), Box::new(b.integrate(var)?)),
                Mul(a, b) => {
                    let a_free = !a.contains_var(var);
                    let b_free = !b.contains_var(var);
                    if a_free {
                        Mul(a.clone(), Box::new(b.integrate(var)?))
                    } else if b_free {
                        Mul(Box::new(a.integrate(var)?), b.clone())
                    } else if let Pow(base, exp) = a.as_ref() {
                        if let Var(v) = base.as_ref() {
                            if v == var {
                                if let Const(n) = exp.as_ref() {
                                    if (*n + 1.0).abs() > 1e-12 {
                                        let new_exp = n + 1.0;
                                        let factor = 1.0 / new_exp;
                                        return Some(Mul(
                                            Box::new(Const(factor)),
                                            Box::new(Pow(
                                                Box::new(Var(var.to_string())),
                                                Box::new(Const(new_exp)),
                                            )),
                                        ));
                                    }
                                }
                            }
                        }
                        integrate_parts(self, var)?
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Pow(base, exp) => {
                    if let Var(v) = base.as_ref() {
                        if v == var {
                            if let Const(n) = exp.as_ref() {
                                if (*n + 1.0).abs() < 1e-12 {
                                    Ln(Box::new(Abs(Box::new(Var(var.to_string())))))
                                } else {
                                    let new_exp = n + 1.0;
                                    let factor = 1.0 / new_exp;
                                    Mul(
                                        Box::new(Const(factor)),
                                        Box::new(Pow(
                                            Box::new(Var(var.to_string())),
                                            Box::new(Const(new_exp)),
                                        )),
                                    )
                                }
                            } else {
                                integrate_parts(self, var)?
                            }
                        } else if let Const(_) = exp.as_ref() {
                            Mul(base.clone(), Box::new(self.integrate(var)?))
                        } else {
                            integrate_parts(self, var)?
                        }
                    } else if let Const(c) = exp.as_ref() {
                        if *c == 0.0 {
                            Var(var.to_string())
                        } else if base.contains_var(var) {
                            integrate_parts(self, var)?
                        } else {
                            Mul(
                                Box::new(Pow(base.clone(), exp.clone())),
                                Box::new(Var(var.to_string())),
                            )
                        }
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Div(num, den) => {
                    if let Var(v) = den.as_ref() {
                        if v == var {
                            if let Const(_) = num.as_ref() {
                                Mul(
                                    Box::new(num.as_ref().clone()),
                                    Box::new(Ln(Box::new(Abs(Box::new(Var(var.to_string())))))),
                                )
                            } else if let Const(c) = num.as_ref() {
                                if *c == 1.0 {
                                    Ln(Box::new(Abs(Box::new(Var(var.to_string())))))
                                } else {
                                    Mul(
                                        Box::new(Const(*c)),
                                        Box::new(Ln(Box::new(Abs(Box::new(Var(var.to_string())))))),
                                    )
                                }
                            } else {
                                integrate_parts(self, var)?
                            }
                        } else {
                            integrate_parts(self, var)?
                        }
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Sin(arg) => {
                    if let Mul(coeff, inner) = arg.as_ref() {
                        if let (Const(c), Var(v)) = (coeff.as_ref(), inner.as_ref()) {
                            if v == var {
                                return Some(Mul(
                                    Box::new(Const(-1.0 / c)),
                                    Box::new(Cos(arg.clone())),
                                ));
                            }
                        }
                    }
                    if arg.is_linear_in(var) {
                        let (a, _) = arg.linear_coeff(var);
                        Mul(Box::new(Const(-1.0 / a)), Box::new(Cos(arg.clone())))
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Cos(arg) => {
                    if let Mul(coeff, inner) = arg.as_ref() {
                        if let (Const(c), Var(v)) = (coeff.as_ref(), inner.as_ref()) {
                            if v == var {
                                return Some(Mul(
                                    Box::new(Const(1.0 / c)),
                                    Box::new(Sin(arg.clone())),
                                ));
                            }
                        }
                    }
                    if arg.is_linear_in(var) {
                        let (a, _) = arg.linear_coeff(var);
                        Mul(Box::new(Const(1.0 / a)), Box::new(Sin(arg.clone())))
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Tan(arg) => {
                    if arg.is_linear_in(var) {
                        let (a, _) = arg.linear_coeff(var);
                        Mul(
                            Box::new(Const(-1.0 / a)),
                            Box::new(Ln(Box::new(Abs(Box::new(Cos(arg.clone())))))),
                        )
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Exp(arg) => {
                    if let Var(v) = arg.as_ref() {
                        if v == var {
                            return Some(Exp(Box::new(Var(var.to_string()))));
                        }
                    }
                    if arg.is_linear_in(var) {
                        let (a, _) = arg.linear_coeff(var);
                        if (a - 1.0).abs() < 1e-12 {
                            Exp(arg.clone())
                        } else {
                            Mul(Box::new(Const(1.0 / a)), Box::new(Exp(arg.clone())))
                        }
                    } else {
                        integrate_parts(self, var)?
                    }
                }
                Ln(arg) => {
                    if let Var(v) = arg.as_ref() {
                        if v == var {
                            return Some(Sub(
                                Box::new(Mul(
                                    Box::new(Var(var.to_string())),
                                    Box::new(Ln(Box::new(Var(var.to_string())))),
                                )),
                                Box::new(Var(var.to_string())),
                            ));
                        }
                    }
                    integrate_parts(self, var)?
                }
                _ => integrate_parts(self, var)?,
            }
            .simplify(),
        )
    }

    fn contains_var(&self, var: &str) -> bool {
        use Expr::*;
        match self {
            Var(v) => v == var,
            Const(_) => false,
            Neg(a) | Sin(a) | Cos(a) | Tan(a) | Asin(a) | Acos(a) | Atan(a) | Exp(a) | Ln(a)
            | Log(a) | Sqrt(a) | Abs(a) | Sinh(a) | Cosh(a) | Tanh(a) | Asinh(a) | Acosh(a)
            | Atanh(a) | Sec(a) | Csc(a) | Cot(a) | Floor(a) | Ceil(a) | Round(a) | Sign(a)
            | Heaviside(a) | Cbrt(a) | Re(a) | Im(a) | Arg(a) | Conj(a) | Erf(a) | Erfc(a)
            | Gamma(a) | LnGamma(a) | Digamma(a) => a.contains_var(var),
            Add(a, b)
            | Sub(a, b)
            | Mul(a, b)
            | Div(a, b)
            | Pow(a, b)
            | Atan2(a, b)
            | Modulo(a, b)
            | Min(a, b)
            | Max(a, b)
            | Beta(a, b)
            | BesselJ(a, b)
            | BesselY(a, b)
            | BesselI(a, b)
            | Lt(a, b)
            | Gt(a, b)
            | Le(a, b)
            | Ge(a, b)
            | Eq(a, b)
            | Ne(a, b) => a.contains_var(var) || b.contains_var(var),
            Clamp(a, b, c) => a.contains_var(var) || b.contains_var(var) || c.contains_var(var),
            Sum(body, _, _, _) | Product(body, _, _, _) => body.contains_var(var),
            Piecewise(cases, default) => {
                cases
                    .iter()
                    .any(|(c, v)| c.contains_var(var) || v.contains_var(var))
                    || default.contains_var(var)
            }
        }
    }

    fn is_linear_in(&self, var: &str) -> bool {
        use Expr::*;
        match self {
            Var(v) => v == var,
            Const(_) => true,
            Neg(a) => a.is_linear_in(var),
            Add(a, b) | Sub(a, b) => a.is_linear_in(var) && b.is_linear_in(var),
            Mul(a, b) => {
                if let Const(_) = a.as_ref() {
                    b.is_linear_in(var)
                } else if let Const(_) = b.as_ref() {
                    a.is_linear_in(var)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn linear_coeff(&self, var: &str) -> (f64, f64) {
        use Expr::*;
        match self {
            Var(v) if v == var => (1.0, 0.0),
            Const(c) => (0.0, *c),
            Neg(a) => {
                let (a_coeff, b_coeff) = a.linear_coeff(var);
                (-a_coeff, -b_coeff)
            }
            Add(a, b) => {
                let (a1, b1) = a.linear_coeff(var);
                let (a2, b2) = b.linear_coeff(var);
                (a1 + a2, b1 + b2)
            }
            Sub(a, b) => {
                let (a1, b1) = a.linear_coeff(var);
                let (a2, b2) = b.linear_coeff(var);
                (a1 - a2, b1 - b2)
            }
            Mul(a, b) => {
                if let Const(c) = a.as_ref() {
                    let (a2, b2) = b.linear_coeff(var);
                    (c * a2, c * b2)
                } else if let Const(c) = b.as_ref() {
                    let (a1, b1) = a.linear_coeff(var);
                    (c * a1, c * b1)
                } else {
                    (0.0, 0.0)
                }
            }
            _ => (0.0, 0.0),
        }
    }
}

fn integrate_parts(expr: &Expr, var: &str) -> Option<Expr> {
    use Expr::*;
    match expr {
        Mul(a, b) => {
            if let Ln(inner) = a.as_ref() {
                if let (Var(v), Pow(base, exp)) = (inner.as_ref(), b.as_ref()) {
                    if v == var {
                        if let Var(bv) = base.as_ref() {
                            if bv == var {
                                if let Const(n) = exp.as_ref() {
                                    if (*n + 1.0).abs() > 1e-12 {
                                        let np1 = n + 1.0;
                                        let x_np1 = Pow(
                                            Box::new(Var(var.to_string())),
                                            Box::new(Const(np1)),
                                        );
                                        let term1 = Mul(
                                            Box::new(Ln(Box::new(Var(var.to_string())))),
                                            Box::new(Mul(
                                                Box::new(Const(1.0 / np1)),
                                                Box::new(x_np1.clone()),
                                            )),
                                        );
                                        let term2 = Mul(
                                            Box::new(Const(1.0 / (np1 * np1))),
                                            Box::new(x_np1),
                                        );
                                        return Some(Sub(Box::new(term1), Box::new(term2)));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Pow(base, _) = a.as_ref() {
                if let Var(v) = base.as_ref() {
                    if v == var && !b.contains_var(var) {
                        return Some(Mul(Box::new(a.integrate(var)?), b.clone()));
                    }
                }
            }
            if let Pow(base, _) = b.as_ref() {
                if let Var(v) = base.as_ref() {
                    if v == var && !a.contains_var(var) {
                        return Some(Mul(a.clone(), Box::new(b.integrate(var)?)));
                    }
                }
            }
            None
        }
        _ => None,
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

const MAX_AST_DEPTH: usize = 256;

pub fn parse_ast(expr: &str) -> Result<Expr, String> {
    // Preprocess: replace common math notations
    let expr = preprocess(expr);
    let mut tokens = tokenize(&expr);
    let result = parse_cmp(&mut tokens, 0)?;
    if !tokens.is_empty() {
        return Err(format!("Unexpected tokens remaining: {:?}", tokens));
    }
    Ok(result)
}

fn check_depth(depth: usize) -> Result<(), String> {
    if depth > MAX_AST_DEPTH {
        Err("Expression is too deeply nested".to_string())
    } else {
        Ok(())
    }
}

fn preprocess(expr: &str) -> String {
    let expr = expr.trim().to_string();
    // Replace π with pi literal value
    let expr = expr.replace("π", "3.141592653589793");
    let expr = replace_standalone(&expr, "pi", "3.141592653589793");
    let expr = replace_standalone(&expr, "tau", "6.283185307179586");
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
            let next_char = if after < expr.len() {
                expr[after..].chars().next()
            } else {
                None
            };

            let prev_is_ident = prev_char
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
            let next_is_ident = next_char
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);

            if !prev_is_ident && !next_is_ident {
                result.push_str(replacement);
                let pattern_char_count = pattern.chars().count();
                // Skip remaining pattern chars (first char was already consumed by outer loop)
                for _ in 1..pattern_char_count {
                    if let Some(skipped) = chars.next() {
                        byte_offset += skipped.len_utf8();
                    }
                }
                // Account for the first pattern char consumed by the outer while loop
                byte_offset += c_byte_len;
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
        if "+-*/^(),<>=!".contains(c) {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(c.to_string());
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            // If we're mid-number (pure digits), push number first
            if !current.is_empty() && !current.chars().any(|ch| ch.is_alphabetic() || ch == '_') {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(c);
            i += 1;
        } else if c.is_numeric() || c == '.' {
            // If we're mid-identifier (contains letters), stay in same token
            // Only break if current is empty or we're not in an identifier
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
    // Combine two-character operators: <=, >=, ==, !=
    // Also discard standalone "=" tokens (assignment, not a valid expression operator).
    let mut combined = Vec::with_capacity(tokens.len());
    let mut j = 0;
    while j < tokens.len() {
        if j + 1 < tokens.len() {
            let pair = (tokens[j].as_str(), tokens[j + 1].as_str());
            match pair {
                ("<", "=") | (">", "=") | ("=", "=") | ("!", "=") => {
                    combined.push(format!("{}{}", pair.0, pair.1));
                    j += 2;
                    continue;
                }
                _ => {}
            }
        }
        // Discard standalone "=" — it's an assignment operator, not a valid expression token
        if tokens[j] != "=" {
            combined.push(tokens[j].clone());
        }
        j += 1;
    }
    combined
}

fn parse_cmp(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    check_depth(depth)?;
    let mut lhs = parse_add_sub(tokens, depth + 1)?;
    while !tokens.is_empty() {
        let op = match tokens[0].as_str() {
            "<" => Expr::Lt,
            ">" => Expr::Gt,
            "<=" => Expr::Le,
            ">=" => Expr::Ge,
            "==" => Expr::Eq,
            "!=" => Expr::Ne,
            _ => break,
        };
        tokens.remove(0);
        let rhs = parse_add_sub(tokens, depth + 1)?;
        lhs = op(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_add_sub(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    check_depth(depth)?;
    let mut lhs = parse_mul_div(tokens, depth + 1)?;
    while !tokens.is_empty() {
        match tokens[0].as_str() {
            "+" => {
                tokens.remove(0);
                let rhs = parse_mul_div(tokens, depth + 1)?;
                lhs = Expr::Add(Box::new(lhs), Box::new(rhs));
            }
            "-" => {
                tokens.remove(0);
                let rhs = parse_mul_div(tokens, depth + 1)?;
                lhs = Expr::Sub(Box::new(lhs), Box::new(rhs));
            }
            _ => break,
        }
    }
    Ok(lhs)
}

fn parse_mul_div(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    check_depth(depth)?;
    let mut lhs = parse_unary(tokens, depth + 1)?;
    while !tokens.is_empty() {
        match tokens[0].as_str() {
            "*" => {
                tokens.remove(0);
                let rhs = parse_unary(tokens, depth + 1)?;
                lhs = Expr::Mul(Box::new(lhs), Box::new(rhs));
            }
            "/" => {
                tokens.remove(0);
                let rhs = parse_unary(tokens, depth + 1)?;
                lhs = Expr::Div(Box::new(lhs), Box::new(rhs));
            }
            _ => break,
        }
    }
    Ok(lhs)
}

fn parse_unary(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    check_depth(depth)?;
    if !tokens.is_empty() && tokens[0] == "-" {
        tokens.remove(0);
        let inner = parse_pow(tokens, depth + 1)?;
        return Ok(Expr::Neg(Box::new(inner)));
    }
    if !tokens.is_empty() && tokens[0] == "+" {
        tokens.remove(0);
    }
    parse_pow(tokens, depth + 1)
}

fn parse_pow(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    check_depth(depth)?;
    let base = parse_primary(tokens, depth + 1)?;
    if !tokens.is_empty() && tokens[0] == "^" {
        tokens.remove(0);
        // Right-associative
        let exp = parse_unary(tokens, depth + 1)?;
        return Ok(Expr::Pow(Box::new(base), Box::new(exp)));
    }
    Ok(base)
}

fn parse_primary(tokens: &mut Vec<String>, depth: usize) -> Result<Expr, String> {
    if tokens.is_empty() {
        return Err("Unexpected end of expression".into());
    }
    let token = tokens.remove(0);
    // Parenthesized expression
    if token == "(" {
        let inner = parse_cmp(tokens, depth + 1)?;
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
            let mut args = vec![parse_cmp(tokens, depth + 1)?];
            while !tokens.is_empty() && tokens[0] == "," {
                tokens.remove(0);
                args.push(parse_cmp(tokens, depth + 1)?);
            }
            if tokens.is_empty() || tokens[0] != ")" {
                return Err(format!(
                    "Missing closing parenthesis for function '{}'",
                    token
                ));
            }
            tokens.remove(0);
            return Ok(match token.to_lowercase().as_str() {
                // Trig
                "sin" => Expr::Sin(Box::new(args.remove(0))),
                "cos" => Expr::Cos(Box::new(args.remove(0))),
                "tan" => Expr::Tan(Box::new(args.remove(0))),
                "asin" | "arcsin" => Expr::Asin(Box::new(args.remove(0))),
                "acos" | "arccos" => Expr::Acos(Box::new(args.remove(0))),
                "atan" | "arctan" => Expr::Atan(Box::new(args.remove(0))),
                // Hyperbolic
                "sinh" => Expr::Sinh(Box::new(args.remove(0))),
                "cosh" => Expr::Cosh(Box::new(args.remove(0))),
                "tanh" => Expr::Tanh(Box::new(args.remove(0))),
                // Inverse hyperbolic
                "asinh" | "arcsinh" => Expr::Asinh(Box::new(args.remove(0))),
                "acosh" | "arccosh" => Expr::Acosh(Box::new(args.remove(0))),
                "atanh" | "arctanh" => Expr::Atanh(Box::new(args.remove(0))),
                // Reciprocal trig
                "sec" => Expr::Sec(Box::new(args.remove(0))),
                "csc" | "cosec" => Expr::Csc(Box::new(args.remove(0))),
                "cot" | "cotan" => Expr::Cot(Box::new(args.remove(0))),
                "asec" | "arcsec" => Expr::Acos(Box::new(Expr::Div(
                    Box::new(Expr::Const(1.0)),
                    Box::new(args.remove(0)),
                ))),
                "acsc" | "arccsc" => Expr::Asin(Box::new(Expr::Div(
                    Box::new(Expr::Const(1.0)),
                    Box::new(args.remove(0)),
                ))),
                "acot" | "arccot" => Expr::Atan(Box::new(Expr::Div(
                    Box::new(Expr::Const(1.0)),
                    Box::new(args.remove(0)),
                ))),
                // Exp/Log
                "exp" => Expr::Exp(Box::new(args.remove(0))),
                "ln" => Expr::Ln(Box::new(args.remove(0))),
                "log" | "log10" => Expr::Log(Box::new(args.remove(0))),
                "log2" => Expr::Div(
                    Box::new(Expr::Ln(Box::new(args.remove(0)))),
                    Box::new(Expr::Ln(Box::new(Expr::Const(2.0)))),
                ),
                // Roots/Powers
                "sqrt" => Expr::Sqrt(Box::new(args.remove(0))),
                "cbrt" => Expr::Cbrt(Box::new(args.remove(0))),
                // Absolute/Sign
                "abs" => Expr::Abs(Box::new(args.remove(0))),
                "sign" | "signum" => Expr::Sign(Box::new(args.remove(0))),
                "heaviside" | "step" => Expr::Heaviside(Box::new(args.remove(0))),
                // Rounding
                "floor" => Expr::Floor(Box::new(args.remove(0))),
                "ceil" | "ceiling" => Expr::Ceil(Box::new(args.remove(0))),
                "round" => Expr::Round(Box::new(args.remove(0))),
                // Two-arg
                "atan2" => {
                    if args.len() < 2 {
                        return Err("atan2 requires 2 arguments".into());
                    }
                    Expr::Atan2(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "mod" | "modulo" => {
                    if args.len() < 2 {
                        return Err("mod requires 2 arguments".into());
                    }
                    Expr::Modulo(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "min" => {
                    if args.len() < 2 {
                        return Err("min requires 2 arguments".into());
                    }
                    Expr::Min(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "max" => {
                    if args.len() < 2 {
                        return Err("max requires 2 arguments".into());
                    }
                    Expr::Max(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "clamp" => {
                    if args.len() < 3 {
                        return Err("clamp requires 3 arguments".into());
                    }
                    Expr::Clamp(
                        Box::new(args.remove(0)),
                        Box::new(args.remove(0)),
                        Box::new(args.remove(0)),
                    )
                }
                // Complex
                "re" | "real" => Expr::Re(Box::new(args.remove(0))),
                "im" | "imag" | "imaginary" => Expr::Im(Box::new(args.remove(0))),
                "arg" | "argument" | "phase" => Expr::Arg(Box::new(args.remove(0))),
                "conj" | "conjugate" => Expr::Conj(Box::new(args.remove(0))),
                // Special functions (1-arg)
                "erf" => Expr::Erf(Box::new(args.remove(0))),
                "erfc" => Expr::Erfc(Box::new(args.remove(0))),
                "gamma" => Expr::Gamma(Box::new(args.remove(0))),
                "lngamma" | "lgamma" => Expr::LnGamma(Box::new(args.remove(0))),
                "digamma" => Expr::Digamma(Box::new(args.remove(0))),
                // Special functions (2-arg)
                "beta" => {
                    if args.len() < 2 {
                        return Err("beta requires 2 arguments".into());
                    }
                    Expr::Beta(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "besselj" => {
                    if args.len() < 2 {
                        return Err("besselj requires 2 arguments".into());
                    }
                    Expr::BesselJ(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "bessely" => {
                    if args.len() < 2 {
                        return Err("bessely requires 2 arguments".into());
                    }
                    Expr::BesselY(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                "besseli" => {
                    if args.len() < 2 {
                        return Err("besseli requires 2 arguments".into());
                    }
                    Expr::BesselI(Box::new(args.remove(0)), Box::new(args.remove(0)))
                }
                // Sum/Product
                "sum" => {
                    if args.len() < 4 {
                        return Err("sum requires 4 arguments: sum(expr, var, start, end)".into());
                    }
                    let body = args.remove(0);
                    let var = if let Expr::Var(v) = args.remove(0) {
                        v
                    } else {
                        return Err("sum variable must be a name".into());
                    };
                    Expr::Sum(
                        Box::new(body),
                        var,
                        Box::new(args.remove(0)),
                        Box::new(args.remove(0)),
                    )
                }
                "product" | "prod" => {
                    if args.len() < 4 {
                        return Err("product requires 4 arguments".into());
                    }
                    let body = args.remove(0);
                    let var = if let Expr::Var(v) = args.remove(0) {
                        v
                    } else {
                        return Err("product variable must be a name".into());
                    };
                    Expr::Product(
                        Box::new(body),
                        var,
                        Box::new(args.remove(0)),
                        Box::new(args.remove(0)),
                    )
                }
                "piecewise" => {
                    // piecewise(cond1, val1, cond2, val2, ..., default)
                    if args.is_empty() {
                        return Err("piecewise requires at least 1 argument".into());
                    }
                    let mut pieces = Vec::new();
                    while args.len() >= 2 {
                        let cond = args.remove(0);
                        let val = args.remove(0);
                        pieces.push((Box::new(cond), Box::new(val)));
                    }
                    let default = if args.is_empty() {
                        Expr::Const(0.0)
                    } else {
                        args.remove(0)
                    };
                    Expr::Piecewise(pieces, Box::new(default))
                }
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
    let nodes = [
        -0.906179845938664,
        -0.538469310105683,
        0.0,
        0.538469310105683,
        0.906179845938664,
    ];
    let weights = [
        0.236926885056189,
        0.478628670499366,
        0.568888888888889,
        0.478628670499366,
        0.236926885056189,
    ];

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
        if left.is_finite() {
            left_vals.push(left);
        }
        if right.is_finite() {
            right_vals.push(right);
        }
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
        assert!(
            (val2 - 1.0).abs() < 1e-6,
            "Expected 1 at pi/2, got {}",
            val2
        );
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
        assert!(
            (result.unwrap() - 1.0).abs() < 1e-4,
            "Expected 1, got {:?}",
            result
        );
    }

    #[test]
    fn test_symbolic_integrate_power() {
        let expr = parse_ast("x^3").unwrap();
        let integrated = expr.integrate("x").unwrap();
        let s = integrated.to_expr_string();
        assert!(s.contains("x ^ 4") || s.contains("x^4"), "Got: {}", s);
    }

    #[test]
    fn test_symbolic_integrate_sin() {
        let expr = parse_ast("sin(x)").unwrap();
        let integrated = expr.integrate("x").unwrap();
        let s = integrated.to_expr_string();
        assert!(s.contains("cos") || s.contains("Cos"), "Got: {}", s);
    }

    #[test]
    fn test_symbolic_integrate_cos() {
        let expr = parse_ast("cos(x)").unwrap();
        let integrated = expr.integrate("x").unwrap();
        let s = integrated.to_expr_string();
        assert!(s.contains("sin") || s.contains("Sin"), "Got: {}", s);
    }

    #[test]
    fn test_symbolic_integrate_exp() {
        let expr = parse_ast("exp(x)").unwrap();
        let integrated = expr.integrate("x").unwrap();
        let s = integrated.to_expr_string();
        assert!(s.contains("exp") || s.contains("Exp"), "Got: {}", s);
    }

    #[test]
    fn test_symbolic_integrate_linear() {
        let expr = parse_ast("2*x + 1").unwrap();
        let integrated = expr.integrate("x").unwrap();
        let s = integrated.to_expr_string();
        assert!(s.contains("x ^ 2") || s.contains("x^2"), "Got: {}", s);
    }

    #[test]
    fn test_trig_simplify_pythagorean() {
        let expr = parse_ast("sin(x)^2 + cos(x)^2").unwrap();
        let simplified = expr.simplify();
        assert_eq!(simplified.to_expr_string(), "1");
    }

    #[test]
    fn test_comparison_lt_gt() {
        let lt = parse_ast("x<0").unwrap();
        assert_eq!(lt.eval_at("x", -1.0), 1.0);
        assert_eq!(lt.eval_at("x", 0.0), 0.0);
        assert_eq!(lt.eval_at("x", 1.0), 0.0);

        let gt = parse_ast("x>0").unwrap();
        assert_eq!(gt.eval_at("x", -1.0), 0.0);
        assert_eq!(gt.eval_at("x", 1.0), 1.0);

        let le = parse_ast("x<=0").unwrap();
        assert_eq!(le.eval_at("x", 0.0), 1.0);
        assert_eq!(le.eval_at("x", 0.001), 0.0);

        let ge = parse_ast("x>=0").unwrap();
        assert_eq!(ge.eval_at("x", 0.0), 1.0);
        assert_eq!(ge.eval_at("x", -0.001), 0.0);

        let eq = parse_ast("x==2").unwrap();
        assert_eq!(eq.eval_at("x", 2.0), 1.0);
        assert_eq!(eq.eval_at("x", 2.1), 0.0);

        let ne = parse_ast("x!=2").unwrap();
        assert_eq!(ne.eval_at("x", 2.0), 0.0);
        assert_eq!(ne.eval_at("x", 2.1), 1.0);
    }

    #[test]
    fn test_piecewise_with_comparisons() {
        let pw = parse_ast("piecewise(x<0, x^2, x>=0, sqrt(x))").unwrap();
        assert!((pw.eval_at("x", -1.0) - 1.0).abs() < 1e-9);
        assert!((pw.eval_at("x", 0.0) - 0.0).abs() < 1e-9);
        assert!((pw.eval_at("x", 1.0) - 1.0).abs() < 1e-9);
        assert!((pw.eval_at("x", 4.0) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_standalone_eq_discarded() {
        // Verify that standalone "=" is discarded during tokenization
        // and == is correctly combined
        let t1 = tokenize("x = 5");
        let t2 = tokenize("x == 5");
        let t3 = tokenize("x <= 5");
        // Standalone = should be filtered out: ["x", "5"]
        assert_eq!(t1, vec!["x", "5"], "standalone = should be discarded");
        // == should be combined: ["x", "==", "5"]
        assert_eq!(t2, vec!["x", "==", "5"], "== should be preserved");
        // <= should be preserved: ["x", "<=", "5"]
        assert_eq!(t3, vec!["x", "<=", "5"], "<= should be preserved");
    }
}
