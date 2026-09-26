//! Motor LaTeX (subset honesto, bidireccional).
//!
//! Traduce entre la expresión canónica de Grafito (la que acepta
//! [`crate::ast::parse_ast`], p. ej. `sin(x) + (x + 1)/(x - 1)`) y un subset
//! útil de LaTeX. Lo implementan los comandos `ParseLatex` y `ToLatex`
//! (`grafito-command`): este módulo es el motor, `commands.rs` solo lo cablea.
//!
//! ## API
//!
//! ```text
//! to_latex("x^2 + (x + 1)/(x - 1)")  ->  "x^{2} + \frac{x + 1}{x - 1}"
//! to_latex(&ast)                     ->  String  (acepta &str, String y &Expr)
//! parse_latex("\frac{1}{2} + \pi")   ->  Ok("(1)/(2) + pi")
//! parse_latex("\int x dx")           ->  Err("... no está soportado ...")
//! ```
//!
//! ## Subset soportado
//!
//! - Fracciones: `\frac{a}{b}` (también `\dfrac`, `\tfrac`, `\cfrac`).
//! - Raíces: `\sqrt{x}`, `\sqrt[3]{x}` (raíz cúbica → `cbrt(x)`).
//! - Potencias y subíndices: `x^{2}`, `x^2`, `x_{1}` → `x_1`
//!   (un solo nivel; `x_{i+1}` da error honesto).
//! - Multiplicación: `\cdot`, `\times`, `\ast` y yuxtaposición
//!   (`2x` → `2*x`; las letras pegadas forman una variable: `xy` → `xy`).
//! - División: `\div` → `/`.
//! - Funciones con comando propio: `\sin \cos \tan \arcsin \arccos \arctan`
//!   (más alias `\asin \acos \atan \sen`), `\sinh \cosh \tanh`,
//!   `\asinh \acosh \atanh`, `\sec \csc \cot`, `\exp \ln \log`,
//!   `\min \max`, `\arg`, `\Re \Im` (→ `re`/`im`).
//! - Todo lo demás (p. ej. `gamma`, `erf`, `atan2`, `besselj`, `sum`,
//!   `piecewise`) via `\operatorname{nombre}(args)` con el nombre canónico.
//! - Griegas: `\pi \tau \theta \alpha \beta \gamma \delta \lambda \mu \sigma`
//!   `\phi \omega \Delta ...` → `pi tau theta ...`.
//! - Comparaciones: `\leq \geq \neq` (más `\lt \gt \le \ge \ne`) →
//!   `<= >= != < >`; `=` solo se lee como `==`.
//! - Delimitadores: `|x|` o `\lvert x \rvert` → `abs(x)`,
//!   `\lfloor x \rfloor` → `floor(x)`, `\lceil x \rceil` → `ceil(x)`.
//! - Ruido perdonado: `$...$`, `$$...$$`, `\(...\)`, `\[...\]` envolventes,
//!   `\left \right` y espaciados (`\, \; \: \!`, `\quad`, `\ `).
//!
//! Fuera de esto el parser NO adivina: devuelve un error en rioplatense que
//! nombra al culpable (`\int`, `\begin`, `\infty`, doble subíndice...).
//!
//! ## Decisiones que conviene conocer
//!
//! - `to_latex` recibe texto canónico o `&Expr` (trait [`ToLatexInput`]) y
//!   devuelve `String` sin `Result`: si el texto no parsea, devuelve el texto
//!   tal cual (documentado en la función).
//! - `pi`, `tau` y `e` se detectan por igualdad exacta de bits (el parser
//!   canónico los convierte en constantes numéricas): un `3.14` aproximado
//!   NO se vuelve `\pi`.
//! - `\sin^2(x)` da error a propósito: escribí `(\sin(x))^{2}`.
//! - `e^{x}` se lee como `e^x` (potencia de la constante), no como `exp(x)`:
//!   para la exponencial usá `\exp(x)`.

use crate::ast::Expr;

/// Largo máximo aceptado (espeja `crate::expr::MAX_EXPR_LENGTH`).
const MAX_LATEX_INPUT_BYTES: usize = crate::expr::MAX_EXPR_LENGTH;

/// Anidado máximo del descenso recursivo (espeja `MAX_EXPR_NESTING`).
const MAX_LATEX_DEPTH: usize = crate::expr::MAX_EXPR_NESTING;

/// Entrada válida para [`to_latex`]: texto canónico o AST ya parseado.
///
/// Existe para que los dos cableados posibles compilen sin cambios:
/// `to_latex("x^2")` y `to_latex(&ast)` valen lo mismo.
pub trait ToLatexInput {
    /// Renderiza la entrada como LaTeX del subset soportado.
    fn render_latex(&self) -> String;
}

impl ToLatexInput for str {
    fn render_latex(&self) -> String {
        latex_from_canonical(self)
    }
}

impl ToLatexInput for String {
    fn render_latex(&self) -> String {
        latex_from_canonical(self.as_str())
    }
}

impl ToLatexInput for &str {
    fn render_latex(&self) -> String {
        latex_from_canonical(self)
    }
}

impl ToLatexInput for &String {
    fn render_latex(&self) -> String {
        latex_from_canonical(self.as_str())
    }
}

impl ToLatexInput for Expr {
    fn render_latex(&self) -> String {
        to_latex_expr(self)
    }
}

impl ToLatexInput for &Expr {
    fn render_latex(&self) -> String {
        to_latex_expr(self)
    }
}

/// Expresión canónica (o AST) a LaTeX.
///
/// Acepta `&str`, `String` y `&Expr` vía [`ToLatexInput`]. Si el texto no
/// parsea con [`crate::ast::parse_ast`] (o excede el presupuesto), devuelve el
/// texto recortado tal cual en vez de inventar: el comando que muestra el
/// resultado nunca miente sobre una traducción que no pudo hacer.
pub fn to_latex(input: impl ToLatexInput) -> String {
    input.render_latex()
}

