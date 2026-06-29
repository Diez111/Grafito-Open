use num_complex::Complex64;
use std::collections::HashMap;

/// Matriz isomórfica a un número complejo z = x + iy
/// Z = [[x, -y], [y, x]]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplexMatrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

impl ComplexMatrix {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            a: x,
            b: -y,
            c: y,
            d: x,
        }
    }

    pub fn from_complex(z: Complex64) -> Self {
        Self::new(z.re, z.im)
    }

    pub fn to_complex(&self) -> Complex64 {
        self.assert_invariant();
        Complex64::new(self.a, self.c)
    }

    pub fn assert_invariant(&self) {
        if self.a.is_finite() && self.b.is_finite() && self.c.is_finite() && self.d.is_finite() {
            let diff1 = (self.a - self.d).abs();
            let diff2 = (self.b + self.c).abs();
            // Aceptamos una pequeña tolerancia por ruido numérico de punto flotante
            assert!(
                diff1 < 1e-10 && diff2 < 1e-10,
                "Complex Representation Invariant Violated! a={}, b={}, c={}, d={}",
                self.a,
                self.b,
                self.c,
                self.d
            );
        }
    }

    pub fn add(&self, other: &Self) -> Self {
        Self {
            a: self.a + other.a,
            b: self.b + other.b,
            c: self.c + other.c,
            d: self.d + other.d,
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        Self {
            a: self.a - other.a,
            b: self.b - other.b,
            c: self.c - other.c,
            d: self.d - other.d,
        }
    }

    pub fn mul(&self, other: &Self) -> Self {
        // Multiplicación de matrices 2x2
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
        }
    }

    pub fn div(&self, other: &Self) -> Option<Self> {
        let det = other.a * other.d - other.b * other.c;
        if det.abs() < 1e-15 {
            return None;
        }
        let inv = Self {
            a: other.d / det,
            b: -other.b / det,
            c: -other.c / det,
            d: other.a / det,
        };
        Some(self.mul(&inv))
    }
}

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
    Asin(Box<ComplexExpr>),
    Acos(Box<ComplexExpr>),
    Atan(Box<ComplexExpr>),
    Sinh(Box<ComplexExpr>),
    Cosh(Box<ComplexExpr>),
    Tanh(Box<ComplexExpr>),
    Asinh(Box<ComplexExpr>),
    Acosh(Box<ComplexExpr>),
    Atanh(Box<ComplexExpr>),
    Gamma(Box<ComplexExpr>),
    BesselJ(Box<ComplexExpr>),
    Conjugate(Box<ComplexExpr>),
    RealPart(Box<ComplexExpr>),
    ImagPart(Box<ComplexExpr>),
    Arg(Box<ComplexExpr>),
    Erf(Box<ComplexExpr>),
    LambertW(Box<ComplexExpr>),
    Zeta(Box<ComplexExpr>),
    BesselY(Box<ComplexExpr>),
}

impl ComplexExpr {
    pub fn eval(&self, vars: &HashMap<String, Complex64>) -> Result<Complex64, String> {
        self.eval_depth(vars, 0)
    }

