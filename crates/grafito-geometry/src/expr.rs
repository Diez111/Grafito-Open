use evalexpr::*;
use std::cell::RefCell;
use std::collections::HashMap;

fn setup_math_context() -> HashMapContext {
    let mut ctx = HashMapContext::new();
    let _ = ctx.set_function(
        "sin".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sin()))),
    );
    let _ = ctx.set_function(
        "cos".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cos()))),
    );
    let _ = ctx.set_function(
        "tan".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tan()))),
    );
    let _ = ctx.set_function(
        "asin".into(),
        evalexpr::Function::new(|arg| {
            let v = arg.as_float()?;
            let clamped = if v > 1.0 && v < 1.0 + 1e-8 {
                1.0
            } else if v < -1.0 && v > -1.0 - 1e-8 {
                -1.0
            } else {
                v
            };
            Ok(Value::Float(clamped.asin()))
        }),
    );
    let _ = ctx.set_function(
        "acos".into(),
        evalexpr::Function::new(|arg| {
            let v = arg.as_float()?;
            let clamped = if v > 1.0 && v < 1.0 + 1e-8 {
                1.0
            } else if v < -1.0 && v > -1.0 - 1e-8 {
                -1.0
            } else {
                v
            };
            Ok(Value::Float(clamped.acos()))
        }),
    );
    let _ = ctx.set_function(
        "atan".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.atan()))),
    );
    let _ = ctx.set_function(
        "sqrt".into(),
        evalexpr::Function::new(|arg| {
            let v = arg.as_float()?;
            let clamped = if v < 0.0 && v > -1e-8 { 0.0 } else { v };
            Ok(Value::Float(clamped.sqrt()))
        }),
    );
    let _ = ctx.set_function(
        "abs".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.abs()))),
    );
    let _ = ctx.set_function(
        "exp".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.exp()))),
    );
    let _ = ctx.set_function(
        "ln".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ln()))),
    );
    let _ = ctx.set_function(
        "sec".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.cos()))),
    );
    let _ = ctx.set_function(
        "csc".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.sin()))),
    );
    let _ = ctx.set_function(
        "cot".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.tan()))),
    );
    let _ = ctx.set_function(
        "sinh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sinh()))),
    );
    let _ = ctx.set_function(
        "cosh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cosh()))),
    );
    let _ = ctx.set_function(
        "tanh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tanh()))),
    );
    let _ = ctx.set_function(
        "sign".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.signum()))),
    );
    let _ = ctx.set_function(
        "floor".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.floor()))),
    );
    let _ = ctx.set_function(
        "ceil".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ceil()))),
    );
    let _ = ctx.set_function(
        "round".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.round()))),
    );
    let _ = ctx.set_function(
        "sec".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.cos()))),
    );
    let _ = ctx.set_function(
        "csc".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.sin()))),
    );
    let _ = ctx.set_function(
        "cot".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.tan()))),
    );
    let _ = ctx.set_function(
        "asinh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.asinh()))),
    );
    let _ = ctx.set_function(
        "acosh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.acosh()))),
    );
    let _ = ctx.set_function(
        "atanh".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.atanh()))),
    );
    let _ = ctx.set_function(
        "heaviside".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(if arg.as_float()? < 0.0 { 0.0 } else { 1.0 }))
        }),
    );
    let _ = ctx.set_function(
        "step".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(if arg.as_float()? < 0.0 { 0.0 } else { 1.0 }))
        }),
    );
    let _ = ctx.set_function(
        "cbrt".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cbrt()))),
    );
    let _ = ctx.set_function(
        "re".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?))),
    );
    let _ = ctx.set_function(
        "real".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?))),
    );
    let _ = ctx.set_function(
        "im".into(),
        evalexpr::Function::new(|_| Ok(Value::Float(0.0))),
    );
    let _ = ctx.set_function(
        "imag".into(),
        evalexpr::Function::new(|_| Ok(Value::Float(0.0))),
    );
    let _ = ctx.set_function(
        "arg".into(),
        evalexpr::Function::new(|arg| {
            let v = arg.as_float()?;
            Ok(Value::Float(if v >= 0.0 {
                0.0
            } else {
                std::f64::consts::PI
            }))
        }),
    );
    let _ = ctx.set_function(
        "conj".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?))),
    );
    let _ = ctx.set_function(
        "conjugate".into(),
        evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?))),
    );
    // Special functions
    let _ = ctx.set_function(
        "erf".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::erf(arg.as_float()?)))
        }),
    );
    let _ = ctx.set_function(
        "erfc".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::erfc(
                arg.as_float()?,
            )))
        }),
    );
    let _ = ctx.set_function(
        "gamma".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::gamma(
                arg.as_float()?,
            )))
        }),
    );
    let _ = ctx.set_function(
        "lngamma".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::ln_gamma(
                arg.as_float()?,
            )))
        }),
    );
    let _ = ctx.set_function(
        "lgamma".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::ln_gamma(
                arg.as_float()?,
            )))
        }),
    );
    let _ = ctx.set_function(
        "digamma".into(),
        evalexpr::Function::new(|arg| {
            Ok(Value::Float(crate::special_functions::digamma(
                arg.as_float()?,
            )))
        }),
    );
    // Multi-arg functions via tuples
    let _ = ctx.set_function(
        "atan2".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(t[0].as_float()?.atan2(t[1].as_float()?)))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "mod".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(t[0].as_float()? % t[1].as_float()?))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "min".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(t[0].as_float()?.min(t[1].as_float()?)))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "max".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(t[0].as_float()?.max(t[1].as_float()?)))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "clamp".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 3 {
                Ok(Value::Float(
                    t[0].as_float()?.clamp(t[1].as_float()?, t[2].as_float()?),
                ))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    3,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "beta".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(crate::special_functions::beta(
                    t[0].as_float()?,
                    t[1].as_float()?,
                )))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "besselj".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(crate::special_functions::bessel_j(
                    t[0].as_float()? as i32,
                    t[1].as_float()?,
                )))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "bessely".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(crate::special_functions::bessel_y(
                    t[0].as_float()? as i32,
                    t[1].as_float()?,
                )))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "besseli".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(crate::special_functions::bessel_i(
                    t[0].as_float()? as i32,
                    t[1].as_float()?,
                )))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_function(
        "log".into(),
        evalexpr::Function::new(|arg| {
            let t = arg.as_tuple()?;
            if t.len() == 2 {
                Ok(Value::Float(t[0].as_float()?.log(t[1].as_float()?)))
            } else {
                Err(evalexpr::EvalexprError::wrong_function_argument_amount(
                    2,
                    t.len(),
                ))
            }
        }),
    );
    let _ = ctx.set_value("pi".into(), Value::Float(std::f64::consts::PI));
    let _ = ctx.set_value("e".into(), Value::Float(std::f64::consts::E));
    ctx
}