/// Renderiza un AST como LaTeX del subset. Núcleo de [`to_latex`].
pub fn to_latex_expr(expr: &Expr) -> String {
    match expr {
        Expr::Const(c) => render_const(*c),
        Expr::Var(v) => render_var(v),
        Expr::Add(a, b) => format!("{} + {}", grouped(a, 1), grouped(b, 2)),
        Expr::Sub(a, b) => format!("{} - {}", grouped(a, 1), grouped(b, 2)),
        Expr::Mul(a, b) => format!(r"{} \cdot {}", factor(a, 2), factor(b, 3)),
        Expr::Div(a, b) => format!(r"\frac{{{}}}{{{}}}", to_latex_expr(a), to_latex_expr(b)),
        Expr::Pow(base, exp) => {
            let rendered = to_latex_expr(base);
            // La base se parenthesiza salvo átomo o grupo único ya cerrado:
            // `\cos(x)^{2}` sería rechazado al reingresar (ambiguo) y
            // `(a)/(b)^{2}` cambiaría el sentido. Sale `(f(x))^{2}`.
            let base_text = if is_atomic_text(&rendered) || is_single_group(&rendered) {
                rendered
            } else {
                format!("({rendered})")
            };
            format!("{base_text}^{{{}}}", to_latex_expr(exp))
        }
        Expr::Neg(u) => {
            let inner = to_latex_expr(u);
            if prec(u) < 2 {
                format!("-({inner})")
            } else {
                format!("-{inner}")
            }
        }
        Expr::Lt(a, b) => format!("{} < {}", grouped(a, 1), grouped(b, 1)),
        Expr::Gt(a, b) => format!("{} > {}", grouped(a, 1), grouped(b, 1)),
        Expr::Le(a, b) => format!(r"{} \leq {}", grouped(a, 1), grouped(b, 1)),
        Expr::Ge(a, b) => format!(r"{} \geq {}", grouped(a, 1), grouped(b, 1)),
        Expr::Eq(a, b) => format!("{} = {}", grouped(a, 1), grouped(b, 1)),
        Expr::Ne(a, b) => format!(r"{} \neq {}", grouped(a, 1), grouped(b, 1)),
        Expr::Sin(u) => latex_call("sin", u),
        Expr::Cos(u) => latex_call("cos", u),
        Expr::Tan(u) => latex_call("tan", u),
        Expr::Asin(u) => latex_call("arcsin", u),
        Expr::Acos(u) => latex_call("arccos", u),
        Expr::Atan(u) => latex_call("arctan", u),
        Expr::Exp(u) => latex_call("exp", u),
        Expr::Ln(u) => latex_call("ln", u),
        Expr::Log(u) => latex_call("log", u),
        Expr::Sqrt(u) => format!(r"\sqrt{{{}}}", to_latex_expr(u)),
        Expr::Abs(u) => format!("|{}|", to_latex_expr(u)),
        Expr::Sinh(u) => latex_call("sinh", u),
        Expr::Cosh(u) => latex_call("cosh", u),
        Expr::Tanh(u) => latex_call("tanh", u),
        Expr::Floor(u) => format!(r"\lfloor {} \rfloor", to_latex_expr(u)),
        Expr::Ceil(u) => format!(r"\lceil {} \rceil", to_latex_expr(u)),
        Expr::Sec(u) => latex_call("sec", u),
        Expr::Csc(u) => latex_call("csc", u),
        Expr::Cot(u) => latex_call("cot", u),
        Expr::Cbrt(u) => format!(r"\sqrt[3]{{{}}}", to_latex_expr(u)),
        Expr::Min(a, b) => format!(r"\min({}, {})", to_latex_expr(a), to_latex_expr(b)),
        Expr::Max(a, b) => format!(r"\max({}, {})", to_latex_expr(a), to_latex_expr(b)),
        Expr::Asinh(u) => operatorname("asinh", &[u]),
        Expr::Acosh(u) => operatorname("acosh", &[u]),
        Expr::Atanh(u) => operatorname("atanh", &[u]),
        Expr::Sign(u) => operatorname("sign", &[u]),
        Expr::Heaviside(u) => operatorname("heaviside", &[u]),
        Expr::Round(u) => operatorname("round", &[u]),
        Expr::Re(u) => operatorname("re", &[u]),
        Expr::Im(u) => operatorname("im", &[u]),
        Expr::Arg(u) => operatorname("arg", &[u]),
        Expr::Conj(u) => operatorname("conj", &[u]),
        Expr::Erf(u) => operatorname("erf", &[u]),
        Expr::Erfc(u) => operatorname("erfc", &[u]),
        Expr::Gamma(u) => operatorname("gamma", &[u]),
        Expr::LnGamma(u) => operatorname("lngamma", &[u]),
        Expr::Digamma(u) => operatorname("digamma", &[u]),
        Expr::Trigamma(u) => operatorname("trigamma", &[u]),
        Expr::Atan2(a, b) => operatorname2("atan2", a, b),
        Expr::Modulo(a, b) => operatorname2("mod", a, b),
        Expr::Beta(a, b) => operatorname2("beta", a, b),
        Expr::BesselJ(n, u) => operatorname2("besselj", n, u),
        Expr::BesselY(n, u) => operatorname2("bessely", n, u),
        Expr::BesselI(n, u) => operatorname2("besseli", n, u),
        Expr::Clamp(x, lo, hi) => format!(
            r"\operatorname{{clamp}}({}, {}, {})",
            to_latex_expr(x),
            to_latex_expr(lo),
            to_latex_expr(hi)
        ),
        Expr::Sum(body, var, start, end) => format!(
            r"\operatorname{{sum}}({}, {var}, {}, {})",
            to_latex_expr(body),
            to_latex_expr(start),
            to_latex_expr(end)
        ),
        Expr::Product(body, var, start, end) => format!(
            r"\operatorname{{product}}({}, {var}, {}, {})",
            to_latex_expr(body),
            to_latex_expr(start),
            to_latex_expr(end)
        ),
        Expr::Piecewise(pieces, default) => {
            let mut args: Vec<String> = Vec::with_capacity(pieces.len() * 2 + 1);
            for (cond, val) in pieces {
                args.push(to_latex_expr(cond));
                args.push(to_latex_expr(val));
            }
            args.push(to_latex_expr(default));
            format!(r"\operatorname{{piecewise}}({})", args.join(", "))
        }
    }
}

/// LaTeX (subset soportado) a expresión canónica.
///
/// Devuelve el texto canónico listo para [`crate::ast::parse_ast`] (p. ej.
/// `\frac{1}{2}` → `(1)/(2)`). El resultado se valida contra el parser
/// canónico antes de salir: si algo no cierra, el error lo dice en
/// rioplatense y nombra al culpable en vez de adivinar.
pub fn parse_latex(latex: &str) -> Result<String, String> {
    let mut text = latex.trim().to_string();
    if text.is_empty() {
        return Err("ParseLatex: entrada vacía, pasame algo como `\\frac{1}{2}`".into());
    }
    if text.len() > MAX_LATEX_INPUT_BYTES {
        return Err(format!(
            "ParseLatex: entrada de {} bytes excede el máximo {MAX_LATEX_INPUT_BYTES}",
            text.len()
        ));
    }
    strip_math_wrappers(&mut text);
    preprocess_delimiters(&mut text);
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    let out = parser.parse_cmp(0)?;
    parser.skip_spaces();
    if let Some(rest) = parser.peek() {
        return Err(parser.err(format!(
            "«{rest}» sobra al final: fijate que los paréntesis y llaves cierren bien"
        )));
    }
    match crate::ast::parse_ast(&out) {
        Ok(_) => Ok(out),
        Err(detail) => Err(format!(
            "ParseLatex: la traducción («{out}») no pasó el parser canónico ({detail}): avisá que hay un bug en el motor LaTeX"
        )),
    }
}

// ── Render helpers ───────────────────────────────────────────────────────────

/// Constantes exactas `pi`/`tau`/`e` por bits (el parser canónico las pliega a
/// número, así se recuperan); un `3.14` aproximado NO se vuelve `\pi`.
fn render_const(value: f64) -> String {
    if value.to_bits() == std::f64::consts::PI.to_bits() {
        return r"\pi".to_string();
    }
    if value.to_bits() == std::f64::consts::TAU.to_bits() {
        return r"\tau".to_string();
    }
    if value.to_bits() == std::f64::consts::E.to_bits() {
        return "e".to_string();
    }
    if !value.is_finite() {
        // NaN/inf no tienen forma canónica finita: se emiten tal cual y
        // `parse_latex` los rechaza con error honesto al reingresar.
        return format!("{value:?}");
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value:?}")
    }
}

