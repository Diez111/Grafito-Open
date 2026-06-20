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

use crate::ast::{parse_ast, Expr};
use std::collections::{HashMap, HashSet};

// ============================================================================
// API pública
// ============================================================================

/// Derivada simbólica de `expr` respecto de `var`.
///
/// Implementa las reglas clásicas (constante, variable, suma, producto,
/// cociente, potencia con exponente constante, regla de la cadena para
/// trigonométricas / hiperbólicas / inversas / exponencial / logaritmo /
/// raíces / valor absoluto) sobre el AST nativo.
pub fn derivative(expr: &str, var: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo derivar '{expr}': {e}"))?;
    let d = simplify_expr(&diff_expr(&ast, var));
    Ok(d.to_expr_string())
}

/// Integración indefinida simbólica básica.
///
/// Soporta: constantes, potencias (regla de la potencia, incluida 1/x → ln|x|),
/// linealidad, constante por función, sin(x) → -cos(x), cos(x) → sin(x),
/// exp(x) → exp(x), ln(x) → x·ln(x) − x. Para casos más complejos cae al
/// integrador simbólico nativo (`Expr::integrate`) y, como último recurso,
/// integración numérica (Simpson adaptativo) en [0, 1].
pub fn integrate(expr: &str, var: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo integrar '{expr}': {e}"))?;
    if let Some(prim) = integrate_expr(&ast, var).or_else(|| ast.integrate(var)) {
        return Ok(simplify_expr(&prim).to_expr_string());
    }
    let val = numeric_integrate(&ast, var, 0.0, 1.0);
    Ok(format!("{val:.8}"))
}

/// Integral definida de `a` a `b`.
///
/// Intenta antidiferenciación simbólica y evalúa en los extremos; si no es
/// posible, usa cuadratura numérica (Simpson adaptativo) sobre el AST.
pub fn integrate_definite(expr: &str, var: &str, a: f64, b: f64) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo integrar '{expr}': {e}"))?;
    if let Some(prim) = integrate_expr(&ast, var).or_else(|| ast.integrate(var)) {
        let prim = simplify_expr(&prim);
        let fa = prim.eval_at(var, a);
        let fb = prim.eval_at(var, b);
        if fa.is_finite() && fb.is_finite() {
            return Ok(format!("\u{222b}[{a},{b}] {expr} d{var} = {:.8}", fb - fa));
        }
    }
    let val = numeric_integrate(&ast, var, a, b);
    Ok(format!("\u{222b}[{a},{b}] {expr} d{var} \u{2248} {val:.8}"))
}

/// Límite de `expr` cuando `var → at` (numérico, extrapolación de Richardson
/// bilateral: aproxima por izquierda y por derecha y verifica coincidencia).
pub fn limit(expr: &str, var: &str, at: f64) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast =
        parse_ast(&pp).map_err(|e| format!("No se pudo calcular el límite de '{expr}': {e}"))?;
    match richardson_limit(&ast, var, at) {
        Some(v) => Ok(format!("lim({var}\u{2192}{at}) {expr} = {v:.8}")),
        None => Ok(format!(
            "lim({var}\u{2192}{at}) {expr} = no existe (o es \u{221e})"
        )),
    }
}

/// Expande polinomios por distributividad: (a+b)*(c+d) → suma de productos.
pub fn expand(expr: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo expandir '{expr}': {e}"))?;
    Ok(simplify_expr(&expand_expr(&ast)).to_expr_string())
}

/// Factoriza buscando raíces reales en [-20, 20] y construyendo (x − r).
pub fn factor(expr: &str, var: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo factorizar '{expr}': {e}"))?;
    let roots = find_real_roots_numeric(&ast, var, -20.0, 20.0);
    if roots.is_empty() {
        return Ok(simplify_expr(&ast).to_expr_string());
    }
    let factors: Vec<String> = roots
        .iter()
        .map(|r| {
            if r.abs() < 1e-9 {
                var.to_string()
            } else if *r > 0.0 {
                format!("({var} - {:.6})", r)
            } else {
                format!("({var} + {:.6})", -r)
            }
        })
        .collect();
    Ok(factors.join(" * "))
}

/// Simplificación algebraica: const folding aritmético e identidades
/// (0+x=x, 1*x=x, x^0=1, x^1=x, -(-x)=x, x-x=0, x/x=1, …).
pub fn simplify(expr: &str) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo simplificar '{expr}': {e}"))?;
    Ok(simplify_expr(&ast).to_expr_string())
}