fn split_args_depth0(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].trim().to_string());
    args
}

fn replace_standalone_var(expr: &str, var: &str, value: f64) -> String {
    let var_chars: Vec<char> = var.chars().collect();
    let expr_chars: Vec<char> = expr.chars().collect();
    let vs = if value == value.trunc() && value.is_finite() {
        format!("{:.1}", value)
    } else {
        value.to_string()
    };
    let mut result = String::new();
    let mut i = 0;
    while i < expr_chars.len() {
        if i + var_chars.len() <= expr_chars.len()
            && expr_chars[i..i + var_chars.len()] == var_chars[..]
        {
            let prev_is_bound = i == 0 || !expr_chars[i - 1].is_ascii_alphabetic();
            let next_is_bound = i + var_chars.len() >= expr_chars.len()
                || !expr_chars[i + var_chars.len()].is_ascii_alphabetic();
            if prev_is_bound && next_is_bound {
                result.push_str(&vs);
                i += var_chars.len();
                continue;
            }
        }
        result.push(expr_chars[i]);
        i += 1;
    }
    result
}

fn find_standalone_sum_product(expr: &str) -> Option<(usize, usize, bool)> {
    let chars: Vec<char> = expr.chars().collect();
    let patterns: &[(&str, bool)] = &[("sum(", false), ("product(", true), ("prod(", true)];
    for i in 0..chars.len() {
        for &(pat, is_prod) in patterns {
            if expr[i..].starts_with(pat) {
                let is_standalone = i == 0 || !chars[i - 1].is_ascii_alphabetic();
                if is_standalone {
                    let open_paren = i + pat.len() - 1;
                    if let Some(close) = find_matching_close(expr, open_paren) {
                        return Some((i, close, is_prod));
                    }
                }
            }
        }
    }
    None
}

fn eval_single_point(expr: &str, x_value: f64) -> Option<f64> {
    const MAX_EXPR_LEN: usize = 5000;
    const MAX_PAREN_DEPTH: usize = 64;
    if expr.len() > MAX_EXPR_LEN {
        return None;
    }
    let mut depth: i32 = 0;
    for c in expr.chars() {
        if c == '(' {
            depth += 1;
            if depth > MAX_PAREN_DEPTH as i32 {
                return None;
            }
        } else if c == ')' {
            depth -= 1;
        }
    }
    // Quick magnitude check — avoids full preprocess_expr recursion risk
    let mut ctx = setup_math_context();
    let _ = ctx.set_value("x".to_string(), Value::Float(x_value));
    match evalexpr::eval_with_context(expr, &ctx) {
        Ok(Value::Float(n)) if n.is_finite() => Some(n),
        Ok(Value::Int(n)) => Some(n as f64),
        _ => None,
    }
}