/// Nombre griego (ascii o unicode) → comando LaTeX.
fn greek_command(name: &str) -> Option<&'static str> {
    match name {
        "alpha" | "α" => Some(r"\alpha"),
        "beta" | "β" => Some(r"\beta"),
        "gamma" | "γ" => Some(r"\gamma"),
        "delta" | "δ" => Some(r"\delta"),
        "epsilon" | "ε" => Some(r"\epsilon"),
        "zeta" | "ζ" => Some(r"\zeta"),
        "eta" | "η" => Some(r"\eta"),
        "theta" | "θ" => Some(r"\theta"),
        "iota" | "ι" => Some(r"\iota"),
        "kappa" | "κ" => Some(r"\kappa"),
        "lambda" | "λ" => Some(r"\lambda"),
        "mu" | "μ" => Some(r"\mu"),
        "nu" | "ν" => Some(r"\nu"),
        "xi" | "ξ" => Some(r"\xi"),
        "pi" | "π" => Some(r"\pi"),
        "rho" | "ρ" => Some(r"\rho"),
        "sigma" | "σ" => Some(r"\sigma"),
        "tau" | "τ" => Some(r"\tau"),
        "phi" | "φ" => Some(r"\phi"),
        "chi" | "χ" => Some(r"\chi"),
        "psi" | "ψ" => Some(r"\psi"),
        "omega" | "ω" => Some(r"\omega"),
        "Gamma" | "Γ" => Some(r"\Gamma"),
        "Delta" | "Δ" => Some(r"\Delta"),
        "Theta" | "Θ" => Some(r"\Theta"),
        "Lambda" | "Λ" => Some(r"\Lambda"),
        "Xi" | "Ξ" => Some(r"\Xi"),
        "Sigma" | "Σ" => Some(r"\Sigma"),
        "Phi" | "Φ" => Some(r"\Phi"),
        "Psi" | "Ψ" => Some(r"\Psi"),
        "Omega" | "Ω" => Some(r"\Omega"),
        _ => None,
    }
}

/// Variable a LaTeX: griegas a comando, `x_1` a `x_{1}`.
fn render_var(name: &str) -> String {
    if let Some(cmd) = greek_command(name) {
        return cmd.to_string();
    }
    if let Some((base, sub)) = name.split_once('_') {
        let base_text = greek_command(base).unwrap_or(base);
        if !base.is_empty()
            && !sub.is_empty()
            && sub.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return format!("{base_text}_{{{sub}}}");
        }
    }
    name.to_string()
}

/// Precedencia espejo de `to_expr_string_paren` (ast.rs).
fn prec(expr: &Expr) -> u8 {
    match expr {
        Expr::Lt(..) | Expr::Gt(..) | Expr::Le(..) | Expr::Ge(..) | Expr::Eq(..) | Expr::Ne(..) => {
            0
        }
        Expr::Add(..) | Expr::Sub(..) => 1,
        Expr::Mul(..) | Expr::Div(..) => 2,
        Expr::Neg(..) => 3,
        Expr::Const(c) if c.is_sign_negative() => 3,
        Expr::Pow(..) => 4,
        _ => 10,
    }
}

/// Factor de producto con paréntesis si es negación o constante negativa:
/// `-(x + 1) \cdot 2` se releería como `-((x + 1) \cdot 2)` (el lector liga el
/// `-` inicial a todo el producto) y rompería el roundtrip estructural.
fn factor(expr: &Expr, min_prec: u8) -> String {
    match expr {
        Expr::Neg(_) => format!("({})", to_latex_expr(expr)),
        Expr::Const(c) if c.is_sign_negative() => format!("({})", to_latex_expr(expr)),
        _ => grouped(expr, min_prec),
    }
}