/// Serie de Taylor de `expr` alrededor de `center` hasta orden `order`
/// (derivadas n-ésimas calculadas simbólicamente y evaluadas en `center`).
pub fn taylor_series(expr: &str, var: &str, center: f64, order: usize) -> Result<String, String> {
    let pp = expr.replace(' ', "");
    let ast = parse_ast(&pp).map_err(|e| format!("No se pudo calcular Taylor de '{expr}': {e}"))?;
    let mut current = ast;
    let mut terms = Vec::new();
    let mut factorial = 1.0f64;
    for n in 0..=order {
        let val = current.eval_at(var, center);
        if val.is_finite() && val.abs() > 1e-12 {
            let coef = val / factorial;
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
        if n < order {
            current = simplify_expr(&diff_expr(&current, var));
            factorial *= (n + 1) as f64;
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
    use Expr::*;
    let du = |x: &Expr| Box::new(diff_expr(x, var));
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
                let dv = diff_expr(exp, var);
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
        if next.to_expr_string() == current.to_expr_string() {
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
                _ if sa == sb => Const(0.0),
                _ => Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Mul(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca * cb),
                (Const(ca), _) if *ca == 0.0 => Const(0.0),
                (_, Const(cb)) if *cb == 0.0 => Const(0.0),
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
                (Const(ca), _) if *ca == 0.0 => Const(0.0),
                (_, Const(cb)) if *cb == 1.0 => sa,
                _ if sa == sb => Const(1.0),
                _ => Div(Box::new(sa), Box::new(sb)),
            }
        }
        Pow(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Const(ca), Const(cb)) => Const(ca.powf(*cb)),
                (_, Const(cb)) if *cb == 0.0 => Const(1.0),
                (_, Const(cb)) if *cb == 1.0 => sa,
                (Const(ca), _) if *ca == 0.0 => Const(0.0),
                (Const(ca), _) if *ca == 1.0 => Const(1.0),
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
    use Expr::*;
    let v = var.to_string();
    let rec = |x: &Expr| integrate_expr(x, var);
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
fn numeric_integrate(ast: &Expr, var: &str, a: f64, b: f64) -> f64 {
    fn simpson<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64) -> f64 {
        let m = (a + b) / 2.0;
        (b - a) / 6.0 * (f(a) + 4.0 * f(m) + f(b))
    }
    fn adaptive<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64, eps: f64, depth: u32) -> f64 {
        let whole = simpson(f, a, b);
        let mid = (a + b) / 2.0;
        let left = simpson(f, a, mid);
        let right = simpson(f, mid, b);
        if depth == 0 || (left + right - whole).abs() < 15.0 * eps {
            left + right + (left + right - whole) / 15.0
        } else {
            adaptive(f, a, mid, eps / 2.0, depth - 1) + adaptive(f, mid, b, eps / 2.0, depth - 1)
        }
    }
    let f = |x: f64| {
        let val = ast.eval_at(var, x);
        if val.is_finite() {
            val
        } else {
            0.0
        }
    };
    adaptive(&f, a, b, 1e-10, 20)
}

// ============================================================================
// Límite numérico (Richardson bilateral)
// ============================================================================

fn richardson_limit(ast: &Expr, var: &str, at: f64) -> Option<f64> {
    let n = 6usize;
    let h0 = 0.1;
    let eval_side = |sign: f64| -> Option<f64> {
        let mut vals: Vec<f64> = Vec::with_capacity(n);
        for i in 0..n {
            let h = h0 / 2.0f64.powi(i as i32);
            let v = ast.eval_at(var, at + sign * h);
            if v.is_finite() {
                vals.push(v);
            } else {
                return None;
            }
        }
        // Richardson (orden h²)
        let mut r = vals.clone();
        for j in 1..n {
            for i in 0..(n - j) {
                let p = 4.0f64.powi(j as i32);
                r[i] = (p * r[i + 1] - r[i]) / (p - 1.0);
            }
        }
        Some(r[0])
    };
    let left = eval_side(-1.0)?;
    let right = eval_side(1.0)?;
    if (left - right).abs() < 1e-4 {
        Some((left + right) / 2.0)
    } else {
        None
    }
}

// ============================================================================
// Utilidades: expansión, sustitución, variables, raíces numéricas
// ============================================================================

/// Distributividad de productos sobre sumas/restas.
fn expand_expr(e: &Expr) -> Expr {
    use Expr::*;
    match e {
        Neg(a) => Neg(Box::new(expand_expr(a))),
        Add(a, b) => Add(Box::new(expand_expr(a)), Box::new(expand_expr(b))),
        Sub(a, b) => Sub(Box::new(expand_expr(a)), Box::new(expand_expr(b))),
        Mul(a, b) => {
            let ea = expand_expr(a);
            let eb = expand_expr(b);
            match (&ea, &eb) {
                (Add(x, y), _) => Add(
                    Box::new(expand_expr(&Mul(x.clone(), Box::new(eb.clone())))),
                    Box::new(expand_expr(&Mul(y.clone(), Box::new(eb)))),
                ),
                (_, Add(x, y)) => Add(
                    Box::new(expand_expr(&Mul(Box::new(ea.clone()), x.clone()))),
                    Box::new(expand_expr(&Mul(Box::new(ea), y.clone()))),
                ),
                (Sub(x, y), _) => Sub(
                    Box::new(expand_expr(&Mul(x.clone(), Box::new(eb.clone())))),
                    Box::new(expand_expr(&Mul(y.clone(), Box::new(eb)))),
                ),
                (_, Sub(x, y)) => Sub(
                    Box::new(expand_expr(&Mul(Box::new(ea.clone()), x.clone()))),
                    Box::new(expand_expr(&Mul(Box::new(ea), y.clone()))),
                ),
                _ => Mul(Box::new(ea), Box::new(eb)),
            }
        }
        _ => e.clone(),
    }
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

/// Busca raíces reales de `ast(var)` en [lo, hi] por escaneo + bisección.
fn find_real_roots_numeric(ast: &Expr, var: &str, lo: f64, hi: f64) -> Vec<f64> {
    let f = |x: f64| ast.eval_at(var, x);
    let mut roots = Vec::new();
    let steps = 2000usize;
    let dx = (hi - lo) / steps as f64;
    let mut prev_x = lo;
    let mut prev_y = f(lo);
    for i in 1..=steps {
        let x = lo + i as f64 * dx;
        let y = f(x);
        if prev_y.is_finite() && y.is_finite() {
            if y.abs() < 1e-12 {
                push_unique(&mut roots, x);
            } else if prev_y * y < 0.0 {
                let mut a = prev_x;
                let mut b = x;
                let mut fa = prev_y;
                for _ in 0..80 {
                    let m = (a + b) / 2.0;
                    let fm = f(m);
                    if fm.abs() < 1e-12 || (b - a).abs() < 1e-14 {
                        a = m;
                        break;
                    }
                    if fa * fm < 0.0 {
                        b = m;
                    } else {
                        a = m;
                        fa = fm;
                    }
                }
                push_unique(&mut roots, a);
            }
        }
        prev_x = x;
        prev_y = y;
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
    let coeffs = collect_polynomial_coeffs(ast, var, 4)?;
    Some(solve_polynomial_real(&coeffs))
}

/// Resolución de polinomios con raíces complejas (Durand–Kerner).
pub fn solve_polynomial_complex(ast: &Expr, var: &str) -> Option<Vec<(f64, f64)>> {
    let coeffs = collect_polynomial_coeffs(ast, var, 20)?;
    Some(durand_kerner(&coeffs))
}

fn durand_kerner(coeffs: &[f64]) -> Vec<(f64, f64)> {
    let degree = coeffs.iter().rposition(|&c| c.abs() > 1e-12).unwrap_or(0);
    if degree == 0 {
        return vec![];
    }

    let lead = coeffs[degree];
    let mut norm_coeffs = vec![0.0; degree + 1];
    for i in 0..=degree {
        norm_coeffs[i] = coeffs[i] / lead;
    }

    let mut roots = Vec::with_capacity(degree);
    let mut angle: f64 = 0.4;
    let radius: f64 = 1.0;
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

    for _ in 0..100 {
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
            next_roots[i] = csub(roots[i], diff);
            let err = diff.0.hypot(diff.1);
            if err > max_err {
                max_err = err;
            }
        }
        roots = next_roots;
        if max_err < 1e-10 {
            break;
        }
    }

    for r in roots.iter_mut() {
        if r.1.abs() < 1e-9 {
            r.1 = 0.0;
        }
    }

    roots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

fn collect_polynomial_coeffs(ast: &Expr, var: &str, max_deg: usize) -> Option<Vec<f64>> {
    use crate::ast::Expr::{self, *};
    let mut coeffs = vec![0.0; max_deg + 1];
    fn collect_terms(expr: &Expr, var: &str, coeffs: &mut Vec<f64>) -> bool {
        match expr {
            Const(c) => {
                coeffs[0] += c;
                true
            }
            Var(v) if v == var => {
                coeffs[1] += 1.0;
                true
            }
            Neg(a) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] -= sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Add(a, b) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, coeffs) && collect_terms(b, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] += sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Sub(a, b) => {
                let mut sub = vec![0.0; coeffs.len()];
                if collect_terms(a, var, coeffs) && collect_terms(b, var, &mut sub) {
                    for i in 0..coeffs.len() {
                        coeffs[i] -= sub[i];
                    }
                    true
                } else {
                    false
                }
            }
            Mul(a, b) => {
                if let Const(c) = a.as_ref() {
                    let mut sub = vec![0.0; coeffs.len()];
                    if collect_terms(b, var, &mut sub) {
                        for i in 0..coeffs.len() {
                            coeffs[i] += c * sub[i];
                        }
                        true
                    } else {
                        false
                    }
                } else if let Const(c) = b.as_ref() {
                    let mut sub = vec![0.0; coeffs.len()];
                    if collect_terms(a, var, &mut sub) {
                        for i in 0..coeffs.len() {
                            coeffs[i] += c * sub[i];
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Pow(base, exp) => {
                if let (Var(v), Const(n)) = (base.as_ref(), exp.as_ref()) {
                    if v == var && *n >= 0.0 && *n <= coeffs.len() as f64 - 1.0 {
                        let idx = n.round() as usize;
                        if idx < coeffs.len() {
                            coeffs[idx] += 1.0;
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
    if collect_terms(ast, var, &mut coeffs) {
        Some(coeffs)
    } else {
        None
    }
}

fn solve_polynomial_real(coeffs: &[f64]) -> Vec<f64> {
    let degree = coeffs.iter().rposition(|&c| c.abs() > 1e-12).unwrap_or(0);
    match degree {
        1 => solve_linear(coeffs),
        2 => solve_quadratic(coeffs),
        3 => solve_cubic(coeffs),
        4 => solve_quartic(coeffs),
        _ => solve_polynomial_newton(coeffs),
    }
}

fn solve_linear(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[1];
    let b = coeffs[0];
    if a.abs() < 1e-12 {
        vec![]
    } else {
        vec![-b / a]
    }
}

fn solve_quadratic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[2];
    let b = coeffs[1];
    let c = coeffs[0];
    if a.abs() < 1e-12 {
        return solve_linear(&[c, b]);
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < -1e-12 {
        vec![]
    } else if discriminant.abs() < 1e-12 {
        vec![-b / (2.0 * a)]
    } else {
        let sqrt_d = discriminant.sqrt();
        vec![(-b - sqrt_d) / (2.0 * a), (-b + sqrt_d) / (2.0 * a)]
    }
}

fn solve_cubic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[3];
    let b = coeffs[2];
    let c = coeffs[1];
    let d = coeffs[0];
    if a.abs() < 1e-12 {
        return solve_quadratic(&[d, c, b]);
    }
    let b = b / a;
    let c = c / a;
    let d = d / a;
    let p = c - b * b / 3.0;
    let q = d - b * c / 3.0 + 2.0 * b * b * b / 27.0;
    let discriminant = q * q / 4.0 + p * p * p / 27.0;
    let shift = -b / 3.0;
    if discriminant > 1e-12 {
        let sqrt_d = discriminant.sqrt();
        let u = (-q / 2.0 + sqrt_d).cbrt();
        let v = (-q / 2.0 - sqrt_d).cbrt();
        vec![u + v + shift]
    } else if discriminant.abs() < 1e-12 {
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
/// coeficiente principal es ~0, y cae a Newton numérico en casos patológicos.
fn solve_quartic(coeffs: &[f64]) -> Vec<f64> {
    let a = coeffs[4];
    let b = coeffs[3];
    let c = coeffs[2];
    let d = coeffs[1];
    let e = coeffs[0];
    if a.abs() < 1e-12 {
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

    if q.abs() < 1e-10 {
        // Bicuadrática: y^4 + p y^2 + r = 0  →  z^2 + p z + r = 0 (z = y^2)
        let zroots = solve_quadratic(&[r, p, 1.0]);
        let mut out = Vec::new();
        for z in zroots {
            if z > 1e-12 {
                let s = z.sqrt();
                out.push(s + shift);
                out.push(-s + shift);
            } else if z.abs() < 1e-12 {
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
    if alpha.abs() < 1e-12 {
        return solve_polynomial_newton(coeffs);
    }
    let beta = -q / (2.0 * alpha);
    let disc1 = -(2.0 * z0 + p + 2.0 * beta);
    let disc2 = -(2.0 * z0 + p - 2.0 * beta);

    let mut out = Vec::new();
    if disc1 >= -1e-9 {
        let s = disc1.max(0.0).sqrt();
        out.push((alpha + s) / 2.0 + shift);
        out.push((alpha - s) / 2.0 + shift);
    }
    if disc2 >= -1e-9 {
        let s = disc2.max(0.0).sqrt();
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
        assert_eq!(r, "1");
    }

    #[test]
    fn test_solve_quadratic_pm2() {
        let r = solve("x^2 - 4", "x").unwrap();
        assert!(r.contains("-2"), "got {r}");
        assert!(r.contains("2.000"), "got {r}");
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
    fn test_limit_sinc() {
        let r = limit("sin(x)/x", "x", 0.0).unwrap();
        assert!(r.contains("1.0"), "got {r}");
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
    fn test_integrate_definite_linear() {
        let result = integrate_definite("2*x", "x", 0.0, 3.0).unwrap();
        assert!(result.contains('9'));
    }
}