fn expand_sum_product_once(expr: &str) -> Option<String> {
    let (func_start, close, is_product) = find_standalone_sum_product(expr)?;
    let op = if is_product { "*" } else { "+" };
    let open = expr[func_start..].find('(')? + func_start;
    let inside = &expr[open + 1..close];
    let args = split_args_depth0(inside);
    if args.len() != 4 {
        return None;
    }

    let body = &args[0];
    let var = &args[1];
    let start: i64 = args[2].trim().parse().ok()?;
    let end: i64 = args[3].trim().parse().ok()?;

    let num_terms = (end.abs_diff(start) + 1) as usize;
    const MAX_TERMS: usize = 2000;
    if num_terms > MAX_TERMS {
        return None;
    }
    if num_terms == 0 {
        let identity = if is_product { "1" } else { "0" };
        return Some(identity.to_string());
    }

    let step: i64 = if end >= start { 1 } else { -1 };
    let mut terms = Vec::with_capacity(num_terms);
    let mut val = start;
    let mut tiny_count = 0u32;
    let min_terms = 5usize;
    loop {
        let substituted = replace_standalone_var(body, var, val as f64);
        // Auto-truncate: stop when terms become numerically negligible
        // (coefficients < 1e-14 or arguments to trig exceed f64 precision at ~1e15)
        if terms.len() >= min_terms {
            // Evaluate at x=0.5 (not x=0) to expose precision loss in trig:
            // cos(11^50 * pi * 0.5) has argument ~5e51 → f64 mantissa saturated → garbage
            let mag = eval_single_point(&substituted, 0.5);
            if mag.is_none() || (mag.unwrap().abs() < 1e-10) {
                tiny_count += 1;
                if tiny_count >= 3 {
                    break; // Series has converged numerically — remaining terms won't affect result
                }
            } else {
                tiny_count = 0;
            }
        }
        terms.push(format!("({})", substituted));
        if val == end {
            break;
        }
        val += step;
    }

    let expanded = terms.join(op);
    let prefix = &expr[..func_start];
    let suffix = &expr[close + 1..];
    Some(format!("{}{}{}", prefix, expanded, suffix))
}

fn find_matching_close(s: &str, open: usize) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut depth = 0;
    for (i, ch) in chars.iter().enumerate().skip(open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn expand_sum_product(expr: &str) -> String {
    let mut s = expr.to_string();
    let mut new_len;
    const MAX_EXPANDED_LEN: usize = 50_000;
    while s.len() <= MAX_EXPANDED_LEN {
        if let Some(expanded) = expand_sum_product_once(&s) {
            new_len = expanded.len();
            if new_len == s.len() {
                break;
            }
            s = expanded;
        } else {
            break;
        }
    }
    s
}

pub fn preprocess_expr(expr: &str) -> String {
    let mut s = expand_sum_product(expr);

    // Replace LaTeX rac{A}{B} with ((A)/(B))
    // We will do a simple iterative replacement finding rac
    while let Some(idx) = s.find("\\frac{") {
        let mut brace_count = 1;
        let mut end_a = 0;
        let start_a = idx + 6;
        for (i, c) in s[start_a..].char_indices() {
            if c == '{' {
                brace_count += 1;
            } else if c == '}' {
                brace_count -= 1;
            }
            if brace_count == 0 {
                end_a = start_a + i;
                break;
            }
        }

        let start_b_search = end_a + 1;
        if start_b_search < s.len() && s[start_b_search..].starts_with("{") {
            let start_b = start_b_search + 1;
            let mut brace_count = 1;
            let mut end_b = 0;
            for (i, c) in s[start_b..].char_indices() {
                if c == '{' {
                    brace_count += 1;
                } else if c == '}' {
                    brace_count -= 1;
                }
                if brace_count == 0 {
                    end_b = start_b + i;
                    break;
                }
            }
            if end_a > 0 && end_b > 0 {
                let part_a = &s[start_a..end_a];
                let part_b = &s[start_b..end_b];
                let replacement = format!("(({})/({}))", part_a, part_b);
                s.replace_range(idx..end_b + 1, &replacement);
                continue;
            }
        }
        // If we couldn't parse it properly, just break to avoid infinite loop
        break;
    }

    // Replace \sqrt{A} with sqrt(A)
    while let Some(idx) = s.find("\\sqrt{") {
        let mut brace_count = 1;
        let mut end_a = 0;
        let start_a = idx + 6;
        for (i, c) in s[start_a..].char_indices() {
            if c == '{' {
                brace_count += 1;
            } else if c == '}' {
                brace_count -= 1;
            }
            if brace_count == 0 {
                end_a = start_a + i;
                break;
            }
        }
        if end_a > 0 {
            let part_a = &s[start_a..end_a];
            let replacement = format!("sqrt({})", part_a);
            s.replace_range(idx..end_a + 1, &replacement);
            continue;
        }
        break;
    }

    let replaced = s
        .replace("cos^(-1)", "acos")
        .replace("sin^(-1)", "asin")
        .replace("tan^(-1)", "atan")
        .replace("π", "pi")
        .replace("\\pi", "pi")
        .replace("\\sin", "sin")
        .replace("\\cos", "cos")
        .replace("\\tan", "tan")
        .replace("\\ln", "ln")
        .replace("\\log", "log")
        .replace("f'(x)", "deriv(f(x))")
        .replace("g'(x)", "deriv(g(x))")
        .replace("h'(x)", "deriv(h(x))")
        // Remove accidental '*' after function names (e.g. if user types acos*(x))
        .replace("acos*(", "acos(")
        .replace("asin*(", "asin(")
        .replace("atan*(", "atan(")
        .replace("sin*(", "sin(")
        .replace("cos*(", "cos(")
        .replace("tan*(", "tan(")
        .replace("log*(", "log(")
        .replace("ln*(", "ln(")
        .replace("sqrt*(", "sqrt(")
        .replace("abs*(", "abs(")
        .replace("exp*(", "exp(");

    let mut res = String::new();
    let chars: Vec<char> = replaced.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_digit() {
                res.push('*');
            }
            if c1.is_ascii_digit() && c2 == '(' && (i == 0 || !chars[i - 1].is_ascii_alphabetic()) {
                res.push('*');
            }
            if c1 == ')' && c2 == '(' {
                res.push('*');
            }
        }
    }
    res
}

