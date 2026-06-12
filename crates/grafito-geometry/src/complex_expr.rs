use num_complex::Complex64;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ComplexExpr {
    Var(String),
    Const(Complex64),
    Add(Box<ComplexExpr>, Box<ComplexExpr>),
    Sub(Box<ComplexExpr>, Box<ComplexExpr>),
    Mul(Box<ComplexExpr>, Box<ComplexExpr>),
    Div(Box<ComplexExpr>, Box<ComplexExpr>),
    Pow(Box<ComplexExpr>, Box<ComplexExpr>),
    Sin(Box<ComplexExpr>),
    Cos(Box<ComplexExpr>),
    Tan(Box<ComplexExpr>),
    Exp(Box<ComplexExpr>),
    Ln(Box<ComplexExpr>),
    Sqrt(Box<ComplexExpr>),
    Abs(Box<ComplexExpr>),
    Neg(Box<ComplexExpr>),
}

impl ComplexExpr {
    pub fn eval(&self, vars: &HashMap<String, Complex64>) -> Result<Complex64, String> {
        match self {
            ComplexExpr::Var(name) => {
                if name == "i" {
                    Ok(Complex64::new(0.0, 1.0))
                } else if name == "e" {
                    Ok(Complex64::new(std::f64::consts::E, 0.0))
                } else if name == "pi" {
                    Ok(Complex64::new(std::f64::consts::PI, 0.0))
                } else {
                    vars.get(name)
                        .copied()
                        .ok_or_else(|| format!("Unknown variable: {}", name))
                }
            }
            ComplexExpr::Const(c) => Ok(*c),
            ComplexExpr::Add(a, b) => Ok(a.eval(vars)? + b.eval(vars)?),
            ComplexExpr::Sub(a, b) => Ok(a.eval(vars)? - b.eval(vars)?),
            ComplexExpr::Mul(a, b) => Ok(a.eval(vars)? * b.eval(vars)?),
            ComplexExpr::Div(a, b) => {
                let den = b.eval(vars)?;
                if den.norm() < 1e-15 {
                    return Err("Division by zero".to_string());
                }
                Ok(a.eval(vars)? / den)
            }
            ComplexExpr::Pow(a, b) => Ok(a.eval(vars)?.powc(b.eval(vars)?)),
            ComplexExpr::Sin(a) => Ok(a.eval(vars)?.sin()),
            ComplexExpr::Cos(a) => Ok(a.eval(vars)?.cos()),
            ComplexExpr::Tan(a) => Ok(a.eval(vars)?.tan()),
            ComplexExpr::Exp(a) => Ok(a.eval(vars)?.exp()),
            ComplexExpr::Ln(a) => Ok(a.eval(vars)?.ln()),
            ComplexExpr::Sqrt(a) => Ok(a.eval(vars)?.sqrt()),
            ComplexExpr::Abs(a) => Ok(Complex64::new(a.eval(vars)?.norm(), 0.0)),
            ComplexExpr::Neg(a) => Ok(-a.eval(vars)?),
        }
    }

    pub fn has_var(&self, target: &str) -> bool {
        match self {
            ComplexExpr::Var(name) => name == target,
            ComplexExpr::Const(_) => false,
            ComplexExpr::Add(a, b)
            | ComplexExpr::Sub(a, b)
            | ComplexExpr::Mul(a, b)
            | ComplexExpr::Div(a, b)
            | ComplexExpr::Pow(a, b) => a.has_var(target) || b.has_var(target),
            ComplexExpr::Sin(a)
            | ComplexExpr::Cos(a)
            | ComplexExpr::Tan(a)
            | ComplexExpr::Exp(a)
            | ComplexExpr::Ln(a)
            | ComplexExpr::Sqrt(a)
            | ComplexExpr::Abs(a)
            | ComplexExpr::Neg(a) => a.has_var(target),
        }
    }
}