/// Hijo con paréntesis si su precedencia no alcanza. `\frac` se exime porque
/// sus llaves ya agrupan (`\frac{a}{b} \cdot c` no necesita paréntesis).
fn grouped(expr: &Expr, min_prec: u8) -> String {
    let rendered = to_latex_expr(expr);
    if prec(expr) < min_prec && !matches!(expr, Expr::Div(..)) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn latex_call(cmd: &str, arg: &Expr) -> String {
    format!("\\{cmd}({})", to_latex_expr(arg))
}

fn operatorname(name: &str, args: &[&Expr]) -> String {
    let rendered: Vec<String> = args.iter().map(|a| to_latex_expr(a)).collect();
    format!(r"\operatorname{{{name}}}({})", rendered.join(", "))
}

fn operatorname2(name: &str, a: &Expr, b: &Expr) -> String {
    format!(
        r"\operatorname{{{name}}}({}, {})",
        to_latex_expr(a),
        to_latex_expr(b)
    )
}

/// Texto canónico → LaTeX (falla devolviendo el texto tal cual).
fn latex_from_canonical(expr: &str) -> String {
    let trimmed = expr.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_LATEX_INPUT_BYTES {
        return trimmed.to_string();
    }
    match crate::ast::parse_ast(trimmed) {
        Ok(ast) => to_latex_expr(&ast),
        Err(_) => trimmed.to_string(),
    }
}

// ── Parser ───────────────────────────────────────────────────────────────────

/// Átomo ya traducido a canónico. `postfix_ok == false` en llamadas
/// (`\sin(x)^2` se rechaza: escribí `(\sin(x))^{2}`).
struct Atom {
    text: String,
    postfix_ok: bool,
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn err(&self, msg: String) -> String {
        format!("ParseLatex: {msg} (posición {})", self.pos)
    }

    /// ¿El input en `pos` empieza con `s`? (por chars, sin partir UTF-8).
    fn starts_with(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(offset, c)| self.chars.get(self.pos + offset).copied() == Some(c))
    }

    fn consume_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.chars().count();
            true
        } else {
            false
        }
    }

    /// Consume `\nombre` con frontera de palabra (que no siga una letra).
    fn try_cmd(&mut self, name: &str) -> bool {
        if self.peek() != Some('\\') {
            return false;
        }
        let mut offset = 1;
        for c in name.chars() {
            if self.chars.get(self.pos + offset).copied() != Some(c) {
                return false;
            }
            offset += 1;
        }
        if self
            .chars
            .get(self.pos + offset)
            .copied()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return false;
        }
        self.pos += offset;
        true
    }

    /// Lee una corrida de letras ASCII (nombre de comando ya sin la barra).
    fn read_cmd_name(&mut self) -> String {
        let mut name = String::new();
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            if let Some(c) = self.peek() {
                name.push(c);
                self.pos += 1;
            }
        }
        name
    }

    /// Lee un identificador (variable o función pelada).
    fn read_ident(&mut self) -> String {
        let mut name = String::new();
        while self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            if let Some(c) = self.peek() {
                name.push(c);
                self.pos += 1;
            }
        }
        name
    }

    // — niveles de precedencia —

    fn parse_cmp(&mut self, depth: usize) -> Result<String, String> {
        if depth > MAX_LATEX_DEPTH {
            return Err(self.err("demasiado anidado, simplificá la expresión".into()));
        }
        let mut out = self.parse_add_sub(depth)?;
        loop {
            self.skip_spaces();
            let op = self.try_cmp_op()?;
            let Some(op) = op else { break };
            let rhs = self.parse_add_sub(depth)?;
            out = format!("{out} {op} {rhs}");
        }
        Ok(out)
    }

    /// Operador de comparación en la posición actual (o `None`).
    fn try_cmp_op(&mut self) -> Result<Option<&'static str>, String> {
        if self.consume_str("<=") {
            return Ok(Some("<="));
        }
        if self.consume_str(">=") {
            return Ok(Some(">="));
        }
        if self.consume_str("==") {
            return Ok(Some("=="));
        }
        if self.consume_str("!=") {
            return Ok(Some("!="));
        }
        if self.consume_str("<") {
            return Ok(Some("<"));
        }
        if self.consume_str(">") {
            return Ok(Some(">"));
        }
        if self.consume_str("=") {
            return Ok(Some("=="));
        }
        for (cmd, op) in [
            ("leq", "<="),
            ("le", "<="),
            ("leqslant", "<="),
            ("geq", ">="),
            ("ge", ">="),
            ("geqslant", ">="),
            ("neq", "!="),
            ("ne", "!="),
            ("lt", "<"),
            ("gt", ">"),
        ] {
            if self.try_cmd(cmd) {
                return Ok(Some(op));
            }
        }
        if self.peek() == Some('\\') {
            let mut probe = Parser {
                chars: self.chars.clone(),
                pos: self.pos,
            };
            probe.pos += 1;
            let name = probe.read_cmd_name();
            if matches!(
                name.as_str(),
                "equiv" | "approx" | "sim" | "simeq" | "ll" | "gg"
            ) {
                return Err(self.err(format!(
                    "«\\{name}» no tiene equivalente canónico, fuera del subset"
                )));
            }
        }
        Ok(None)
    }

    fn parse_add_sub(&mut self, depth: usize) -> Result<String, String> {
        let mut out = self.parse_unary(depth)?;
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    let rhs = self.parse_unary(depth)?;
                    out = format!("{out} + {rhs}");
                }
                Some('-') => {
                    self.pos += 1;
                    let rhs = self.parse_unary(depth)?;
                    out = format!("{out} - {rhs}");
                }
                _ => break,
            }
        }
        Ok(out)
    }

    fn parse_unary(&mut self, depth: usize) -> Result<String, String> {
        let mut neg = false;
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('+') => self.pos += 1,
                Some('-') => {
                    self.pos += 1;
                    neg = !neg;
                }
                _ => break,
            }
        }
        let out = self.parse_mul_div(depth)?;
        if neg {
            Ok(format!("-({out})"))
        } else {
            Ok(out)
        }
    }

    fn parse_mul_div(&mut self, depth: usize) -> Result<String, String> {
        let mut out = self.parse_postfix(depth)?;
        loop {
            self.skip_spaces();
            let op: Option<&str> = if self.consume_str("*") {
                Some("*")
            } else if self.consume_str("/") {
                Some("/")
            } else if self.try_cmd("cdot") || self.try_cmd("times") || self.try_cmd("ast") {
                Some("*")
            } else if self.try_cmd("div") {
                Some("/")
            } else if self.operand_starts() {
                Some("*")
            } else {
                None
            };
            let Some(op) = op else { break };
            let rhs = self.parse_postfix(depth)?;
            out = format!("{out}{op}{rhs}");
        }
        Ok(out)
    }

    /// ¿Empieza un operando acá? (para la multiplicación implícita).
    /// Las letras pegadas ya las absorbió `read_ident`; lo que sigue juxtapuesto
    /// multiplica: `2x`, `2\pi`, `(a)(b)`, `\pi x`.
    fn operand_starts(&self) -> bool {
        match self.peek() {
            Some(c) if c.is_ascii_digit() => true,
            Some(c) if c.is_alphabetic() || c == '_' => true,
            Some('(') | Some('{') | Some('|') => true,
            Some('.') => self
                .chars
                .get(self.pos + 1)
                .copied()
                .is_some_and(|c| c.is_ascii_digit()),
            Some('\\') => {
                let mut probe = Parser {
                    chars: self.chars.clone(),
                    pos: self.pos + 1,
                };
                let name = probe.read_cmd_name();
                is_operand_cmd(&name)
            }
            _ => false,
        }
    }

    fn parse_postfix(&mut self, depth: usize) -> Result<String, String> {
        let atom = self.parse_atom(depth)?;
        let mut text = atom.text;
        let mut sup: Option<String> = None;
        let mut sub: Option<String> = None;
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('^') => {
                    if sup.is_some() {
                        return Err(self.err(
                            "doble potencia sin llaves: usá `x^{2^3}` si es eso lo que querés"
                                .into(),
                        ));
                    }
                    if !atom.postfix_ok {
                        return Err(self.err(
                            "potencia sobre una llamada sin paréntesis: escribí `(\\sin(x))^{2}`"
                                .into(),
                        ));
                    }
                    self.pos += 1;
                    sup = Some(self.parse_sup_content(depth)?);
                }
                Some('_') => {
                    if sub.is_some() {
                        return Err(self.err(
                            "doble subíndice, fuera del subset: usá una sola variable como `x_1`"
                                .into(),
                        ));
                    }
                    if !atom.postfix_ok {
                        return Err(self.err(
                            "subíndice sobre una llamada sin paréntesis, fuera del subset".into(),
                        ));
                    }
                    self.pos += 1;
                    sub = Some(self.parse_sub_content()?);
                }
                _ => break,
            }
        }
        if let Some(s) = sub {
            if text.contains('_') {
                return Err(self.err(
                    "doble subíndice, fuera del subset: usá una sola variable como `x_1`".into(),
                ));
            }
            if !is_plain_var(&text) {
                return Err(self.err(
                    "el subíndice va sobre una variable simple (p. ej. `x_{1}`), no sobre eso"
                        .into(),
                ));
            }
            text = format!("{text}_{s}");
        }
        if let Some(e) = sup {
            text = format!("{}^({e})", parenthesize_pow_base(&text));
        }
        Ok(text)
    }

    /// Contenido de `^{...}` (o átomo pelado) en canónico con paréntesis.
    fn parse_sup_content(&mut self, depth: usize) -> Result<String, String> {
        self.skip_spaces();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let inner = self.parse_cmp(depth + 1)?;
                self.skip_spaces();
                if !self.consume_str("}") {
                    return Err(self.err("falta cerrar la llave del exponente".into()));
                }
                Ok(group_if_needed(&inner))
            }
            _ => {
                let atom = self.parse_atom(depth)?;
                Ok(group_if_needed(&atom.text))
            }
        }
    }

    /// Contenido de `_{...}`: solo letras y dígitos (`x_{12}` sí, `x_{i+1}` no).
    fn parse_sub_content(&mut self) -> Result<String, String> {
        self.skip_spaces();
        match self.peek() {
            Some('{') => {
                self.pos += 1;
                let mut body = String::new();
                loop {
                    match self.peek() {
                        None => {
                            return Err(self.err("falta cerrar la llave del subíndice".into()));
                        }
                        Some('}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(c) => {
                            body.push(c);
                            self.pos += 1;
                        }
                    }
                }
                if body.is_empty() || !is_sub_body(&body) {
                    return Err(self.err(
                        "subíndice con expresión, fuera del subset: usá solo letras y dígitos, p. ej. `x_{12}`"
                            .into(),
                    ));
                }
                Ok(body)
            }
            Some('\\') => {
                self.pos += 1;
                let name = self.read_cmd_name();
                if name.is_empty() {
                    return Err(self.err("se esperaba el subíndice".into()));
                }
                greek_canonical(&name).map_or_else(
                    || {
                        Err(self.err(format!(
                            "«\\{name}» como subíndice, fuera del subset: usá solo letras y dígitos"
                        )))
                    },
                    |canon| Ok(canon.to_string()),
                )
            }
            Some(c) if c.is_ascii_alphanumeric() || c == '_' => {
                self.pos += 1;
                Ok(c.to_string())
            }
            Some(c) if c.is_alphabetic() => {
                self.pos += 1;
                Ok(c.to_string())
            }
            _ => Err(self.err("se esperaba el subíndice (p. ej. `x_1` o `x_{12}`)".into())),
        }
    }

    // — átomos —

    fn parse_atom(&mut self, depth: usize) -> Result<Atom, String> {
        self.skip_spaces();
        match self.peek() {
            None => Err(self.err("expresión incompleta: falta un operando".into())),
            Some('(') => {
                self.pos += 1;
                let inner = self.parse_cmp(depth + 1)?;
                self.skip_spaces();
                if !self.consume_str(")") {
                    return Err(self.err("falta cerrar el paréntesis".into()));
                }
                Ok(Atom {
                    text: format!("({inner})"),
                    postfix_ok: true,
                })
            }
            Some('{') => {
                self.pos += 1;
                let inner = self.parse_cmp(depth + 1)?;
                self.skip_spaces();
                if !self.consume_str("}") {
                    return Err(self.err("falta cerrar la llave".into()));
                }
                Ok(Atom {
                    text: format!("({inner})"),
                    postfix_ok: true,
                })
            }
            Some('|') => {
                self.pos += 1;
                self.parse_abs_inner(depth)
            }
            Some(c) if c.is_ascii_digit() => {
                let literal = self.scan_number()?;
                Ok(Atom {
                    text: literal,
                    postfix_ok: true,
                })
            }
            Some('.') => {
                if self
                    .chars
                    .get(self.pos + 1)
                    .copied()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    let literal = self.scan_number()?;
                    Ok(Atom {
                        text: literal,
                        postfix_ok: true,
                    })
                } else {
                    Err(self.err("punto suelto, fuera del subset".into()))
                }
            }
            Some(c) if c.is_alphabetic() || c == '_' => self.parse_bare_ident(depth),
            Some('\\') => {
                self.pos += 1;
                match self.peek() {
                    Some(c) if c.is_ascii_alphabetic() => {
                        let name = self.read_cmd_name();
                        self.dispatch_cmd(&name, depth)
                    }
                    Some(_) | None => Err(self.err(
                        "ese escape no está en el subset (valen `\\frac`, `\\sqrt`, `\\sin`... y griegas como `\\pi`)"
                            .into(),
                    )),
                }
            }
            Some('^') | Some('_') => Err(self
                .err("ese `^`/`_` no tiene base: poné la variable adelante, p. ej. `x^2`".into())),
            Some(')') | Some(']') => Err(self.err("paréntesis de más: sobra un cierre".into())),
            Some('}') => Err(self.err("llave de más: sobra un cierre".into())),
            Some(',') => {
                Err(self
                    .err("la coma solo vale dentro de una llamada, p. ej. `\\min(a, b)`".into()))
            }
            Some('*') | Some('/') => {
                Err(self.err("falta el operando izquierdo de ese operador".into()))
            }
            Some('+' | '-') => Err(self.err(
                "signo suelto acá: si es un exponente negativo usá llaves, p. ej. `x^{-2}`".into(),
            )),
            Some('[') => {
                Err(self.err("corchetes pelados, fuera del subset: usá paréntesis `(…)`".into()))
            }
            Some('$') => {
                Err(self.err("signos `$` sueltos: envolvé toda la entrada o sacalos".into()))
            }
            Some('&') => {
                Err(self.err("tablas y matrices (`&`, `\\\\`) están fuera del subset".into()))
            }
            _ => Err(self.err("ese caracter está fuera del subset soportado".into())),
        }
    }

    /// `|expr|` → `abs(expr)` (con anidado de paréntesis/llaves respetado).
    /// El `|` de apertura ya viene consumido.
    fn parse_abs_inner(&mut self, depth: usize) -> Result<Atom, String> {
        let mut paren: usize = 0;
        let mut brace: usize = 0;
        let mut index = self.pos;
        let closer = loop {
            match self.chars.get(index).copied() {
                None => break None,
                Some('|') if paren == 0 && brace == 0 => break Some(index),
                Some('(') => paren += 1,
                Some(')') => paren = paren.saturating_sub(1),
                Some('{') => brace += 1,
                Some('}') => brace = brace.saturating_sub(1),
                _ => {}
            }
            index += 1;
        };
        let Some(closer) = closer else {
            return Err(self.err("falta cerrar `|` del valor absoluto".into()));
        };
        let inner_chars: Vec<char> = self.chars[self.pos..closer].to_vec();
        if inner_chars.is_empty() {
            return Err(self.err("valor absoluto vacío `||`".into()));
        }
        let mut inner = Parser {
            chars: inner_chars,
            pos: 0,
        };
        let translated = inner.parse_cmp(depth + 1)?;
        inner.skip_spaces();
        if inner.peek().is_some() {
            return Err(self.err("eso dentro de `|…|` no se entiende".into()));
        }
        self.pos = closer + 1;
        Ok(Atom {
            text: format!("abs({translated})"),
            postfix_ok: true,
        })
    }

    /// Número canónico (entero, decimal o científica `1e-3`).
    fn scan_number(&mut self) -> Result<String, String> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.pos += 1;
            }
            if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            } else {
                self.pos = save; // la `e` era una variable (`2e` → `2*e`)
            }
        }
        let literal: String = self.chars[start..self.pos].iter().collect();
        match literal.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(literal),
            _ => Err(self.err(format!("número inválido «{literal}»"))),
        }
    }

    /// Variable pelada o llamada a función conocida.
    fn parse_bare_ident(&mut self, depth: usize) -> Result<Atom, String> {
        let mut name = self.read_ident();
        // `x_{1}`: read_ident se traga la `_` previa a la llave y el `{`
        // parecería llamada a función; se devuelve para que el parser de
        // subíndices la procese como tal.
        while name.ends_with('_') && matches!(self.peek(), Some('{')) {
            name.pop();
            self.pos -= 1;
        }
        if is_nonfinite_name(&name) {
            return Err(self.err(format!(
                "«{name}» no es finito: las constantes no finitas están fuera del subset"
            )));
        }
        self.skip_spaces();
        let has_args = matches!(self.peek(), Some('(') | Some('{'));
        if has_args {
            if let Some(canon) = canonical_function_name(&name) {
                let text = self.parse_call(&name, canon, depth)?;
                return Ok(Atom {
                    text,
                    postfix_ok: false,
                });
            }
            if name.chars().count() == 1 {
                // `x(y+1)`: la yuxtaposición multiplica, el `(` lo come el nivel mul.
                return Ok(Atom {
                    text: name,
                    postfix_ok: true,
                });
            }
            return Err(self.err(format!(
                "«{name}» no es una función soportada (usá: sin, cos, tan, exp, ln, log, sqrt, abs, min, max...)"
            )));
        }
        if canonical_function_name(&name).is_some() {
            return Err(self.err(format!(
                "«{name}» es una función: escribí `{name}(…)` con el argumento entre paréntesis"
            )));
        }
        Ok(Atom {
            text: name,
            postfix_ok: true,
        })
    }

    /// Llamada `nombre(args)` con chequeo de aridad en rioplatense.
    fn parse_call(&mut self, display: &str, canon: &str, depth: usize) -> Result<String, String> {
        let closer = match self.peek() {
            Some('(') => ')',
            Some('{') => '}',
            _ => {
                return Err(self.err(format!(
                    "«{display}» necesita el argumento entre paréntesis: `{display}(…)`"
                )));
            }
        };
        self.pos += 1;
        let mut args = Vec::new();
        self.skip_spaces();
        if self.peek() == Some(closer) {
            self.pos += 1;
        } else {
            loop {
                args.push(self.parse_cmp(depth + 1)?);
                self.skip_spaces();
                if self.peek() == Some(',') {
                    self.pos += 1;
                    continue;
                }
                if self.peek() == Some(closer) {
                    self.pos += 1;
                    break;
                }
                return Err(self.err(format!("en `{display}(…)` se esperaba `,` o el cierre")));
            }
        }
        check_arity(display, canon, args.len(), self.pos)?;
        Ok(format!("{canon}({})", args.join(", ")))
    }

    // — comandos con barra —

    fn dispatch_cmd(&mut self, name: &str, depth: usize) -> Result<Atom, String> {
        match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.brace_group("frac", depth)?;
                let den = self.brace_group("frac", depth)?;
                Ok(Atom {
                    text: format!("({num})/({den})"),
                    postfix_ok: true,
                })
            }
            "sqrt" => {
                let mut cubic = false;
                self.skip_spaces();
                if self.peek() == Some('[') {
                    self.pos += 1;
                    let mut order = String::new();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(self.err("falta cerrar `[n]` en `\\sqrt[n]`".into()));
                            }
                            Some(']') => {
                                self.pos += 1;
                                break;
                            }
                            Some(c) => {
                                order.push(c);
                                self.pos += 1;
                            }
                        }
                    }
                    if order.trim() == "3" {
                        cubic = true;
                    } else {
                        return Err(self.err(format!(
                            "solo `\\sqrt` y `\\sqrt[3]` están soportados (pediste orden «{}»)",
                            order.trim()
                        )));
                    }
                }
                let inner = self.brace_group("sqrt", depth)?;
                Ok(Atom {
                    text: if cubic {
                        format!("cbrt({inner})")
                    } else {
                        format!("sqrt({inner})")
                    },
                    postfix_ok: true,
                })
            }
            "lvert" => self.parse_abs_inner(depth),
            "rvert" => Err(self.err("sobra un `\\rvert` de cierre".into())),
            "lfloor" => {
                let inner = self.until_closer("rfloor", depth)?;
                Ok(Atom {
                    text: format!("floor({inner})"),
                    postfix_ok: true,
                })
            }
            "rfloor" => Err(self.err("sobra un `\\rfloor` de cierre".into())),
            "lceil" => {
                let inner = self.until_closer("rceil", depth)?;
                Ok(Atom {
                    text: format!("ceil({inner})"),
                    postfix_ok: true,
                })
            }
            "rceil" => Err(self.err("sobra un `\\rceil` de cierre".into())),
            "cdot" | "times" | "ast" | "div" | "pm" | "mp" => {
                Err(self.err("falta el operando izquierdo de ese operador".into()))
            }
            "Re" => self.single_arg_cmd("Re", "re", depth),
            "Im" => self.single_arg_cmd("Im", "im", depth),
            "arg" => self.single_arg_cmd("arg", "arg", depth),
            "min" | "max" => {
                let canon = name.to_string();
                let text = self.parse_call(&format!("\\{name}"), &canon, depth)?;
                Ok(Atom {
                    text,
                    postfix_ok: false,
                })
            }
            "operatorname" => {
                self.skip_spaces();
                if !self.consume_str("{") {
                    return Err(self.err(
                        "usá `\\operatorname{nombre}(…)`, con el nombre entre llaves".into(),
                    ));
                }
                let mut raw = String::new();
                loop {
                    match self.peek() {
                        None => {
                            return Err(
                                self.err("falta cerrar la llave de `\\operatorname`".into())
                            );
                        }
                        Some('}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(c) => {
                            raw.push(c);
                            self.pos += 1;
                        }
                    }
                }
                let requested = raw.trim().to_lowercase();
                let Some(canon) = canonical_function_name(&requested) else {
                    return Err(self.err(format!(
                        "«{requested}» no es una función soportada (usá: sin, cos, exp, gamma, erf, min, sum...)"
                    )));
                };
                let text =
                    self.parse_call(&format!("\\operatorname{{{requested}}}"), canon, depth)?;
                Ok(Atom {
                    text,
                    postfix_ok: false,
                })
            }
            _ => {
                if let Some(canon) = greek_canonical(name) {
                    return Ok(Atom {
                        text: canon.to_string(),
                        postfix_ok: true,
                    });
                }
                if let Some(canon) = canonical_function_name(name) {
                    return self.single_arg_cmd(&format!("\\{name}"), canon, depth);
                }
                Err(unsupported_cmd(name, self.pos))
            }
        }
    }

    /// Grupo `{…}` obligatorio (para `\frac` y `\sqrt`).
    fn brace_group(&mut self, owner: &str, depth: usize) -> Result<String, String> {
        self.skip_spaces();
        if !self.consume_str("{") {
            return Err(self.err(format!(
                "`\\{owner}` necesita llaves: `\\{owner}{{a}}{{b}}`"
            )));
        }
        let inner = self.parse_cmp(depth + 1)?;
        self.skip_spaces();
        if !self.consume_str("}") {
            return Err(self.err(format!("falta cerrar la llave de `\\{owner}`")));
        }
        Ok(inner)
    }

    /// Lee hasta `\rfloor` / `\rceil` (respetando anidado de llaves).
    fn until_closer(&mut self, closer: &str, depth: usize) -> Result<String, String> {
        let start = self.pos;
        let mut brace: usize = 0;
        let end = loop {
            match self.peek() {
                None => {
                    return Err(self.err(format!("falta cerrar con `\\{closer}`")));
                }
                Some('{') => {
                    brace += 1;
                    self.pos += 1;
                }
                Some('}') => {
                    brace = brace.saturating_sub(1);
                    self.pos += 1;
                }
                Some('\\') if brace == 0 => {
                    let mut probe = Parser {
                        chars: self.chars.clone(),
                        pos: self.pos,
                    };
                    if probe.try_cmd(closer) {
                        break self.pos;
                    }
                    self.pos += 1;
                }
                Some(_) => self.pos += 1,
            }
        };
        let inner_chars: Vec<char> = self.chars[start..end].to_vec();
        let mut inner = Parser {
            chars: inner_chars,
            pos: 0,
        };
        let translated = inner.parse_cmp(depth + 1)?;
        inner.skip_spaces();
        if inner.peek().is_some() {
            return Err(self.err("eso de adentro no se entiende".into()));
        }
        let mut probe = Parser {
            chars: self.chars.clone(),
            pos: end,
        };
        probe.try_cmd(closer);
        self.pos = probe.pos;
        Ok(translated)
    }

    /// Una función que toma un átomo (`\sin x`, `\sin{x}`, `\sin(x)`).
    /// La llamada nunca acepta `^`/`_` directo (usá paréntesis).
    fn single_arg_cmd(&mut self, display: &str, canon: &str, depth: usize) -> Result<Atom, String> {
        self.skip_spaces();
        match self.peek() {
            Some('^') | Some('_') => Err(self.err(format!(
                "`{display}` con `^`/`_` directo es ambiguo: escribí `({display}(x))^{{2}}`"
            ))),
            None => Err(self.err(format!("a `{display}` le falta el argumento"))),
            _ => {
                let atom = self.parse_atom(depth)?;
                Ok(Atom {
                    text: format!("{canon}({})", atom.text),
                    postfix_ok: false,
                })
            }
        }
    }
}