thread_local! {
    /// Cache of compiled expressions, keyed by the original expression string.
    ///
    /// Storing `None` means the expression failed to compile and should fall
    /// back to the slow interpreted path.
    static COMPILED_EXPR_CACHE: RefCell<HashMap<String, Option<CompiledExpr>>> =
        RefCell::new(HashMap::new());
}

/// Evaluate a mathematical expression, reusing a previously compiled form when
/// available.
///
/// This is the preferred path for callers that evaluate the same expression
/// repeatedly with different variable values (e.g. bound geometry parameters,
/// parametric samples). The first call compiles and caches the expression;
/// subsequent calls skip tokenisation/parsing/pre-processing.
pub fn evaluate_cached(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    COMPILED_EXPR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(expr) {
            let compiled = CompiledExpr::new(expr, &HashMap::new()).ok();
            cache.insert(expr.to_string(), compiled);
        }
        match cache.get(expr) {
            Some(Some(compiled)) => compiled.eval(vars),
            _ => evaluate(expr, vars),
        }
    })
}

/// Evaluate a mathematical expression string with given variable values.
pub fn evaluate(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    let expr_raw = expr;
    let expr = preprocess_expr(expr);

    // FAST PATH: try custom AST parser first
    let vars_map: std::collections::HashMap<String, f64> =
        vars.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let ignore: Vec<&str> = vars.iter().map(|(k, _)| k.as_str()).collect();
    if let Ok(mut ast) = crate::ast::parse_ast(&expr) {
        ast = ast.substitute_vars(&vars_map, &ignore).simplify();
        // For 1-var functions, use eval_at
        if vars.len() == 1 {
            let (var, val) = &vars[0];
            let result = ast.eval_at(var, *val);
            if result.is_finite() {
                return Ok(result);
            }
        } else if vars.len() == 2 {
            let (v1, x1) = &vars[0];
            let (v2, x2) = &vars[1];
            let result = ast.eval_2d(v1, *x1, v2, *x2);
            if result.is_finite() {
                return Ok(result);
            }
        } else if vars.len() == 3 {
            let (v1, x1) = &vars[0];
            let (v2, x2) = &vars[1];
            let (v3, x3) = &vars[2];
            let result = ast.eval_3d(v1, *x1, v2, *x2, v3, *x3);
            if result.is_finite() {
                return Ok(result);
            }
        }
    }

    // COMPLEX PATH: try complex evaluation if expression contains standalone 'i'
    if has_standalone_i(expr_raw) {
        if let Ok(complex_ast) = crate::complex_expr::parse(expr_raw) {
            let mut cmap = std::collections::HashMap::new();
            for (name, val) in vars {
                cmap.insert(name.clone(), num_complex::Complex64::new(*val, 0.0));
            }
            if let Ok(result) = complex_ast.eval(&cmap) {
                if result.re.is_finite() {
                    return Ok(result.re);
                }
            }
        }
    }

    // SLOW PATH FALLBACK: evalexpr
    let mut ctx = setup_math_context();
    for (name, val) in vars {
        if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
            return Err(format!("Variable error: {}", e));
        }
    }
    match eval_with_context(&expr, &ctx) {
        Ok(Value::Float(n)) => Ok(n),
        Ok(Value::Int(n)) => Ok(n as f64),
        Ok(other) => Err(format!(
            "Expression did not evaluate to a number: {:?}",
            other
        )),
        Err(e) => Err(format!("Evaluation error: {}", e)),
    }
}