    fn eval_depth(
        &self,
        vars: &HashMap<String, Complex64>,
        depth: u32,
    ) -> Result<Complex64, String> {
        const MAX_COMPLEX_EVAL_DEPTH: u32 = 256;
        if depth > MAX_COMPLEX_EVAL_DEPTH {
            return Err("expresión compleja demasiado profunda".to_string());
        }
        match self {
            ComplexExpr::Var(name) => {
                let z = if name == "i" {
                    Complex64::new(0.0, 1.0)
                } else if name == "e" {
                    Complex64::new(std::f64::consts::E, 0.0)
                } else if name == "pi" {
                    Complex64::new(std::f64::consts::PI, 0.0)
                } else {
                    vars.get(name)
                        .copied()
                        .ok_or_else(|| format!("Unknown variable: {}", name))?
                };
                let mz = ComplexMatrix::from_complex(z);
                Ok(mz.to_complex())
            }
            ComplexExpr::Const(c) => Ok(ComplexMatrix::from_complex(*c).to_complex()),
            ComplexExpr::Add(a, b) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                let mb = ComplexMatrix::from_complex(b.eval_depth(vars, depth + 1)?);
                Ok(ma.add(&mb).to_complex())
            }
            ComplexExpr::Sub(a, b) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                let mb = ComplexMatrix::from_complex(b.eval_depth(vars, depth + 1)?);
                Ok(ma.sub(&mb).to_complex())
            }
            ComplexExpr::Mul(a, b) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                let mb = ComplexMatrix::from_complex(b.eval_depth(vars, depth + 1)?);
                Ok(ma.mul(&mb).to_complex())
            }
            ComplexExpr::Div(a, b) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                let mb = ComplexMatrix::from_complex(b.eval_depth(vars, depth + 1)?);
                if let Some(res) = ma.div(&mb) {
                    Ok(res.to_complex())
                } else {
                    Err("Division by zero".to_string())
                }
            }
            ComplexExpr::Pow(a, b) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                let mb = ComplexMatrix::from_complex(b.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().powc(mb.to_complex())).to_complex())
            }
            ComplexExpr::Sin(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().sin()).to_complex())
            }
            ComplexExpr::Cos(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().cos()).to_complex())
            }
            ComplexExpr::Tan(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().tan()).to_complex())
            }
            ComplexExpr::Exp(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().exp()).to_complex())
            }
            ComplexExpr::Ln(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().ln()).to_complex())
            }
            ComplexExpr::Sqrt(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().sqrt()).to_complex())
            }
            ComplexExpr::Abs(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(
                    ComplexMatrix::from_complex(Complex64::new(ma.to_complex().norm(), 0.0))
                        .to_complex(),
                )
            }
            ComplexExpr::Neg(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(-ma.to_complex()).to_complex())
            }
            ComplexExpr::Asin(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().asin()).to_complex())
            }
            ComplexExpr::Acos(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().acos()).to_complex())
            }
            ComplexExpr::Atan(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().atan()).to_complex())
            }
            ComplexExpr::Sinh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().sinh()).to_complex())
            }
            ComplexExpr::Cosh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().cosh()).to_complex())
            }
            ComplexExpr::Tanh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().tanh()).to_complex())
            }
            ComplexExpr::Asinh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().asinh()).to_complex())
            }
            ComplexExpr::Acosh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().acosh()).to_complex())
            }
            ComplexExpr::Atanh(a) => {
                let ma = ComplexMatrix::from_complex(a.eval_depth(vars, depth + 1)?);
                Ok(ComplexMatrix::from_complex(ma.to_complex().atanh()).to_complex())
            }
            ComplexExpr::Gamma(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_gamma(z)).to_complex())
            }
            ComplexExpr::BesselJ(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_bessel_j(0.0, z)).to_complex())
            }
            ComplexExpr::Conjugate(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(z.conj()).to_complex())
            }
            ComplexExpr::RealPart(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(Complex64::new(z.re, 0.0)).to_complex())
            }
            ComplexExpr::ImagPart(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(Complex64::new(z.im, 0.0)).to_complex())
            }
            ComplexExpr::Arg(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(Complex64::new(z.arg(), 0.0)).to_complex())
            }
            ComplexExpr::Erf(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_erf(z)).to_complex())
            }
            ComplexExpr::LambertW(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_lambert_w(z)).to_complex())
            }
            ComplexExpr::Zeta(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_zeta(z)).to_complex())
            }
            ComplexExpr::BesselY(a) => {
                let z = a.eval_depth(vars, depth + 1)?;
                Ok(ComplexMatrix::from_complex(complex_bessel_y(0.0, z)).to_complex())
            }
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
            | ComplexExpr::Neg(a)
            | ComplexExpr::Asin(a)
            | ComplexExpr::Acos(a)
            | ComplexExpr::Atan(a)
            | ComplexExpr::Sinh(a)
            | ComplexExpr::Cosh(a)
            | ComplexExpr::Tanh(a)
            | ComplexExpr::Asinh(a)
            | ComplexExpr::Acosh(a)
            | ComplexExpr::Atanh(a)
            | ComplexExpr::Gamma(a)
            | ComplexExpr::BesselJ(a)
            | ComplexExpr::Conjugate(a)
            | ComplexExpr::RealPart(a)
            | ComplexExpr::ImagPart(a)
            | ComplexExpr::Arg(a)
            | ComplexExpr::Erf(a)
            | ComplexExpr::LambertW(a)
            | ComplexExpr::Zeta(a)
            | ComplexExpr::BesselY(a) => a.has_var(target),
        }
    }
}