// ── Tablas y validación ──────────────────────────────────────────────────────

/// Comando griego → nombre canónico ASCII.
fn greek_canonical(name: &str) -> Option<&'static str> {
    match name {
        "alpha" => Some("alpha"),
        "beta" => Some("beta"),
        "gamma" => Some("gamma"),
        "delta" => Some("delta"),
        "epsilon" | "varepsilon" => Some("epsilon"),
        "zeta" => Some("zeta"),
        "eta" => Some("eta"),
        "theta" | "vartheta" => Some("theta"),
        "iota" => Some("iota"),
        "kappa" => Some("kappa"),
        "lambda" => Some("lambda"),
        "mu" => Some("mu"),
        "nu" => Some("nu"),
        "xi" => Some("xi"),
        "pi" | "varpi" => Some("pi"),
        "rho" | "varrho" => Some("rho"),
        "sigma" | "varsigma" => Some("sigma"),
        "tau" => Some("tau"),
        "phi" | "varphi" => Some("phi"),
        "chi" => Some("chi"),
        "psi" => Some("psi"),
        "omega" => Some("omega"),
        "Gamma" => Some("Gamma"),
        "Delta" => Some("Delta"),
        "Theta" => Some("Theta"),
        "Lambda" => Some("Lambda"),
        "Xi" => Some("Xi"),
        "Sigma" => Some("Sigma"),
        "Phi" => Some("Phi"),
        "Psi" => Some("Psi"),
        "Omega" => Some("Omega"),
        _ => None,
    }
}

