use evalexpr::*;

fn setup_math_context() -> HashMapContext {
    let mut ctx = HashMapContext::new();
    let _ = ctx.set_function("sin".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sin()))));
    let _ = ctx.set_function("cos".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cos()))));
    let _ = ctx.set_function("tan".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tan()))));
    let _ = ctx.set_function("asin".into(), evalexpr::Function::new(|arg| {
        let v = arg.as_float()?;
        let clamped = if v > 1.0 && v < 1.0 + 1e-8 { 1.0 } else if v < -1.0 && v > -1.0 - 1e-8 { -1.0 } else { v };
        Ok(Value::Float(clamped.asin()))
    }));
    let _ = ctx.set_function("acos".into(), evalexpr::Function::new(|arg| {
        let v = arg.as_float()?;
        let clamped = if v > 1.0 && v < 1.0 + 1e-8 { 1.0 } else if v < -1.0 && v > -1.0 - 1e-8 { -1.0 } else { v };
        Ok(Value::Float(clamped.acos()))
    }));
    let _ = ctx.set_function("atan".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.atan()))));
    let _ = ctx.set_function("sqrt".into(), evalexpr::Function::new(|arg| {
        let v = arg.as_float()?;
        let clamped = if v < 0.0 && v > -1e-8 { 0.0 } else { v };
        Ok(Value::Float(clamped.sqrt()))
    }));
    let _ = ctx.set_function("abs".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.abs()))));
    let _ = ctx.set_function("exp".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.exp()))));
    let _ = ctx.set_function("ln".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ln()))));
    let _ = ctx.set_function("sec".into(), evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.cos()))));
    let _ = ctx.set_function("csc".into(), evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.sin()))));
    let _ = ctx.set_function("cot".into(), evalexpr::Function::new(|arg| Ok(Value::Float(1.0 / arg.as_float()?.tan()))));
    let _ = ctx.set_function("sinh".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sinh()))));
    let _ = ctx.set_function("cosh".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cosh()))));
    let _ = ctx.set_function("tanh".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tanh()))));
    let _ = ctx.set_function("sign".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.signum()))));
    let _ = ctx.set_function("floor".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.floor()))));
    let _ = ctx.set_function("ceil".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ceil()))));
    let _ = ctx.set_function("round".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.round()))));
    let _ = ctx.set_function("log".into(), evalexpr::Function::new(|arg| {
        let t = arg.as_tuple()?;
        if t.len() == 2 {
            Ok(Value::Float(t[0].as_float()?.log(t[1].as_float()?)))
        } else {
            Err(evalexpr::EvalexprError::wrong_function_argument_amount(2, t.len()))
        }
    }));
    let _ = ctx.set_value("pi".into(), Value::Float(std::f64::consts::PI));
    let _ = ctx.set_value("e".into(), Value::Float(std::f64::consts::E));
    ctx
}


pub fn preprocess_expr(expr: &str) -> String {
    let mut s = expr.to_string();
    
    // Replace LaTeX rac{A}{B} with ((A)/(B))
    // We will do a simple iterative replacement finding rac
    while let Some(idx) = s.find("\\frac{") {
        let mut brace_count = 1;
        let mut end_a = 0;
        let start_a = idx + 6;
        for (i, c) in s[start_a..].char_indices() {
            if c == '{' { brace_count += 1; }
            else if c == '}' { brace_count -= 1; }
            if brace_count == 0 { end_a = start_a + i; break; }
        }
        
        let start_b_search = end_a + 1;
        if start_b_search < s.len() && s[start_b_search..].starts_with("{") {
            let start_b = start_b_search + 1;
            let mut brace_count = 1;
            let mut end_b = 0;
            for (i, c) in s[start_b..].char_indices() {
                if c == '{' { brace_count += 1; }
                else if c == '}' { brace_count -= 1; }
                if brace_count == 0 { end_b = start_b + i; break; }
            }
            if end_a > 0 && end_b > 0 {
                let part_a = &s[start_a..end_a];
                let part_b = &s[start_b..end_b];
                let replacement = format!("(({})/({}))", part_a, part_b);
                s.replace_range(idx..end_b+1, &replacement);
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
            if c == '{' { brace_count += 1; }
            else if c == '}' { brace_count -= 1; }
            if brace_count == 0 { end_a = start_a + i; break; }
        }
        if end_a > 0 {
            let part_a = &s[start_a..end_a];
            let replacement = format!("sqrt({})", part_a);
            s.replace_range(idx..end_a+1, &replacement);
            continue;
        }
        break;
    }

    let replaced = s.replace("cos^(-1)", "acos")
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
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() { res.push('*'); }
            if c1 == ')' && c2.is_ascii_alphabetic() { res.push('*'); }
            if c1 == ')' && c2.is_ascii_digit() { res.push('*'); }
            if c1.is_ascii_digit() && c2 == '(' { res.push('*'); }
            if c1 == ')' && c2 == '(' { res.push('*'); }
        }
    }
    res
}


/// Evaluate a mathematical expression string with given variable values.
pub fn evaluate(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    let mut ctx = setup_math_context();
    let expr = preprocess_expr(expr);
    
    for (name, val) in vars {
        if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
            return Err(format!("Variable error: {}", e));
        }
    }
    match eval_with_context(&expr, &ctx) {
        Ok(Value::Float(n)) => Ok(n),
        Ok(Value::Int(n)) => Ok(n as f64),
        Ok(other) => Err(format!("Expression did not evaluate to a number: {:?}", other)),
        Err(e) => Err(format!("Evaluation error: {}", e)),
    }
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
pub fn eval_function_with_vars(expr: &str, x: f64, vars: &std::collections::HashMap<String, f64>) -> Result<f64, String> {
    eval_function_batch(expr, std::iter::once(x), vars)
        .and_then(|mut res| {
            if let Some(Some(val)) = res.pop() { Ok(val) } else { Err("Evaluation failed".to_string()) }
        })
}