pub fn parse(input: &str) -> Result<ComplexExpr, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let expr = parse_expr(&tokens, &mut pos, 0)?;
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
                    "sin"
                        | "cos"
                        | "tan"
                        | "exp"
                        | "ln"
                        | "log"
                        | "sqrt"
                        | "abs"
                        | "asin"
                        | "acos"
                        | "atan"
                        | "sinh"
                        | "cosh"
                        | "tanh"
                        | "asinh"
                        | "acosh"
                        | "atanh"
                        | "sec"
                        | "csc"
                        | "cot"
                        | "asec"
                        | "acsc"
                        | "acot"
                        | "gamma"
                        | "bessel_j"
                        | "conj"
                        | "re"
                        | "im"
                        | "arg"
                        | "erf"
                        | "lambert_w"
                        | "zeta"
                        | "bessel_y"
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

const MAX_COMPLEX_DEPTH: usize = 256;

fn check_depth(depth: usize) -> Result<(), String> {
    if depth > MAX_COMPLEX_DEPTH {
        Err("Complex expression is too deeply nested".to_string())
    } else {
        Ok(())
    }
}

fn parse_expr(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ComplexExpr, String> {
    parse_add_sub(tokens, pos, depth)
}

fn parse_add_sub(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ComplexExpr, String> {
    check_depth(depth)?;
    let mut node = parse_mul_div(tokens, pos, depth + 1)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                node = ComplexExpr::Add(
                    Box::new(node),
                    Box::new(parse_mul_div(tokens, pos, depth + 1)?),
                );
            }
            Token::Minus => {
                *pos += 1;
                node = ComplexExpr::Sub(
                    Box::new(node),
                    Box::new(parse_mul_div(tokens, pos, depth + 1)?),
                );
            }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_mul_div(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ComplexExpr, String> {
    check_depth(depth)?;
    let mut node = parse_pow(tokens, pos, depth + 1)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Star => {
                *pos += 1;
                node =
                    ComplexExpr::Mul(Box::new(node), Box::new(parse_pow(tokens, pos, depth + 1)?));
            }
            Token::Slash => {
                *pos += 1;
                node =
                    ComplexExpr::Div(Box::new(node), Box::new(parse_pow(tokens, pos, depth + 1)?));
            }
            _ => break,
        }
    }
    Ok(node)
}

fn parse_pow(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ComplexExpr, String> {
    check_depth(depth)?;
    let mut node = parse_primary(tokens, pos, depth + 1)?;
    if *pos < tokens.len() && tokens[*pos] == Token::Caret {
        *pos += 1;
        // Right-associative
        node = ComplexExpr::Pow(Box::new(node), Box::new(parse_pow(tokens, pos, depth + 1)?));
    }
    Ok(node)
}

fn parse_primary(tokens: &[Token], pos: &mut usize, depth: usize) -> Result<ComplexExpr, String> {
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
                let arg = parse_expr(tokens, pos, depth + 1)?;
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
                    "asin" | "arcsin" => Ok(ComplexExpr::Asin(Box::new(arg))),
                    "acos" | "arccos" => Ok(ComplexExpr::Acos(Box::new(arg))),
                    "atan" | "arctan" => Ok(ComplexExpr::Atan(Box::new(arg))),
                    "sinh" => Ok(ComplexExpr::Sinh(Box::new(arg))),
                    "cosh" => Ok(ComplexExpr::Cosh(Box::new(arg))),
                    "tanh" => Ok(ComplexExpr::Tanh(Box::new(arg))),
                    "asinh" | "arcsinh" => Ok(ComplexExpr::Asinh(Box::new(arg))),
                    "acosh" | "arccosh" => Ok(ComplexExpr::Acosh(Box::new(arg))),
                    "atanh" | "arctanh" => Ok(ComplexExpr::Atanh(Box::new(arg))),
                    "sec" => Ok(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(ComplexExpr::Cos(Box::new(arg))),
                    )),
                    "csc" => Ok(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(ComplexExpr::Sin(Box::new(arg))),
                    )),
                    "cot" => Ok(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(ComplexExpr::Tan(Box::new(arg))),
                    )),
                    "asec" | "arcsec" => Ok(ComplexExpr::Acos(Box::new(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(arg),
                    )))),
                    "acsc" | "arccsc" => Ok(ComplexExpr::Asin(Box::new(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(arg),
                    )))),
                    "acot" | "arccot" => Ok(ComplexExpr::Atan(Box::new(ComplexExpr::Div(
                        Box::new(ComplexExpr::Const(Complex64::new(1.0, 0.0))),
                        Box::new(arg),
                    )))),
                    "gamma" => Ok(ComplexExpr::Gamma(Box::new(arg))),
                    "bessel_j" => Ok(ComplexExpr::BesselJ(Box::new(arg))),
                    "conj" => Ok(ComplexExpr::Conjugate(Box::new(arg))),
                    "re" => Ok(ComplexExpr::RealPart(Box::new(arg))),
                    "im" => Ok(ComplexExpr::ImagPart(Box::new(arg))),
                    "arg" => Ok(ComplexExpr::Arg(Box::new(arg))),
                    "erf" => Ok(ComplexExpr::Erf(Box::new(arg))),
                    "lambert_w" => Ok(ComplexExpr::LambertW(Box::new(arg))),
                    "zeta" => Ok(ComplexExpr::Zeta(Box::new(arg))),
                    "bessel_y" => Ok(ComplexExpr::BesselY(Box::new(arg))),
                    _ => Err(format!("Unknown function: {}", name)),
                }
            } else {
                Ok(ComplexExpr::Var(name.clone()))
            }
        }
        Token::Minus => {
            *pos += 1;
            Ok(ComplexExpr::Neg(Box::new(parse_primary(
                tokens,
                pos,
                depth + 1,
            )?)))
        }
        Token::LParen => {
            *pos += 1;
            let expr = parse_expr(tokens, pos, depth + 1)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                return Err("Expected ')'".to_string());
            }
            *pos += 1;
            Ok(expr)
        }
        t => Err(format!("Unexpected token: {:?}", t)),
    }
}