fn has_standalone_i(expr: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    for idx in 0..chars.len() {
        if chars[idx] == 'i' || chars[idx] == 'I' {
            let prev_bound = idx == 0 || !chars[idx - 1].is_ascii_alphabetic();
            let next_bound = idx + 1 >= chars.len() || !chars[idx + 1].is_ascii_alphabetic();
            if prev_bound && next_bound {
                return true;
            }
        }
    }
    false
}

/// Evaluate a function f(x) expression.
pub fn eval_function(expr: &str, x: f64) -> Result<f64, String> {
    evaluate(expr, &[("x".to_string(), x)])
}

/// Evaluate expression with a named variable (not necessarily "x").
pub fn eval_function_var(expr: &str, var: &str, val: f64) -> Result<f64, String> {
    evaluate(expr, &[(var.to_string(), val)])
}

/// Evaluate a function f(x) with additional variables.
pub fn eval_function_with_vars(
    expr: &str,
    x: f64,
    vars: &std::collections::HashMap<String, f64>,
) -> Result<f64, String> {
    eval_function_batch(expr, std::iter::once(x), vars).and_then(|mut res| {
        if let Some(Some(val)) = res.pop() {
            Ok(val)
        } else {
            Err("Evaluation failed".to_string())
        }
    })
}

pub fn eval_batch_1d(
    expr: &str,
    var_name: &str,
    xs: impl Iterator<Item = f64> + Clone,
    vars: &std::collections::HashMap<String, f64>,
) -> Result<Vec<Option<f64>>, String> {
    let expr_clean = expr.trim();
    if expr_clean.starts_with("deriv(") && expr_clean.ends_with(')') {
        let inner = &expr_clean[6..expr_clean.len() - 1];
        let eps = f64::EPSILON.sqrt();
        let xs_vec: Vec<f64> = xs.collect();
        let hs: Vec<f64> = xs_vec.iter().map(|&x| eps * x.abs().max(1.0)).collect();
        let xs1: Vec<f64> = xs_vec.iter().zip(&hs).map(|(&x, &h)| x + h).collect();
        let xs2: Vec<f64> = xs_vec.iter().zip(&hs).map(|(&x, &h)| x - h).collect();
        let res1 = eval_batch_1d(inner, var_name, xs1.into_iter(), vars)?;
        let res2 = eval_batch_1d(inner, var_name, xs2.into_iter(), vars)?;

        let mut results = Vec::with_capacity(res1.len());
        for ((y1, y2), h) in res1.into_iter().zip(res2).zip(hs) {
            if let (Some(y1), Some(y2)) = (y1, y2) {
                results.push(Some((y1 - y2) / (2.0 * h)));
            } else {
                results.push(None);
            }
        }
        return Ok(results);
    }

    let expr_clean = preprocess_expr(expr_clean);
    let is_complex = has_standalone_i(expr.trim());

    // COMPLEX PATH: if expression contains i, use complex evaluator
    if is_complex {
        if let Ok(complex_ast) = crate::complex_expr::parse(expr.trim()) {
            let mut cmap = std::collections::HashMap::new();
            for (k, v) in vars {
                cmap.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
            }
            let mut results = Vec::new();
            for x in xs.clone() {
                cmap.insert(var_name.to_string(), num_complex::Complex64::new(x, 0.0));
                match complex_ast.eval(&cmap) {
                    Ok(val) if val.re.is_finite() => results.push(Some(val.re)),
                    _ => results.push(None),
                }
            }
            return Ok(results);
        }
    }

    // FAST PATH: try to parse with our custom AST
    if let Ok(mut ast) = crate::ast::parse_ast(&expr_clean) {
        ast = ast.substitute_vars(vars, &[var_name]).simplify();
        let mut results = Vec::new();
        for x in xs {
            let res = ast.eval_at(var_name, x);
            if res.is_nan() {
                results.push(None);
            } else {
                results.push(Some(res));
            }
        }
        return Ok(results);
    }

    // SLOW PATH FALLBACK: evalexpr
    let tree =
        evalexpr::build_operator_tree(&expr_clean).map_err(|e| format!("Compile error: {}", e))?;
    let mut ctx = setup_math_context();

    for (name, val) in vars {
        if name != var_name {
            if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
                return Err(format!("Variable error: {}", e));
            }
        }
    }

    let mut results = Vec::new();
    for x in xs {
        if ctx.set_value(var_name.to_string(), Value::from(x)).is_err() {
            results.push(None);
            continue;
        }
        match tree.eval_with_context(&ctx) {
            Ok(Value::Float(n)) => results.push(Some(n)),
            Ok(Value::Int(n)) => results.push(Some(n as f64)),
            _ => results.push(None),
        }
    }
    Ok(results)
}