pub fn parse(input: &str) -> Result<ComplexExpr, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let expr = parse_expr(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected token at end: {:?}", tokens[pos]));
    }
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_digit() || c == '.' {
            let mut num_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    num_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(
                num_str.parse().map_err(|_| "Invalid number")?,
            ));
        } else if c.is_alphabetic() {
            let mut id = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    id.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Ident(id));
        } else {
            let t = match c {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '^' => Token::Caret,
                '(' => Token::LParen,
                ')' => Token::RParen,
                _ => return Err(format!("Unknown char: {}", c)),
            };
            tokens.push(t);
            chars.next();
        }
    }

    // Insert implicit multiplications
    let mut i = 0;
    while i + 1 < tokens.len() {
        let insert = match (&tokens[i], &tokens[i + 1]) {
            (Token::Number(_), Token::Ident(_)) => true,
            (Token::Number(_), Token::LParen) => true,
            (Token::RParen, Token::Ident(_)) => true,
            (Token::RParen, Token::LParen) => true,
            (Token::Ident(_), Token::LParen) => {
                // If it's a known function, do NOT insert '*'
                let name = if let Token::Ident(n) = &tokens[i] {
                    n.as_str()
                } else {
                    ""
                };
                !matches!(
                    name,
                    "sin" | "cos" | "tan" | "exp" | "ln" | "log" | "sqrt" | "abs"
                )
            }
            _ => false,
        };
        if insert {
            tokens.insert(i + 1, Token::Star);
            i += 1;
        }
        i += 1;
    }

    Ok(tokens)
}

fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<ComplexExpr, String> {
    parse_add_sub(tokens, pos)
}

fn parse_add_sub(tokens: &[Token], pos: &mut usize) -> Result<ComplexExpr, String> {
    let mut node = parse_mul_div(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                node = ComplexExpr::Add(Box::new(node), Box::new(parse_mul_div(tokens, pos)?));
            }
            Token::Minus => {
                *pos += 1;
                node = ComplexExpr::Sub(Box::new(node), Box::new(parse_mul_div(tokens, pos)?));
            }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_mul_div(tokens: &[Token], pos: &mut usize) -> Result<ComplexExpr, String> {
    let mut node = parse_pow(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Star => {
                *pos += 1;
                node = ComplexExpr::Mul(Box::new(node), Box::new(parse_pow(tokens, pos)?));
            }
            Token::Slash => {
                *pos += 1;
                node = ComplexExpr::Div(Box::new(node), Box::new(parse_pow(tokens, pos)?));
            }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_pow(tokens: &[Token], pos: &mut usize) -> Result<ComplexExpr, String> {
    let mut node = parse_primary(tokens, pos)?;
    if *pos < tokens.len() && tokens[*pos] == Token::Caret {
        *pos += 1;
        // Right-associative
        node = ComplexExpr::Pow(Box::new(node), Box::new(parse_pow(tokens, pos)?));
    }
    Ok(node)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<ComplexExpr, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected EOF".to_string());
    }
    match &tokens[*pos] {
        Token::Number(n) => {
            *pos += 1;
            Ok(ComplexExpr::Const(Complex64::new(*n, 0.0)))
        }
        Token::Ident(name) => {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == Token::LParen {
                *pos += 1;
                let arg = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                    return Err("Expected ')'".to_string());
                }
                *pos += 1;
                match name.as_str() {
                    "sin" => Ok(ComplexExpr::Sin(Box::new(arg))),
                    "cos" => Ok(ComplexExpr::Cos(Box::new(arg))),
                    "tan" => Ok(ComplexExpr::Tan(Box::new(arg))),
                    "exp" => Ok(ComplexExpr::Exp(Box::new(arg))),
                    "ln" | "log" => Ok(ComplexExpr::Ln(Box::new(arg))),
                    "sqrt" => Ok(ComplexExpr::Sqrt(Box::new(arg))),
                    "abs" => Ok(ComplexExpr::Abs(Box::new(arg))),
                    _ => Err(format!("Unknown function: {}", name)),
                }
            } else {
                Ok(ComplexExpr::Var(name.clone()))
            }
        }
        Token::Minus => {
            *pos += 1;
            Ok(ComplexExpr::Neg(Box::new(parse_primary(tokens, pos)?)))
        }
        Token::LParen => {
            *pos += 1;
            let expr = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                return Err("Expected ')'".to_string());
            }
            *pos += 1;
            Ok(expr)
        }
        t => Err(format!("Unexpected token: {:?}", t)),
    }
}

pub fn eval_complex_batch(
    expr: &str,
    base_symbol: &str,
    points: impl Iterator<Item = Complex64>,
    vars: &HashMap<String, f64>,
) -> Result<Vec<Option<Complex64>>, String> {
    let ast = parse(expr)?;
    let mut cmap = HashMap::new();
    for (k, v) in vars {
        cmap.insert(k.clone(), Complex64::new(*v, 0.0));
    }

    let mut res = Vec::new();
    for z in points {
        cmap.insert(base_symbol.to_string(), z);
        match ast.eval(&cmap) {
            Ok(val) => {
                if val.re.is_finite() && val.im.is_finite() {
                    res.push(Some(val));
                } else {
                    res.push(None);
                }
            }
            Err(_) => res.push(None),
        }
    }
    Ok(res)
}
