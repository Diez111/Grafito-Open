use evalexpr::*;

/// Evaluate a mathematical expression string with given variable values.
pub fn evaluate(expr: &str, vars: &[(String, f64)]) -> Result<f64, String> {
    let mut ctx = HashMapContext::new();
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