/// Aproximación de la función Gamma compleja usando la fórmula de Lanczos.
///
/// Γ(z) = sqrt(2π) * (z + g - 0.5)^(z - 0.5) * e^(-(z + g - 0.5)) * Σ c_k / (z + k)
///
/// Implementación basada en el algoritmo de Lanczos con g=7 y 9 coeficientes,
/// que da ~15 dígitos de precisión para Re(z) > 0. Para Re(z) < 0 usa la
/// fórmula de reflexión: Γ(z) = π / (sin(πz) * Γ(1-z)).
fn complex_gamma(z: Complex64) -> Complex64 {
    // Coeficientes de Lanczos (g=7, n=9)
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];

    if z.re < 0.5 {
        // Fórmula de reflexión
        let pi = std::f64::consts::PI;
        let one_minus_z = Complex64::new(1.0, 0.0) - z;
        let sin_pi_z = (pi * z).sin();
        let gamma_one_minus_z = complex_gamma(one_minus_z);
        let denom = sin_pi_z * gamma_one_minus_z;
        if denom.norm() < 1e-300 {
            return Complex64::new(f64::NAN, f64::NAN);
        }
        return pi / denom;
    }

    let z_minus_1 = z - Complex64::new(1.0, 0.0);
    let mut sum = Complex64::new(C[0], 0.0);
    for (k, &c) in C.iter().enumerate().skip(1) {
        let denom = z_minus_1 + Complex64::new(k as f64, 0.0);
        if denom.norm() < 1e-300 {
            return Complex64::new(f64::NAN, f64::NAN);
        }
        sum += Complex64::new(c, 0.0) / denom;
    }

    let z_plus_g_minus_half = z_minus_1 + Complex64::new(G - 0.5, 0.0);
    let sqrt_2pi = (2.0 * std::f64::consts::PI).sqrt();

    // (z + g - 0.5)^(z - 0.5)
    let exponent = z_minus_1 + Complex64::new(0.5, 0.0);
    let power = z_plus_g_minus_half.powc(exponent);

    // e^(-(z + g - 0.5))
    let exp_term = (-z_plus_g_minus_half).exp();

    let result = Complex64::new(sqrt_2pi, 0.0) * power * exp_term * sum;

    if result.re.is_finite() && result.im.is_finite() {
        result
    } else {
        Complex64::new(f64::NAN, f64::NAN)
    }
}