pub fn eval_batch_2d(
    expr: &str,
    var1_name: &str,
    var2_name: &str,
    points: impl Iterator<Item = (f64, f64)>,
    vars: &std::collections::HashMap<String, f64>,
) -> Result<Vec<Option<f64>>, String> {
    let expr_clean = expr.trim();
    let expr_clean = preprocess_expr(expr_clean);

    // FAST PATH: try to parse with our custom AST
    if let Ok(mut ast) = crate::ast::parse_ast(&expr_clean) {
        ast = ast
            .substitute_vars(vars, &[var1_name, var2_name])
            .simplify();
        let mut results = Vec::new();
        for (v1, v2) in points {
            let res = ast.eval_2d(var1_name, v1, var2_name, v2);
            if res.is_nan() {
                results.push(None);
            } else {
                results.push(Some(res));
            }
        }
        return Ok(results);
    }

    // SLOW PATH FALLBACK: evalexpr
    let tree =
        evalexpr::build_operator_tree(&expr_clean).map_err(|e| format!("Compile error: {}", e))?;
    let mut ctx = setup_math_context();

    for (name, val) in vars {
        if name != var1_name && name != var2_name {
            if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
                return Err(format!("Variable error: {}", e));
            }
        }
    }

    let mut results = Vec::new();
    for (v1, v2) in points {
        if ctx
            .set_value(var1_name.to_string(), Value::from(v1))
            .is_err()
            || ctx
                .set_value(var2_name.to_string(), Value::from(v2))
                .is_err()
        {
            results.push(None);
            continue;
        }
        match tree.eval_with_context(&ctx) {
            Ok(Value::Float(n)) => results.push(Some(n)),
            Ok(Value::Int(n)) => results.push(Some(n as f64)),
            _ => results.push(None),
        }
    }
    Ok(results)
}

/// Batch evaluate a function for multiple x values.
pub fn eval_function_batch(
    expr: &str,
    xs: impl Iterator<Item = f64> + Clone,
    vars: &std::collections::HashMap<String, f64>,
) -> Result<Vec<Option<f64>>, String> {
    eval_batch_1d(expr, "x", xs, vars)
}

/// Batch evaluate a surface f(x, y) for multiple (x, y) points.
pub fn eval_surface_batch(
    expr: &str,
    pts: impl Iterator<Item = (f64, f64)>,
    vars: &std::collections::HashMap<String, f64>,
) -> Result<Vec<Option<f64>>, String> {
    let expr = preprocess_expr(expr);
    let tree = evalexpr::build_operator_tree(&expr).map_err(|e| format!("Compile error: {}", e))?;
    let mut ctx = setup_math_context();

    for (name, val) in vars {
        if name != "x" && name != "y" {
            if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
                return Err(format!("Variable error: {}", e));
            }
        }
    }

    let mut results = Vec::new();
    for (x, y) in pts {
        if ctx.set_value("x".to_string(), Value::from(x)).is_err()
            || ctx.set_value("y".to_string(), Value::from(y)).is_err()
        {
            results.push(None);
            continue;
        }
        match tree.eval_with_context(&ctx) {
            Ok(Value::Float(n)) => results.push(Some(n)),
            Ok(Value::Int(n)) => results.push(Some(n as f64)),
            _ => results.push(None),
        }
    }
    Ok(results)
}

/// Parse and check if expression is valid.
pub fn validate(expr: &str) -> bool {
    eval_function(expr, 0.0).is_ok()
}

/// Parse, preprocess, substitute variables, and simplify a function expression
/// into a reusable AST. This avoids re-parsing the expression for every sample
/// point during rendering — critical for sum()-expanded functions.
pub fn prepare_function_ast(
    expr: &str,
    vars: &std::collections::HashMap<String, f64>,
    ignore: &[&str],
) -> Result<crate::ast::Expr, String> {
    let expr_clean = preprocess_expr(expr);
    let mut ast = crate::ast::parse_ast(&expr_clean).map_err(|e| format!("Parse error: {}", e))?;
    ast = ast.substitute_vars(vars, ignore).simplify();
    Ok(ast)
}

