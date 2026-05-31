import re

with open('crates/grafito-geometry/src/expr.rs', 'r') as f:
    text = f.read()

new_preprocess = """
pub fn preprocess_expr(expr: &str) -> String {
    let mut s = expr.to_string();
    
    // Replace LaTeX \frac{A}{B} with ((A)/(B))
    // We will do a simple iterative replacement finding \frac
    while let Some(idx) = s.find("\\\\frac{") {
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
    while let Some(idx) = s.find("\\\\sqrt{") {
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
        .replace("\\\\pi", "pi")
        .replace("\\\\sin", "sin")
        .replace("\\\\cos", "cos")
        .replace("\\\\tan", "tan")
        .replace("\\\\ln", "ln")
        .replace("\\\\log", "log")
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
"""

text = text.replace("""pub fn preprocess_expr(expr: &str) -> String {
    let replaced = expr.replace("cos^(-1)", "acos")
        .replace("sin^(-1)", "asin")
        .replace("tan^(-1)", "atan")
        .replace("π", "pi")
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
}""", new_preprocess)

with open('crates/grafito-geometry/src/expr.rs', 'w') as f:
    f.write(text)