/// Función de Bessel de primera clase J_n(z) para orden n=0 por defecto.
///
/// Usa la representación en series de potencias:
/// J_n(z) = Σ_{k=0}^∞ (-1)^k / (k! * Γ(k + n + 1)) * (z/2)^(2k + n)
///
/// Para el caso general con n arbitrario, se usa la fórmula integral:
/// J_n(z) = (1/2π) ∫_{-π}^{π} e^{i(nτ - z sin(τ))} dτ
fn complex_bessel_j(n: f64, z: Complex64) -> Complex64 {
    // Para z pequeño, usar la serie de potencias (más precisa)
    if z.norm() < 20.0 {
        let half_z = z * 0.5;
        let mut k = 0u32;
        // term_0 = (z/2)^n / Γ(n+1)
        let gamma_n_plus_1 = complex_gamma(Complex64::new(n + 1.0, 0.0));
        if gamma_n_plus_1.norm() < 1e-300 {
            return Complex64::new(f64::NAN, f64::NAN);
        }
        let mut term = half_z.powc(Complex64::new(n, 0.0)) / gamma_n_plus_1;

        let mut sum = term;
        let half_z_sq = half_z * half_z;

        loop {
            k += 1;
            // term_k = term_{k-1} * (-z²/4) / (k * (k + n))
            let denom = Complex64::new(k as f64 * (k as f64 + n), 0.0);
            if denom.norm() < 1e-300 {
                break;
            }
            term = -term * half_z_sq / denom;
            sum += term;
            if term.norm() < sum.norm() * 1e-16 || k > 200 {
                break;
            }
        }
        return sum;
    }

    // Para z grande, usar la representación integral (Cuadratura trapezoidal)
    let m = 256u32;
    let dtau = 2.0 * std::f64::consts::PI / m as f64;
    let mut integral = Complex64::new(0.0, 0.0);
    for j in 0..m {
        let tau = -std::f64::consts::PI + j as f64 * dtau;
        let exponent = Complex64::new(0.0, n * tau)
            - z * Complex64::new(tau.sin(), 0.0) * Complex64::new(0.0, 1.0);
        integral += exponent.exp();
    }
    integral *= Complex64::new(dtau / (2.0 * std::f64::consts::PI), 0.0);

    if integral.re.is_finite() && integral.im.is_finite() {
        integral
    } else {
        Complex64::new(f64::NAN, f64::NAN)
    }
}