pub fn eval_batch_1d(expr: &str, var_name: &str, xs: impl Iterator<Item = f64> + Clone, vars: &std::collections::HashMap<String, f64>) -> Result<Vec<Option<f64>>, String> {
    let expr_clean = expr.trim();
    if expr_clean.starts_with("deriv(") && expr_clean.ends_with(')') {
        let inner = &expr_clean[6..expr_clean.len()-1];
        let h = 1e-5;
        let xs_vec: Vec<f64> = xs.collect();
        let xs1: Vec<f64> = xs_vec.iter().map(|&x| x + h).collect();
        let xs2: Vec<f64> = xs_vec.iter().map(|&x| x - h).collect();
        let res1 = eval_batch_1d(inner, var_name, xs1.into_iter(), vars)?;
        let res2 = eval_batch_1d(inner, var_name, xs2.into_iter(), vars)?;
        
        let mut results = Vec::with_capacity(res1.len());
        for (y1, y2) in res1.into_iter().zip(res2.into_iter()) {
            if let (Some(y1), Some(y2)) = (y1, y2) {
                results.push(Some((y1 - y2) / (2.0 * h)));
            } else {
                results.push(None);
            }
        }
        return Ok(results);
    }

    let expr_clean = preprocess_expr(expr_clean);
    
    // FAST PATH: try to parse with our custom AST
    if let Ok(mut ast) = crate::ast::parse_ast(&expr_clean) {
        ast = ast.substitute_vars(vars, &[var_name]).simplify();
        let mut results = Vec::new();
        for x in xs {
            let res = ast.eval_at(var_name, x);
            if res.is_nan() { results.push(None); } else { results.push(Some(res)); }
        }
        return Ok(results);
    }

    // SLOW PATH FALLBACK: evalexpr
    let tree = evalexpr::build_operator_tree(&expr_clean).map_err(|e| format!("Compile error: {}", e))?;
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

pub fn eval_batch_2d(expr: &str, var1_name: &str, var2_name: &str, points: impl Iterator<Item = (f64, f64)>, vars: &std::collections::HashMap<String, f64>) -> Result<Vec<Option<f64>>, String> {
    let expr_clean = expr.trim();
    let expr_clean = preprocess_expr(expr_clean);
    
    // FAST PATH: try to parse with our custom AST
    if let Ok(mut ast) = crate::ast::parse_ast(&expr_clean) {
        ast = ast.substitute_vars(vars, &[var1_name, var2_name]).simplify();
        let mut results = Vec::new();
        for (v1, v2) in points {
            let res = ast.eval_2d(var1_name, v1, var2_name, v2);
            if res.is_nan() { results.push(None); } else { results.push(Some(res)); }
        }
        return Ok(results);
    }

    // SLOW PATH FALLBACK: evalexpr
    let tree = evalexpr::build_operator_tree(&expr_clean).map_err(|e| format!("Compile error: {}", e))?;
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
        if ctx.set_value(var1_name.to_string(), Value::from(v1)).is_err() ||
           ctx.set_value(var2_name.to_string(), Value::from(v2)).is_err() {
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
pub fn eval_function_batch(expr: &str, xs: impl Iterator<Item = f64> + Clone, vars: &std::collections::HashMap<String, f64>) -> Result<Vec<Option<f64>>, String> {
    eval_batch_1d(expr, "x", xs, vars)
}


/// Batch evaluate a surface f(x, y) for multiple (x, y) points.
pub fn eval_surface_batch(expr: &str, pts: impl Iterator<Item = (f64, f64)>, vars: &std::collections::HashMap<String, f64>) -> Result<Vec<Option<f64>>, String> {
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
        if ctx.set_value("x".to_string(), Value::from(x)).is_err() ||
           ctx.set_value("y".to_string(), Value::from(y)).is_err() {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_eval_syntax() {
        println!("sin(1.0): {:?}", eval_function("sin(1.0)", 1.0));
        println!("math::sin(1.0): {:?}", eval_function("math::sin(1.0)", 1.0));
        println!("x^2 at 2.0: {:?}", eval_function("x^2", 2.0));
        println!("x^2 at 2.5: {:?}", eval_function("x^2", 2.5));
        println!("cos^(-1)(1-abs(x))-π at 0.5: {:?}", eval_function("cos^(-1)(1-abs(x))-π", 0.5));
    }
    fn insert_implicit_multiplication(text: &str) -> String {
        let mut res = String::new();
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            res.push(chars[i]);
            if i + 1 < chars.len() {
                let c1 = chars[i];
                let c2 = chars[i + 1];
                if c1.is_ascii_digit() && c2.is_ascii_alphabetic() { res.push('*'); }
                if c1 == ')' && c2.is_ascii_alphabetic() { res.push('*'); }
                if c1 == ')' && c2.is_ascii_digit() { res.push('*'); }
                if c1.is_ascii_digit() && c2 == '(' { res.push('*'); }
                if c1 == ')' && c2 == '(' { res.push('*'); }
            }
        }
        res
    }

    #[test]
    fn test_implicit_mul() {
        println!("2x -> {}", insert_implicit_multiplication("2x"));
        println!("2(x+1) -> {}", insert_implicit_multiplication("2(x+1)"));
        println!("(x+1)(x-1) -> {}", insert_implicit_multiplication("(x+1)(x-1)"));
        println!("sin(x) -> {}", insert_implicit_multiplication("sin(x)"));
        println!("2sin(x) -> {}", insert_implicit_multiplication("2sin(x)"));
    }
}