/// Alias → nombre canónico de función (espeja el parser de `ast.rs`).
fn canonical_function_name(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "sin" | "sen" => Some("sin"),
        "cos" => Some("cos"),
        "tan" => Some("tan"),
        "asin" | "arcsin" => Some("asin"),
        "acos" | "arccos" => Some("acos"),
        "atan" | "arctan" => Some("atan"),
        "sinh" => Some("sinh"),
        "cosh" => Some("cosh"),
        "tanh" => Some("tanh"),
        "asinh" | "arcsinh" => Some("asinh"),
        "acosh" | "arccosh" => Some("acosh"),
        "atanh" | "arctanh" => Some("atanh"),
        "sec" => Some("sec"),
        "csc" | "cosec" => Some("csc"),
        "cot" | "cotan" => Some("cot"),
        "exp" => Some("exp"),
        "ln" => Some("ln"),
        "log" | "log10" => Some("log"),
        "sqrt" => Some("sqrt"),
        "cbrt" => Some("cbrt"),
        "abs" => Some("abs"),
        "sign" | "signum" => Some("sign"),
        "heaviside" | "step" => Some("heaviside"),
        "floor" => Some("floor"),
        "ceil" | "ceiling" => Some("ceil"),
        "round" => Some("round"),
        "atan2" => Some("atan2"),
        "mod" | "modulo" => Some("mod"),
        "min" => Some("min"),
        "max" => Some("max"),
        "clamp" => Some("clamp"),
        "re" | "real" => Some("re"),
        "im" | "imag" | "imaginary" => Some("im"),
        "arg" | "argument" | "phase" => Some("arg"),
        "conj" | "conjugate" => Some("conj"),
        "erf" => Some("erf"),
        "erfc" => Some("erfc"),
        "gamma" => Some("gamma"),
        "lngamma" | "lgamma" => Some("lngamma"),
        "digamma" => Some("digamma"),
        "trigamma" => Some("trigamma"),
        "beta" => Some("beta"),
        "besselj" => Some("besselj"),
        "bessely" => Some("bessely"),
        "besseli" => Some("besseli"),
        "sum" => Some("sum"),
        "product" | "prod" => Some("product"),
        "piecewise" => Some("piecewise"),
        _ => None,
    }
}