/// Función de error compleja erf(z) usando la representación integral:
/// erf(z) = (2/√π) ∫_0^z e^{-t²} dt
///
/// Para z complejo se usa la fórmula:
/// erf(z) = 1 - e^{-z²} * w(iz)
/// donde w(z) = e^{-z²} * erfc(-iz) es la función de Faddeeva.
///
/// Implementación: series de potencias para |z| < 3, fracción continua para |z| >= 3.
fn complex_erf(z: Complex64) -> Complex64 {
    let sqrt_pi = std::f64::consts::PI.sqrt();
    let z_norm = z.norm();

    if z_norm < 3.0 {
        // Series: erf(z) = (2/√π) * Σ_{n=0}^∞ (-1)^n * z^{2n+1} / (n! * (2n+1))
        let z_sq = z * z;
        let mut term = z;
        let mut sum = term;
        let mut n = 1u32;
        loop {
            term = -term * z_sq / Complex64::new(n as f64, 0.0);
            let next = term / Complex64::new(2.0 * n as f64 + 1.0, 0.0);
            sum += next;
            if next.norm() < sum.norm() * 1e-16 || n > 100 {
                break;
            }
            n += 1;
        }
        return sum * Complex64::new(2.0 / sqrt_pi, 0.0);
    }

    // Para |z| >= 3: usar la fórmula asintótica
    // erf(z) ≈ 1 - e^{-z²} / (√π * z) para Re(z) > 0
    // erf(z) ≈ -1 + e^{-z²} / (√π * z) para Re(z) < 0 (grandes)
    if z.re.abs() > 1.0 {
        let exp_term = (-z * z).exp();
        let one = Complex64::new(1.0, 0.0);
        let sign = if z.re >= 0.0 { 1.0 } else { -1.0 };
        let result =
            sign * one - exp_term / (Complex64::new(sqrt_pi, 0.0) * z) * Complex64::new(sign, 0.0);
        if result.re.is_finite() && result.im.is_finite() {
            return result;
        }
    }

    // Fallback: integración numérica en línea recta de 0 a z
    let n = 200u32;
    let dt = z / Complex64::new(n as f64, 0.0);
    let mut integral = Complex64::new(0.0, 0.0);
    for k in 0..n {
        let t = dt * Complex64::new(k as f64 + 0.5, 0.0);
        integral += (-t * t).exp();
    }
    let result = integral * dt * Complex64::new(2.0 / sqrt_pi, 0.0);
    if result.re.is_finite() && result.im.is_finite() {
        result
    } else {
        Complex64::new(f64::NAN, f64::NAN)
    }
}

/// Función de Lambert W (rama principal W_0) usando iteración de Newton.
/// W(z) es la solución de w * e^w = z.
fn complex_lambert_w(z: Complex64) -> Complex64 {
    // Estimación inicial
    let mut w = if z.norm() < 2.7 {
        // Cerca del origen: w ≈ z - z² + 1.5*z³
        z - z * z + z * z * z * Complex64::new(1.5, 0.0)
    } else {
        // Lejos del origen: w ≈ ln(z) - ln(ln(z))
        let ln_z = z.ln();
        ln_z - ln_z.ln()
    };

    // Iteración de Newton: w_{n+1} = w_n - (w_n * e^{w_n} - z) / (e^{w_n} * (1 + w_n))
    for _ in 0..50 {
        let exp_w = w.exp();
        let numer = w * exp_w - z;
        let denom = exp_w * (Complex64::new(1.0, 0.0) + w);
        if denom.norm() < 1e-300 {
            break;
        }
        let delta = numer / denom;
        w -= delta;
        if delta.norm() < w.norm() * 1e-16 {
            break;
        }
    }

    if w.re.is_finite() && w.im.is_finite() {
        w
    } else {
        Complex64::new(f64::NAN, f64::NAN)
    }
}

/// Función zeta de Riemann usando la fórmula de Borwein (convergencia acelerada).
/// ζ(s) = 1/(1 - 2^{1-s}) * Σ_{k=0}^∞ [ 1/2^{k+1} * Σ_{j=0}^k (-1)^j * C(k,j) * (j+1)^{-s} ]
fn complex_zeta(s: Complex64) -> Complex64 {
    // Para s con Re(s) < 0, usar la ecuación funcional
    if s.re < 0.0 {
        // Ecuación funcional: ζ(s) = 2^s * π^{s-1} * sin(πs/2) * Γ(1-s) * ζ(1-s)
        let one = Complex64::new(1.0, 0.0);
        let one_minus_s = one - s;
        let zeta_one_minus = complex_zeta(one_minus_s);
        let pi = std::f64::consts::PI;
        let two_pow_s = Complex64::new(2.0, 0.0).powc(s);
        let pi_pow_sm1 = Complex64::new(pi, 0.0).powc(s - one);
        let sin_half = (s * Complex64::new(pi / 2.0, 0.0)).sin();
        let gamma_term = complex_gamma(one_minus_s);
        let result = two_pow_s * pi_pow_sm1 * sin_half * gamma_term * zeta_one_minus;
        if result.re.is_finite() && result.im.is_finite() {
            return result;
        }
        return Complex64::new(f64::NAN, f64::NAN);
    }

    // Fórmula de Borwein (orden N=20)
    let n_terms = 20usize;
    let one = Complex64::new(1.0, 0.0);
    let two = Complex64::new(2.0, 0.0);
    let two_pow_1_minus_s = two.powc(one - s);
    let factor = one / (one - two_pow_1_minus_s);

    // Coeficientes de Borwein d_k
    let d_k = |k: usize| -> f64 {
        let mut sum = 0.0;
        for j in 0..=k {
            let binom = comb(k, j) as f64;
            sum += (-1.0f64).powi(j as i32) * binom * ((j + 1) as f64).powf(-1.0);
        }
        sum / 2.0f64.powi((k + 1) as i32)
    };

    let mut sum = Complex64::new(0.0, 0.0);
    for k in 0..n_terms {
        let d = d_k(k);
        let term = Complex64::new(d, 0.0) * (Complex64::new((k + 1) as f64, 0.0)).powc(-s);
        sum += term;
        if term.norm() < sum.norm() * 1e-16 {
            break;
        }
    }

    let result = factor * sum;
    if result.re.is_finite() && result.im.is_finite() {
        result
    } else {
        Complex64::new(f64::NAN, f64::NAN)
    }
}