/// A pre-parsed expression that can be evaluated many times without
/// re-tokenising or re-parsing the original string.
///
/// The fast path uses Grafito's native AST. If the expression cannot be parsed
/// by the native AST (e.g. it uses evalexpr-only syntax), it falls back to a
/// pre-built evalexpr operator tree. Constants supplied at compile time are
/// substituted once.
#[derive(Clone)]
pub struct CompiledExpr {
    ast: Option<crate::ast::Expr>,
    tree: Option<evalexpr::Node>,
}

impl CompiledExpr {
    /// Compile an expression, substituting the supplied constants.
    pub fn new(
        expr: &str,
        constants: &std::collections::HashMap<String, f64>,
    ) -> Result<Self, String> {
        let expr_clean = preprocess_expr(expr);
        let ignore: Vec<&str> = constants.keys().map(|s| s.as_str()).collect();

        if let Ok(mut ast) = crate::ast::parse_ast(&expr_clean) {
            ast = ast.substitute_vars(constants, &ignore).simplify();
            return Ok(Self {
                ast: Some(ast),
                tree: None,
            });
        }

        let tree = evalexpr::build_operator_tree(&expr_clean)
            .map_err(|e| format!("Compile error: {}", e))?;
        Ok(Self {
            ast: None,
            tree: Some(tree),
        })
    }

    /// Evaluate the compiled expression with the given variable values.
    /// The variable names must match those supplied at construction time.
    pub fn eval(&self, vars: &[(String, f64)]) -> Result<f64, String> {
        if let Some(ast) = &self.ast {
            match vars.len() {
                1 => {
                    let (var, val) = &vars[0];
                    let result = ast.eval_at(var, *val);
                    if result.is_finite() {
                        return Ok(result);
                    }
                }
                2 => {
                    let (v1, x1) = &vars[0];
                    let (v2, x2) = &vars[1];
                    let result = ast.eval_2d(v1, *x1, v2, *x2);
                    if result.is_finite() {
                        return Ok(result);
                    }
                }
                3 => {
                    let (v1, x1) = &vars[0];
                    let (v2, x2) = &vars[1];
                    let (v3, x3) = &vars[2];
                    let result = ast.eval_3d(v1, *x1, v2, *x2, v3, *x3);
                    if result.is_finite() {
                        return Ok(result);
                    }
                }
                _ => {
                    // Generic path: substitute all supplied variables into the
                    // AST, simplify, and read the resulting constant.
                    let vars_map: HashMap<String, f64> =
                        vars.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    let substituted = ast.clone().substitute_vars(&vars_map, &[]).simplify();
                    if let crate::ast::Expr::Const(result) = substituted {
                        if result.is_finite() {
                            return Ok(result);
                        }
                    }
                }
            }
        }

        if let Some(tree) = &self.tree {
            let mut ctx = setup_math_context();
            for (name, val) in vars {
                if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
                    return Err(format!("Variable error: {}", e));
                }
            }
            return match tree.eval_with_context(&ctx) {
                Ok(Value::Float(n)) => Ok(n),
                Ok(Value::Int(n)) => Ok(n as f64),
                Ok(other) => Err(format!(
                    "Expression did not evaluate to a number: {:?}",
                    other
                )),
                Err(e) => Err(format!("Evaluation error: {}", e)),
            };
        }

        Err("Compiled expression has no evaluator".to_string())
    }

    /// Convenience: evaluate a single-variable compiled expression.
    pub fn eval_at(&self, var: &str, val: f64) -> Result<f64, String> {
        self.eval(&[(var.to_string(), val)])
    }
}

/// Compile an expression with the given constants and evaluate it once.
pub fn evaluate_compiled(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    let constants: std::collections::HashMap<String, f64> =
        vars.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let compiled = CompiledExpr::new(expr, &constants)?;
    compiled.eval(vars)
}

/// Evaluate a pre-parsed AST at a batch of x values.
pub fn eval_parsed_batch(
    ast: &crate::ast::Expr,
    var_name: &str,
    xs: impl Iterator<Item = f64>,
) -> Vec<Option<f64>> {
    xs.map(|x| {
        let res = ast.eval_at(var_name, x);
        if res.is_nan() {
            None
        } else {
            Some(res)
        }
    })
    .collect()
}

