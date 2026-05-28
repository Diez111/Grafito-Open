use evalexpr::*;

/// Evaluate a mathematical expression string with given variable values.
pub fn evaluate(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    let mut ctx = HashMapContext::new();
    
    // Bind standard math functions so user doesn't have to type "math::sin"
    let _ = ctx.set_function("sin".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sin()))));
    let _ = ctx.set_function("cos".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cos()))));
    let _ = ctx.set_function("tan".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tan()))));
    let _ = ctx.set_function("sqrt".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sqrt()))));
    let _ = ctx.set_function("abs".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.abs()))));
    let _ = ctx.set_function("exp".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.exp()))));
    let _ = ctx.set_function("ln".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ln()))));
    let _ = ctx.set_function("log".into(), evalexpr::Function::new(|arg| {
        let t = arg.as_tuple()?;
        if t.len() == 2 {
            Ok(Value::Float(t[0].as_float()?.log(t[1].as_float()?)))
        } else {
            Err(evalexpr::EvalexprError::wrong_function_argument_amount(2, t.len()))
        }
    }));
    
    for (name, val) in vars {
        if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
            return Err(format!("Variable error: {}", e));
        }
    }
    match eval_with_context(expr, &ctx) {
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

/// Evaluate a function f(x) with additional variables.
pub fn eval_function_with_vars(expr: &str, x: f64, vars: &std::collections::HashMap<String, f64>) -> Result<f64, String> {
    let mut ctx = HashMapContext::new();
    
    // Bind standard math functions
    let _ = ctx.set_function("sin".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sin()))));
    let _ = ctx.set_function("cos".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.cos()))));
    let _ = ctx.set_function("tan".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.tan()))));
    let _ = ctx.set_function("sqrt".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.sqrt()))));
    let _ = ctx.set_function("abs".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.abs()))));
    let _ = ctx.set_function("exp".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.exp()))));
    let _ = ctx.set_function("ln".into(), evalexpr::Function::new(|arg| Ok(Value::Float(arg.as_float()?.ln()))));
    let _ = ctx.set_function("log".into(), evalexpr::Function::new(|arg| {
        let t = arg.as_tuple()?;
        if t.len() == 2 {
            Ok(Value::Float(t[0].as_float()?.log(t[1].as_float()?)))
        } else {
            Err(evalexpr::EvalexprError::wrong_function_argument_amount(2, t.len()))
        }
    }));
    
    if let Err(e) = ctx.set_value("x".to_string(), Value::from(x)) {
        return Err(format!("Variable error: {}", e));
    }
    for (name, val) in vars {
        if name != "x" {
            if let Err(e) = ctx.set_value(name.clone(), Value::from(*val)) {
                return Err(format!("Variable error: {}", e));
            }
        }
    }
    match eval_with_context(expr, &ctx) {
        Ok(Value::Float(n)) => Ok(n),
        Ok(Value::Int(n)) => Ok(n as f64),
        Ok(other) => Err(format!("Expression did not evaluate to a number: {:?}", other)),
        Err(e) => Err(format!("Evaluation error: {}", e)),
    }
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