/// Aridad esperada (`None` = `piecewise`, que acepta 1 o más).
fn expected_arity(canon: &str) -> Option<usize> {
    match canon {
        "atan2" | "mod" | "min" | "max" | "beta" | "besselj" | "bessely" | "besseli" => Some(2),
        "clamp" => Some(3),
        "sum" | "product" => Some(4),
        "piecewise" => None,
        _ => Some(1),
    }
}

fn check_arity(display: &str, canon: &str, got: usize, pos: usize) -> Result<(), String> {
    match expected_arity(canon) {
        Some(want) if got != want => Err(format!(
            "ParseLatex: `{display}` lleva {want} argumento(s), le pasaste {got} (posición {pos})"
        )),
        None if got < 1 => Err(format!(
            "ParseLatex: `{display}` necesita al menos 1 argumento (posición {pos})"
        )),
        _ => Ok(()),
    }
}

/// Comandos que pueden abrir un operando (para la yuxtaposición con `*`).
fn is_operand_cmd(name: &str) -> bool {
    matches!(
        name,
        "frac"
            | "dfrac"
            | "tfrac"
            | "cfrac"
            | "sqrt"
            | "sin"
            | "cos"
            | "tan"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "asin"
            | "acos"
            | "atan"
            | "sen"
            | "sinh"
            | "cosh"
            | "tanh"
            | "asinh"
            | "acosh"
            | "atanh"
            | "sec"
            | "csc"
            | "cosec"
            | "cot"
            | "cotan"
            | "exp"
            | "ln"
            | "log"
            | "cbrt"
            | "abs"
            | "floor"
            | "ceil"
            | "round"
            | "min"
            | "max"
            | "arg"
            | "Re"
            | "Im"
            | "lfloor"
            | "lceil"
            | "lvert"
            | "operatorname"
    ) || greek_canonical(name).is_some()
        || canonical_function_name(name).is_some()
}

