import re

with open('crates/grafito-geometry/src/ast.rs', 'r') as f:
    text = f.read()

new_methods = """
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
"""

text = text.replace("    pub fn eval_at(&self, var: &str, value: f64) -> f64 {", new_methods + "\n    pub fn eval_at(&self, var: &str, value: f64) -> f64 {")

with open('crates/grafito-geometry/src/ast.rs', 'w') as f:
    f.write(text)