/// Evaluate ∫[lower → x] integrand(var) d(var) for each x value.
/// Uses Gauss-Legendre 5-point quadrature with adaptive subdivision.
pub fn eval_integral_batch(
    integrand: &str,
    int_var: &str,
    lower: f64,
    xs: impl Iterator<Item = f64> + Clone,
    vars: &std::collections::HashMap<String, f64>,
) -> Vec<Option<f64>> {
    // Prepare the integrand AST once
    let expr_clean = preprocess_expr(integrand);
    let prepared = crate::ast::parse_ast(&expr_clean)
        .map(|ast| ast.substitute_vars(vars, &[int_var]).simplify());

    let prepared = match prepared {
        Ok(p) => p,
        Err(_) => return xs.map(|_| None).collect(),
    };

    fn adaptive_integrate(
        prepared: &crate::ast::Expr,
        int_var: &str,
        a: f64,
        b: f64,
        depth: u32,
    ) -> Option<f64> {
        if depth == 0 {
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
            let mid = (a + b) * 0.5;
            let half = (b - a) * 0.5;
            let mut sum = 0.0;
            for (&xi, &wi) in nodes.iter().zip(weights.iter()) {
                let t = mid + half * xi;
                let val = prepared.eval_at(int_var, t);
                if val.is_finite() {
                    sum += wi * val;
                }
            }
            return Some(sum * half);
        }
        let mid = (a + b) * 0.5;
        let left = adaptive_integrate(prepared, int_var, a, mid, depth - 1)?;
        let right = adaptive_integrate(prepared, int_var, mid, b, depth - 1)?;
        Some(left + right)
    }

    xs.map(|x| {
        if x < lower {
            adaptive_integrate(&prepared, int_var, x, lower, 4).map(|v| -v)
        } else if (x - lower).abs() < 1e-12 {
            Some(0.0)
        } else {
            adaptive_integrate(&prepared, int_var, lower, x, 4)
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_eval_syntax() {
        println!("sin(1.0): {:?}", eval_function("sin(1.0)", 1.0));
        println!("math::sin(1.0): {:?}", eval_function("math::sin(1.0)", 1.0));
        println!("x^2 at 2.0: {:?}", eval_function("x^2", 2.0));
        println!("x^2 at 2.5: {:?}", eval_function("x^2", 2.5));
        println!(
            "cos^(-1)(1-abs(x))-π at 0.5: {:?}",
            eval_function("cos^(-1)(1-abs(x))-π", 0.5)
        );
    }
    fn insert_implicit_multiplication(text: &str) -> String {
        let mut res = String::new();
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            res.push(chars[i]);
            if i + 1 < chars.len() {
                let c1 = chars[i];
                let c2 = chars[i + 1];
                if c1.is_ascii_digit() && c2.is_ascii_alphabetic() {
                    res.push('*');
                }
                if c1 == ')' && c2.is_ascii_alphabetic() {
                    res.push('*');
                }
                if c1 == ')' && c2.is_ascii_digit() {
                    res.push('*');
                }
                if c1.is_ascii_digit()
                    && c2 == '('
                    && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
                {
                    res.push('*');
                } // 3( but not atan2(
                if c1 == ')' && c2 == '(' {
                    res.push('*');
                }
            }
        }
        res
    }

    #[test]
    fn test_implicit_mul() {
        println!("2x -> {}", insert_implicit_multiplication("2x"));
        println!("2(x+1) -> {}", insert_implicit_multiplication("2(x+1)"));
        println!(
            "(x+1)(x-1) -> {}",
            insert_implicit_multiplication("(x+1)(x-1)")
        );
        println!("sin(x) -> {}", insert_implicit_multiplication("sin(x)"));
        println!("2sin(x) -> {}", insert_implicit_multiplication("2sin(x)"));
    }

    #[test]
    fn test_sum_expansion() {
        // sum(n^2, n, 1, 5) = 1+4+9+16+25 = 55
        assert!((eval_function("sum(n^2, n, 1, 5)", 0.0).unwrap() - 55.0).abs() < 0.01);
        // sum(1/n, n, 1, 4) = 1 + 1/2 + 1/3 + 1/4 ≈ 2.08333
        let v = eval_function("sum(1/n, n, 1, 4)", 0.0).unwrap();
        assert!((v - 2.08333333).abs() < 0.01, "got {}", v);
        // product(i, i, 1, 4) = 24
        assert!((eval_function("product(i, i, 1, 4)", 0.0).unwrap() - 24.0).abs() < 0.01);
        // sum(sin(n*x)/n, n, 1, 3) at x=0.5
        let v = eval_function("sum(sin(n*x)/n, n, 1, 3)", 0.5).unwrap();
        assert!((v - 1.23266).abs() < 0.01, "got {}", v);
        // Negative range: sum(n, n, -2, 2) = 0
        assert!((eval_function("sum(n, n, -2, 2)", 0.0).unwrap() - 0.0).abs() < 0.01);
        // Sum with x: f(x) = sum(n*x, n, 1, 3) = x + 2x + 3x = 6x, at x=2 = 12
        assert!((eval_function("sum(n*x, n, 1, 3)", 2.0).unwrap() - 12.0).abs() < 0.01);
    }
}