/// Coeficiente binomial C(n, k)
fn comb(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result: u64 = 1;
    for i in 0..k {
        result = result * (n - i) as u64 / (i + 1) as u64;
    }
    result
}

/// Función de Bessel de segunda clase Y_n(z) para orden n=0.
/// Y_n(z) = (J_n(z) * cos(nπ) - J_{-n}(z)) / sin(nπ)
/// Para n entero, se usa el límite:
/// Y_0(z) = (2/π) * (ln(z/2) + γ) * J_0(z) - (2/π) * Σ_{k=1}^∞ (-1)^k * H_k / (k!)² * (z/2)^{2k}
/// donde γ es la constante de Euler-Mascheroni y H_k es el k-ésimo número armónico.
fn complex_bessel_y(n: f64, z: Complex64) -> Complex64 {
    let pi = std::f64::consts::PI;
    let euler_gamma = 0.5772156649015329;

    // Para n=0, usar la serie con logaritmo
    if n.abs() < 1e-10 {
        let half_z = z * 0.5;
        let ln_half_z = half_z.ln();
        let gamma_const = Complex64::new(euler_gamma, 0.0);

        // J_0(z) usando serie
        let j0 = complex_bessel_j(0.0, z);

        // Serie de corrección: Σ_{k=1}^∞ (-1)^k * H_k / (k!)² * (z/2)^{2k}
        let half_z_sq = half_z * half_z;
        let mut term = Complex64::new(1.0, 0.0);
        let mut harmonic = 1.0; // H_1 = 1
        let mut sum = Complex64::new(0.0, 0.0);

        for k in 1..=100 {
            term = -term * half_z_sq / Complex64::new(k as f64 * k as f64, 0.0);
            let correction = term * Complex64::new(harmonic, 0.0);
            sum += correction;

            harmonic += 1.0 / (k as f64 + 1.0);

            if correction.norm() < sum.norm() * 1e-16 {
                break;
            }
        }

        let result = Complex64::new(2.0 / pi, 0.0) * ((ln_half_z + gamma_const) * j0 - sum);

        if result.re.is_finite() && result.im.is_finite() {
            return result;
        }
        return Complex64::new(f64::NAN, f64::NAN);
    }

    // Para n != 0, usar la relación Y_n = (J_n * cos(nπ) - J_{-n}) / sin(nπ)
    let j_n = complex_bessel_j(n, z);
    let j_minus_n = complex_bessel_j(-n, z);
    let cos_np = Complex64::new((n * pi).cos(), 0.0);
    let sin_np = (n * pi).sin();

    if sin_np.abs() < 1e-10 {
        // Caso degenerado, usar integración numérica
        return Complex64::new(f64::NAN, f64::NAN);
    }

    let result = (j_n * cos_np - j_minus_n) / Complex64::new(sin_np, 0.0);
    if result.re.is_finite() && result.im.is_finite() {
        result
    } else {
        Complex64::new(f64::NAN, f64::NAN)
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