/// Variable simple (`x`, `theta`, `x_1` ya plegada no; base sin `_` raro).
fn is_plain_var(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Cuerpo de subíndice `{…}`: solo letras y dígitos ASCII.
fn is_sub_body(body: &str) -> bool {
    !body.is_empty() && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Nombres que el parser canónico leería como no-finitos.
fn is_nonfinite_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(lower.as_str(), "nan" | "inf" | "infinity")
}

/// Texto atómico para base de potencia (`2`, `x`, `theta_1`).
fn is_atomic_text(text: &str) -> bool {
    is_plain_var(text) || text.parse::<f64>().is_ok_and(|v: f64| v.is_finite())
}

/// ¿Un solo grupo `(…)` ya cerrado? (`(a+b)` sí, `(a)/(b)` no).
fn is_single_group(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&'(') || chars.last() != Some(&')') {
        return false;
    }
    let mut depth: usize = 0;
    for (i, c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i != chars.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// `base^(exp)`: paréntesis si la base tiene un operador a nivel 0.
fn parenthesize_pow_base(base: &str) -> String {
    if has_top_level_op(base) {
        format!("({base})")
    } else {
        base.to_string()
    }
}

/// ¿Hay `+ - * /` fuera de paréntesis/llaves? (ignora el `-` inicial).
fn has_top_level_op(text: &str) -> bool {
    let mut depth: usize = 0;
    for (i, c) in text.chars().enumerate() {
        match c {
            '(' | '{' => depth += 1,
            ')' | '}' => depth = depth.saturating_sub(1),
            '+' | '*' | '/' if depth == 0 => return true,
            '-' if depth == 0 && i > 0 => {
                // El `-` de la científica (`1e-3`) también parenthesiza:
                // inofensivo y siempre válido.
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Envuelve en paréntesis salvo átomos (`2`, `x`, `sin(x)`, `(a+b)`).
fn group_if_needed(text: &str) -> String {
    if is_plain_var(text) || text.parse::<f64>().is_ok() || has_wrapping_parens(text) {
        text.to_string()
    } else {
        format!("({text})")
    }
}

/// ¿Empieza con `(` (o letra+`(`) y termina con `)`?
fn has_wrapping_parens(text: &str) -> bool {
    let t = text.trim();
    if !t.ends_with(')') {
        return false;
    }
    if t.starts_with('(') {
        return true;
    }
    t.bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && t.contains('(')
}

/// Error honesto para comandos fuera del subset (nombra al culpable).
fn unsupported_cmd(name: &str, pos: usize) -> String {
    match name {
        "int" | "oint" | "iint" | "sum" | "prod" | "lim" | "sup" | "inf" => {
            format!(
                "ParseLatex: `\\{name}` (operadores grandes) está fuera del subset: este motor traduce expresiones, no las evalúa (posición {pos})"
            )
        }
        "begin" | "end" => {
            format!(
                "ParseLatex: entornos (`\\begin…`) y matrices están fuera del subset (posición {pos})"
            )
        }
        "infty" => {
            format!(
                "ParseLatex: `\\infty` está fuera del subset, no hay infinito canónico (posición {pos})"
            )
        }
        "partial" | "nabla" | "mod" => {
            format!("ParseLatex: `\\{name}` está fuera del subset de expresiones (posición {pos})")
        }
        "text" | "mathrm" | "mathbf" | "mathit" | "boldsymbol" | "mbox" | "textbf" => {
            format!(
                "ParseLatex: texto con formato (`\\{name}`) está fuera del subset (posición {pos})"
            )
        }
        "overline" | "underline" | "hat" | "bar" | "vec" | "dot" | "ddot" | "tilde" | "widehat" => {
            format!("ParseLatex: decoraciones (`\\{name}`) están fuera del subset (posición {pos})")
        }
        _ => {
            format!(
                "ParseLatex: `\\{name}` no está soportado (posición {pos}). Subset: `\\frac`, `\\sqrt`, `^`, `_`, `\\cdot`, funciones (`\\sin`, `\\cos`, `\\exp`, `\\ln`, `\\log`...) y griegas (`\\pi`, `\\theta`)"
            )
        }
    }
}

/// Saca `$…$`, `$$…$$`, `\(…\)`, `\[…\]` si envuelven toda la entrada.
fn strip_math_wrappers(text: &mut String) {
    for (open, close) in [("$$", "$$"), ("\\(", "\\)"), ("\\[", "\\]"), ("$", "$")] {
        if text.len() > open.len() + close.len() && text.starts_with(open) && text.ends_with(close)
        {
            let inner = text[open.len()..text.len() - close.len()]
                .trim()
                .to_string();
            // `$x$y` no es envolvente real: solo se saca si no quedan `$` dentro
            // (para la forma `$`, que es ambigua).
            if open == "$" && inner.contains('$') {
                continue;
            }
            *text = inner;
            break;
        }
    }
}

/// `\left(`/`\right)` → paréntesis pelados; espaciados → un espacio.
fn preprocess_delimiters(text: &mut String) {
    for (from, to) in [
        ("\\left(", "("),
        ("\\right)", ")"),
        ("\\left[", "("),
        ("\\right]", ")"),
        ("\\left\\{", "{"),
        ("\\right\\}", "}"),
        ("\\left|", "|"),
        ("\\right|", "|"),
        ("\\left.", ""),
        ("\\right.", ""),
        ("\\qquad", " "),
        ("\\quad", " "),
        ("\\;", " "),
        ("\\:", " "),
        ("\\,", " "),
        ("\\!", " "),
        ("\\ ", " "),
    ] {
        if text.contains(from) {
            *text = text.replace(from, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_latex, to_latex};
    use crate::ast::parse_ast;

    /// `canon → latex → canon'` con igualdad estructural (vía `to_expr_string`).
    fn assert_roundtrip(canon: &str) -> String {
        let latex = to_latex(canon);
        let back = parse_latex(&latex)
            .unwrap_or_else(|e| panic!("roundtrip falló para «{canon}» (latex «{latex}»): {e}"));
        let want = parse_ast(canon)
            .unwrap_or_else(|e| panic!("canon inválido «{canon}»: {e}"))
            .to_expr_string();
        let got = parse_ast(&back)
            .unwrap_or_else(|e| panic!("traducción inválida «{back}»: {e}"))
            .to_expr_string();
        assert_eq!(got, want, "canon «{canon}» → latex «{latex}» → «{back}»");
        back
    }

    #[test]
    fn potencia_y_fraccion_basicas() {
        assert_eq!(to_latex("x^2"), "x^{2}");
        assert_eq!(to_latex("(x + 1)/(x - 1)"), r"\frac{x + 1}{x - 1}");
        assert_roundtrip("x^2");
        assert_roundtrip("(x + 1)/(x - 1)");
    }

    #[test]
    fn raiz_y_trig_con_griega() {
        assert_roundtrip("sqrt(x^2 + 1)");
        assert_roundtrip("sin(x) + cos(theta)^2");
        assert_roundtrip("tan(x) - sec(x)/csc(x)");
    }

    #[test]
    fn constantes_pi_tau_e() {
        assert_eq!(to_latex("pi"), r"\pi");
        assert_eq!(to_latex("tau"), r"\tau");
        assert_eq!(to_latex("e"), "e");
        assert_roundtrip("pi");
        assert_roundtrip("2*pi + e");
        // `3.14` aproximado NO se vuelve `\pi`.
        assert_eq!(to_latex("3.14"), "3.14");
    }

    #[test]
    fn multiplicacion_precedencia_y_negacion() {
        assert_roundtrip("2*x + 3*y");
        assert_roundtrip("-(x + 1)*2 + 3");
        assert_roundtrip("x - (y - z)");
    }

    #[test]
    fn subindices() {
        assert_eq!(to_latex("x_1"), "x_{1}");
        assert_roundtrip("x_1 + y_12");
        assert_eq!(parse_latex("x_{1}^{2}").expect("x_1^2"), "x_1^(2)");
    }

    #[test]
    fn exp_log_abs_floor() {
        assert_roundtrip("exp(x) + ln(x) + log(x)");
        assert_roundtrip("abs(x - 1) + floor(x) + ceil(y)");
        assert_roundtrip("cbrt(x + 1)");
    }

    #[test]
    fn comparaciones() {
        assert_roundtrip("x <= y");
        assert_eq!(parse_latex("x \\leq y + 1").expect("leq"), "x <= y + 1");
        assert_eq!(parse_latex("a \\neq b").expect("neq"), "a != b");
    }

    #[test]
    fn to_latex_acepta_ast_y_texto() {
        let ast = parse_ast("x^2 + sin(y)").expect("ast");
        assert_eq!(to_latex(&ast), to_latex("x^2 + sin(y)"));
        assert_eq!(to_latex(ast), to_latex("x^2 + sin(y)"));
        let owned = "x^2".to_string();
        assert_eq!(to_latex(owned), "x^{2}");
    }

    #[test]
    fn to_latex_devuelve_input_si_no_parsea() {
        assert_eq!(to_latex("(((no cierra"), "(((no cierra");
        assert_eq!(to_latex(""), "");
    }

    #[test]
    fn parse_directo_subset() {
        assert_eq!(parse_latex("\\frac{1}{2}").expect("frac"), "(1)/(2)");
        assert_eq!(parse_latex("\\sqrt{2}").expect("sqrt"), "sqrt(2)");
        assert_eq!(parse_latex("\\sqrt[3]{x}").expect("cbrt"), "cbrt(x)");
        assert_eq!(parse_latex("2\\pi").expect("2pi"), "2*pi");
        assert_eq!(parse_latex("\\pi^2").expect("pi2"), "pi^(2)");
        assert_eq!(parse_latex("$x^2$").expect("wrappers"), "x^(2)");
        assert_eq!(
            parse_latex("\\operatorname{gamma}(x)").expect("gamma"),
            "gamma(x)"
        );
        assert_eq!(parse_latex("\\min(a, b)").expect("min"), "min(a, b)");
        assert_eq!(parse_latex("|x - 1|").expect("abs"), "abs(x - 1)");
    }

    #[test]
    fn errores_honestos_fuera_del_subset() {
        for bad in [
            "\\int x dx",
            "\\sum_{i=1}^n i",
            "\\begin{matrix}a\\end{matrix}",
            "\\foo",
            "x_{i+1}",
            "\\frac{1}{2",
            "",
            "\\infty",
            "\\sin^2(x)",
            "sin",
        ] {
            assert!(
                parse_latex(bad).is_err(),
                "«{bad}» debería dar error honesto"
            );
        }
        let err = parse_latex("\\int x dx").unwrap_err();
        assert!(err.contains("\\int"), "el error nombra al culpable: {err}");
    }

    #[test]
    fn operatorname_redondo() {
        assert_roundtrip("gamma(x) + erf(x)");
        assert_roundtrip("atan2(y, x) + min(a, b)");
    }
}
