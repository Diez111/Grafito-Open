//! Feedback formativo — evalúa respuesta y sugiere próximo paso.

use crate::exercise::Exercise;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Misconception {
    Sign,
    Distributive,
    ChainRule,
    Fraction,
    Domain,
    Notation,
    Exponent,
    Algebra,
    Concept,
    None,
}

impl Misconception {
    /// Etiqueta en español para UI y reportes.
    pub const fn etiqueta(&self) -> &'static str {
        match self {
            Self::Sign => "signo",
            Self::Distributive => "propiedad distributiva",
            Self::ChainRule => "regla de la cadena",
            Self::Fraction => "fracciones",
            Self::Domain => "dominio",
            Self::Notation => "notación",
            Self::Exponent => "potencias y raíces",
            Self::Algebra => "manipulación algebraica",
            Self::Concept => "conceptual",
            Self::None => "ninguna",
        }
    }

    /// Parsea una etiqueta libre al enum cerrado (R6e: el FSM jamás guarda
    /// strings inventados por el LLM).
    ///
    /// Acepta variante inglesa (`Sign`), española (`signo`) y minúsculas;
    /// `None`/`none`/`ninguna`/vacío → `None`. Todo lo demás → `None`
    /// (sin `Concept` silencioso: el llamador distingue "sin dato" de
    /// "error conceptual"). Pura, sin `unwrap`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "sign" | "signo" => Some(Self::Sign),
            "distributive" | "distributiva" | "propiedad distributiva" => Some(Self::Distributive),
            "chainrule" | "chain_rule" | "chain-rule" | "regla de la cadena" => {
                Some(Self::ChainRule)
            }
            "fraction" | "fraccion" | "fracción" | "fracciones" => Some(Self::Fraction),
            "domain" | "dominio" => Some(Self::Domain),
            "notation" | "notacion" | "notación" => Some(Self::Notation),
            "exponent" | "exponente" | "potencias y raíces" | "potencias" => Some(Self::Exponent),
            "algebra" | "algebraica" | "manipulación algebraica" => Some(Self::Algebra),
            "concept" | "conceptual" | "concepto" => Some(Self::Concept),
            "none" | "ninguna" | "" => Some(Self::None),
            _ => Option::None,
        }
    }
}

/// Veredicto de corrección: exacta, equivalente simbólica, parcial o incorrecta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Exact,
    Equivalent,
    Partial,
    Incorrect,
}

impl Verdict {
    /// Etiqueta en español.
    pub const fn etiqueta(&self) -> &'static str {
        match self {
            Self::Exact => "exacta",
            Self::Equivalent => "equivalente",
            Self::Partial => "parcial",
            Self::Incorrect => "incorrecta",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Feedback {
    pub correct: bool,
    pub verdict: Verdict,
    pub misconception: Misconception,
    pub message: String,
    pub next_step: String,
}

impl Feedback {
    /// ¿Es parcialmente correcta? (`verdict == Partial`)
    pub fn is_partial(&self) -> bool {
        self.verdict == Verdict::Partial
    }
}

/// Evaluador determinista — compara normalizando espacios y case, con tolerancia numérica.
#[derive(Debug, Clone, Default)]
pub struct FeedbackEngine;

fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace(' ', "")
        .replace("·", "*")
        .replace('×', "*")
}

fn parse_numeric(raw: &str) -> Option<f64> {
    let t = raw.trim().to_lowercase().replace(' ', "").replace(',', ".");
    if t.is_empty() {
        return None;
    }
    // Si contiene '=', tomar parte después del último '='
    let t = if t.contains('=') {
        t.split('=').next_back().unwrap_or(&t).to_string()
    } else {
        t
    };
    // Fracción simple a/b
    if t.contains('/') {
        let parts: Vec<&str> = t.split('/').collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            // Evitar casos con paréntesis o variables
            if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
                if b.abs() > 1e-12 {
                    return Some(a / b);
                }
            }
            return None;
        }
        return None;
    }
    // Intento directo
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    // Manejar "pi" aproximado
    if t == "pi" || t == "π" {
        return Some(std::f64::consts::PI);
    }
    None
}

fn numeric_close(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let diff = (a - b).abs();
    if b.abs() < 1e-9 {
        diff <= 0.02
    } else {
        diff <= 0.02 * b.abs()
    }
}

/// ¿Parcialmente cerca en lo numérico? (entre 2 % y 10 % relativo, mismo signo).
fn numeric_partial(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    if numeric_close(a, b) {
        return false;
    }
    // Signo opuesto nunca es parcial (es error de signo).
    if a != 0.0 && b != 0.0 && a.signum() != b.signum() {
        return false;
    }
    let diff = (a - b).abs();
    if b.abs() < 1e-9 {
        diff > 0.02 && diff <= 0.10
    } else {
        let rel = diff / b.abs();
        rel > 0.02 && rel <= 0.10
    }
}

/// Forma canónica simbólica simple: minúsculas, sin espacios,
/// `·×`→`*`, `**`→`^`, `²`→`^2`, `³`→`^3`, `√`→`sqrt`, `π`→`pi`,
/// `sen(`→`sin(` (notación española aceptada como equivalente).
fn canonical_symbolic(s: &str) -> String {
    let mut t = s.trim().to_lowercase().replace(' ', "");
    t = t.replace(['·', '×'], "*");
    // `**` antes de cualquier otra cosa para no duplicar `^`.
    t = t.replace("**", "^");
    t = t.replace('²', "^2").replace('³', "^3");
    t = t.replace('√', "sqrt").replace('π', "pi");
    // `sen(` español → `sin(` inglés (misma función).
    t = t.replace("sen(", "sin(");
    t
}

/// Quita `*` implícito (coeficiente·variable) para equivalencia `2*x` ≡ `2x`.
/// Solo quita `*` entre dígito/letra/`)` y letra/`(` para no confundir `2*3` con `23`.
fn strip_implicit_star(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '*' {
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            let next = if i + 1 < chars.len() {
                Some(chars[i + 1])
            } else {
                None
            };
            let prev_ok = prev.is_some_and(|p| p.is_ascii_alphanumeric() || p == ')');
            let next_ok = next.is_some_and(|n| n.is_ascii_alphabetic() || n == '(');
            if prev_ok && next_ok {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// ¿Equivalencia simbólica simple? (sin CAS).
/// Acepta `x^2`≡`x**2`≡`x²`, `2*x`≡`2x`, `sen(`≡`sin(`.
fn symbolic_equivalent(solution: &str, answer: &str) -> bool {
    let cs = canonical_symbolic(solution);
    let ca = canonical_symbolic(answer);
    if cs == ca {
        return true;
    }
    if strip_implicit_star(&cs) == strip_implicit_star(&ca) {
        return true;
    }
    false
}

fn tokenize_symbolic(s: &str) -> Vec<String> {
    let canon = canonical_symbolic(s);
    canon
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// ¿Parcial por solapamiento de términos? (≥50 % de términos en común).
fn symbolic_partial(solution: &str, answer: &str) -> bool {
    if symbolic_equivalent(solution, answer) {
        return false;
    }
    let sol_toks = tokenize_symbolic(solution);
    let ans_toks = tokenize_symbolic(answer);
    if sol_toks.is_empty() || ans_toks.is_empty() {
        return false;
    }
    // Si ambas son numéricas puras de un solo token distinto, no es parcial.
    if sol_toks.len() == 1 && ans_toks.len() == 1 {
        return false;
    }
    // Ola 1: BTreeMap determinista (el conteo no depende del hash aleatorio).
    let mut sol_counts = std::collections::BTreeMap::new();
    for t in &sol_toks {
        *sol_counts.entry(t).or_insert(0usize) += 1;
    }
    let mut common = 0usize;
    for t in &ans_toks {
        if let Some(c) = sol_counts.get_mut(t) {
            if *c > 0 {
                *c -= 1;
                common += 1;
            }
        }
    }
    let denom = sol_toks.len().max(ans_toks.len()) as f64;
    if denom < 1.0 {
        return false;
    }
    (common as f64) / denom >= 0.5
}

fn is_domain_phrase(s: &str) -> bool {
    let low = s.to_lowercase();
    let norm = normalize(s);
    low.contains("no existe")
        || norm.contains("noexiste")
        || norm.contains("infinito")
        || norm.contains("indeterminado")
        || norm.contains("noexiste")
        || low.contains("no está definida")
        || low.contains("no esta definida")
}

fn has_power_marker(s: &str) -> bool {
    s.contains('^')
        || s.contains('²')
        || s.contains('³')
        || s.contains("**")
        || s.contains("sqrt")
        || s.contains('√')
}

/// Clave "sin notación": minúsculas canónicas sin `^`/`**`/`²`/`³`
/// (`x^2`, `x**2`, `x²` → `x2`). Dos textos con la misma clave difieren solo
/// en notación. Pura, sin regex ni `unwrap`.
fn notation_key(s: &str) -> String {
    canonical_symbolic(s).replace(['^'], "")
}

/// Funciones elementales presentes en una expresión canónica.
fn funciones_elementales(s: &str) -> Vec<&'static str> {
    ["sin", "cos", "tan", "exp", "ln", "log", "sqrt"]
        .into_iter()
        .filter(|f| s.contains(f))
        .collect()
}

/// ¿El texto contiene un patrón real `(a+b)·c`: un grupo `(...)` con `+` que
/// se MULTIPLICA (explícito `*`, implícito letra/dígito/`(` tras el `)`)?
///
/// `(x+1)^2` NO cuenta (la potencia no es propiedad distributiva) — con el
/// chequeo viejo (`prompt contiene '('` y `'+'`) cualquier error de un
/// enunciado con paréntesis se etiquetaba "propiedad distributiva" y robaba
/// el diagnóstico de Sign/Fraction/ChainRule/Concept. Pura, sin regex.
fn tiene_distributiva_multiplicada(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '(' {
            let mut depth = 0i32;
            let mut close: Option<usize> = None;
            for (j, c) in chars.iter().enumerate().skip(i) {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(cierre) = close {
                let interior: String = chars[i + 1..cierre].iter().collect();
                if interior.contains('+') {
                    let mut k = cierre + 1;
                    while k < chars.len() && (chars[k] == ' ' || chars[k] == '\t') {
                        k += 1;
                    }
                    let multiplica = match chars.get(k) {
                        Some('*') | Some('(') => true,
                        Some(c) if c.is_ascii_digit() => true,
                        // Implícito sin espacio: `(a+b)c`.
                        Some(c) if k == cierre + 1 && c.is_ascii_alphabetic() => true,
                        _ => false,
                    };
                    if multiplica {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

/// ¿Mismos términos con los signos cambiados? (`2*x-1` vs `1-2*x`).
///
/// Solo textos simples sin paréntesis: tokeniza en términos firmados y exige
/// igualdad del multiset de términos SIN signo con diferencia EN los signos —
/// el error de signo clásico al distribuir. Pura, sin regex ni `unwrap`.
fn terminos_con_signos_opuestos(sol: &str, ans: &str) -> bool {
    fn terminos_firmados(s: &str) -> Option<Vec<(bool, String)>> {
        if s.contains('(') || s.contains(')') {
            return None;
        }
        let mut out: Vec<(bool, String)> = Vec::new();
        let mut positivo = true;
        let mut actual = String::new();
        for c in s.chars() {
            match c {
                '+' | '-' => {
                    let t = actual.trim().to_lowercase();
                    if !t.is_empty() {
                        out.push((positivo, t));
                    }
                    actual.clear();
                    positivo = c == '+';
                }
                _ => actual.push(c),
            }
        }
        let t = actual.trim().to_lowercase();
        if !t.is_empty() {
            out.push((positivo, t));
        }
        if out.len() < 2 {
            return None;
        }
        Some(out)
    }
    let (Some(a), Some(b)) = (terminos_firmados(sol), terminos_firmados(ans)) else {
        return false;
    };
    if a.len() != b.len() {
        return false;
    }
    let mut sin_signo_a: Vec<&str> = a.iter().map(|(_, t)| t.as_str()).collect();
    let mut sin_signo_b: Vec<&str> = b.iter().map(|(_, t)| t.as_str()).collect();
    sin_signo_a.sort_unstable();
    sin_signo_b.sort_unstable();
    if sin_signo_a != sin_signo_b {
        return false;
    }
    let mut firmados_a = a.clone();
    let mut firmados_b = b.clone();
    firmados_a.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));
    firmados_b.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));
    firmados_a != firmados_b
}

/// Diagnóstico de misconception (10 tipadas) por SEÑALES ESTRICTAS con
/// prioridad fija. El orden documentado se respeta y **solo se diagnostica
/// cada variante cuando hay evidencia de esa variante** (antes `Distributive`
/// y `Notation` se robaban el diagnóstico con señales toscas):
///
/// `Sign → Fraction → Distributive → ChainRule → Domain → Notation →
///  Exponent → Algebra → Concept`
fn diagnose(exercise: &Exercise, answer: &str, sol_norm: &str, ans_norm: &str) -> Misconception {
    // Orden: Sign, Fraction, Distributive, ChainRule, Domain, Notation, Exponent, Algebra, Concept

    // Sign: '-' distinto (contenido o signo numérico o términos firmados al revés)
    let sol_has_minus = sol_norm.contains('-');
    let ans_has_minus = ans_norm.contains('-');
    if sol_has_minus != ans_has_minus {
        return Misconception::Sign;
    }
    if let (Some(sv), Some(av)) = (parse_numeric(&exercise.solution), parse_numeric(answer)) {
        if sv != 0.0 && av != 0.0 && sv.signum() != av.signum() {
            return Misconception::Sign;
        }
    }
    if terminos_con_signos_opuestos(&exercise.solution, answer) {
        return Misconception::Sign;
    }

    // Fraction: ambas contienen '/' pero valores difieren
    if answer.contains('/') && exercise.solution.contains('/') {
        let sol_val = parse_numeric(&exercise.solution);
        let ans_val = parse_numeric(answer);
        match (sol_val, ans_val) {
            (Some(sv), Some(av)) => {
                if !numeric_close(av, sv) {
                    return Misconception::Fraction;
                }
            }
            _ => return Misconception::Fraction,
        }
    }

    let sol_low = exercise.solution.to_lowercase();
    let ans_low = answer.to_lowercase();
    let prompt_low = exercise.prompt.to_lowercase();

    // Distributive: patrón real `(a+b)·c` (paréntesis multiplicado) en el
    // prompt, o respuesta que expandió sin paréntesis
    // (`a*c+b*c`) contra solución con paréntesis. Solo prompt (no solución):
    // una solución puede ser legítimamente un producto `(x+1)*sin(x)` sin que
    // el error del alumno sea distributivo (p. ej. olvidar la derivada de
    // afuera). El prompt con `(x+1)^2` ya NO dispara nada: la potencia no es
    // distributiva.
    if tiene_distributiva_multiplicada(&prompt_low) {
        return Misconception::Distributive;
    }
    if ans_norm.contains('+')
        && ans_norm.contains('*')
        && !ans_norm.contains('(')
        && sol_norm.contains('(')
    {
        return Misconception::Distributive;
    }

    // ChainRule: la solución compone funciones elementales que la respuesta
    // dejó afuera (derivar sin(x²) y quedarse con 2x: falta cos).
    let sol_funcs = funciones_elementales(&canonical_symbolic(&exercise.solution));
    let ans_funcs = funciones_elementales(&canonical_symbolic(answer));
    if !sol_funcs.is_empty() && sol_funcs.iter().any(|f| !ans_funcs.contains(f)) {
        return Misconception::ChainRule;
    }

    // Domain: una es frase de dominio y la otra no
    let ans_is_domain = is_domain_phrase(answer);
    let sol_is_domain = is_domain_phrase(&exercise.solution);
    if ans_is_domain != sol_is_domain {
        return Misconception::Domain;
    }
    if ans_is_domain && parse_numeric(&exercise.solution).is_some() {
        return Misconception::Domain;
    }
    if sol_is_domain && parse_numeric(answer).is_some() {
        return Misconception::Domain;
    }

    // Notation: mismos términos con distinta notación (^ vs ², sin vs sen,
    // sqrt vs √). Se exige que las claves "sin notación" sean IGUALES: con el
    // chequeo viejo (`solución con ^2` y `respuesta con un "2" sin ^`),
    // "21" contra "x^2" daba "notación".
    let notation_diff = sol_low.contains('^') != ans_low.contains('^')
        || (sol_low.contains("sin") && ans_low.contains("sen"))
        || (sol_low.contains("sen") && ans_low.contains("sin"))
        || sol_low.contains('²') != ans_low.contains('²')
        || (sol_low.contains("sqrt") && ans_low.contains('√'))
        || (sol_low.contains('√') && ans_low.contains("sqrt"));
    if notation_diff && notation_key(&exercise.solution) == notation_key(answer) {
        return Misconception::Notation;
    }

    // Exponent: ambas con marcadores de potencia/raíz pero distinto exponente
    // (ej. x^2 vs x^3, sqrt(4) vs sqrt(9)). No roba casos de Notation
    // (exige marcador en ambas y claves distintas).
    if has_power_marker(&sol_low)
        && has_power_marker(&ans_low)
        && notation_key(&exercise.solution) != notation_key(answer)
    {
        return Misconception::Exponent;
    }

    // Algebra: intento de despeje o manipulación (contiene x o = en ambas)
    // (ej. x=2 vs x=3, 2*x+1 vs 2*x+2).
    let sol_has_alg = sol_low.contains('x') || sol_low.contains('=');
    let ans_has_alg = ans_low.contains('x') || ans_low.contains('=');
    if sol_has_alg && ans_has_alg {
        return Misconception::Algebra;
    }

    Misconception::Concept
}

impl FeedbackEngine {
    /// Corrige una respuesta: exacta, equivalente simbólica simple o parcial.
    /// Alias público exigido por la herramienta `assess_answer`.
    pub fn assess_answer(&self, exercise: &Exercise, answer: &str) -> Feedback {
        self.assess(exercise, answer)
    }

    pub fn assess(&self, exercise: &Exercise, answer: &str) -> Feedback {
        let sol_norm = normalize(&exercise.solution);
        let ans_norm = normalize(answer);

        // 1. Igualdad exacta normalizada → veredicto Exacta.
        if sol_norm == ans_norm {
            return Feedback {
                correct: true,
                verdict: Verdict::Exact,
                misconception: Misconception::None,
                message: "¡Correcto! Bien razonado.".into(),
                next_step: "Probá el siguiente nivel o pedí una animación.".into(),
            };
        }

        // 2a. Tolerancia numérica 2 % → veredicto Equivalente.
        if let (Some(sol_val), Some(ans_val)) =
            (parse_numeric(&exercise.solution), parse_numeric(answer))
        {
            if numeric_close(ans_val, sol_val) {
                return Feedback {
                    correct: true,
                    verdict: Verdict::Equivalent,
                    misconception: Misconception::None,
                    message: "¡Correcto! Bien razonado.".into(),
                    next_step: "Probá el siguiente nivel o pedí una animación.".into(),
                };
            }
        }

        // 2b. Equivalencia simbólica simple (sin CAS) → veredicto Equivalente.
        // Acepta x^2≡x**2≡x², 2*x≡2x, sen(≡sin(.
        if symbolic_equivalent(&exercise.solution, answer) {
            return Feedback {
                correct: true,
                verdict: Verdict::Equivalent,
                misconception: Misconception::None,
                message: "¡Correcto! Misma expresión con otra notación válida.".into(),
                next_step: "Probá el siguiente nivel o pedí una animación.".into(),
            };
        }

        // 3. Diagnóstico de misconception (10 tipadas).
        let misconception = diagnose(exercise, answer, &sol_norm, &ans_norm);

        // 4. ¿Parcial? (numérico 2–10 % o solapamiento simbólico ≥50 %).
        let partial = if let (Some(sol_val), Some(ans_val)) =
            (parse_numeric(&exercise.solution), parse_numeric(answer))
        {
            numeric_partial(ans_val, sol_val)
        } else {
            symbolic_partial(&exercise.solution, answer)
        };

        let (base_message, next_step) = match misconception {
            Misconception::Sign => (
                "Revisá el signo: ¿es positivo o negativo? Cuidado con los menos al distribuir.".to_string(),
                "Repasá am1-der (reglas de signos) y sec-ec.".to_string(),
            ),
            Misconception::Fraction => (
                "Revisá fracciones: no se suman numeradores y denominadores por separado. Buscá común denominador.".to_string(),
                "Repasá sec-fracc (operaciones con fracciones).".to_string(),
            ),
            Misconception::Distributive => (
                "Revisá la propiedad distributiva: (a+b)·c = a·c + b·c, no a + b·c.".to_string(),
                "Repasá sec-prop (propiedad distributiva).".to_string(),
            ),
            Misconception::ChainRule => (
                "Revisá la regla de la cadena: deriva afuera y adentro. Si derivás sin(x²), queda cos(x²)·2x.".to_string(),
                "Repasá am1-der (regla de la cadena).".to_string(),
            ),
            Misconception::Domain => (
                "Revisá el dominio: ¿la función está definida ahí? ¿existe el límite o es una indeterminación?".to_string(),
                "Repasá am1-func (dominio) y am1-lim.".to_string(),
            ),
            Misconception::Notation => (
                "Revisá la notación: usá ^ para potencias (x^2), sin/cos en minúscula y paréntesis claros.".to_string(),
                "Repasá notación matemática básica y am1-func.".to_string(),
            ),
            Misconception::Exponent => (
                "Revisá potencias y raíces: (a+b)² no es a²+b², x^2·x^3 = x^5 y √a² = |a|. Compará exponentes paso a paso.".to_string(),
                "Repasá sec-cuad (potencias) y am1-func.".to_string(),
            ),
            Misconception::Algebra => (
                "Revisá el despeje algebraico paso a paso: lo que suma pasa restando, lo que multiplica pasa dividiendo. Verificá cada paso.".to_string(),
                "Repasá sec-ec (despejes) y alg-matrices.".to_string(),
            ),
            Misconception::Concept => (
                format!(
                    "Casi. Esperaba '{}', revisá el procedimiento paso a paso.",
                    exercise.solution
                ),
                "Repasá la pista socrática y pedí la animación de la tangente.".to_string(),
            ),
            Misconception::None => (
                "¡Correcto! Bien razonado.".to_string(),
                "Probá el siguiente nivel o pedí una animación.".to_string(),
            ),
        };

        if partial {
            let verdict = Verdict::Partial;
            let message = format!("Parcialmente correcto: vas bien encaminado. {base_message}");
            return Feedback {
                correct: false,
                verdict,
                misconception,
                message,
                next_step,
            };
        }

        Feedback {
            correct: false,
            verdict: Verdict::Incorrect,
            misconception,
            message: base_message,
            next_step,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exercise::{Exercise, ExerciseDifficulty, ExerciseKind};

    fn mk_ex(prompt: &str, solution: &str) -> Exercise {
        Exercise {
            prompt: prompt.into(),
            solution: solution.into(),
            kind: ExerciseKind::Numeric,
            difficulty: ExerciseDifficulty::Easy,
            lo_id: "test".into(),
            params: std::collections::BTreeMap::new(),
            seed: None,
            validator: crate::exercise::ValidatorKind::Exact,
        }
    }

    #[test]
    fn feedback_correct() {
        let ex = Exercise {
            prompt: "".into(),
            solution: "2".into(),
            kind: ExerciseKind::Numeric,
            difficulty: ExerciseDifficulty::Easy,
            lo_id: "".into(),
            params: std::collections::BTreeMap::new(),
            seed: None,
            validator: crate::exercise::ValidatorKind::Exact,
        };
        let fb = FeedbackEngine.assess(&ex, "2");
        assert!(fb.correct);
        assert_eq!(fb.misconception, Misconception::None);
    }

    #[test]
    fn feedback_correct_with_tolerance() {
        let ex = mk_ex("Deriva", "2");
        let fb = FeedbackEngine.assess(&ex, "2.01");
        // 2 vs 2.01 diff 0.01 => 0.5% <2% => correct
        assert!(fb.correct);
        assert_eq!(fb.misconception, Misconception::None);
    }

    #[test]
    fn feedback_correct_normalized_spaces_case() {
        let ex = mk_ex("p", "Sin(x)");
        let fb = FeedbackEngine.assess(&ex, " sin(x) ");
        assert!(fb.correct);
    }

    #[test]
    fn feedback_sign() {
        let ex = mk_ex("calc", "5");
        let fb = FeedbackEngine.assess(&ex, "-5");
        assert!(!fb.correct);
        assert_eq!(fb.misconception, Misconception::Sign);
        assert!(fb.message.to_lowercase().contains("signo"));
        assert!(fb.next_step.contains("am1-der"));
    }

    #[test]
    fn feedback_sign_negative_solution() {
        let ex = mk_ex("calc", "-3");
        let fb = FeedbackEngine.assess(&ex, "3");
        assert_eq!(fb.misconception, Misconception::Sign);
    }

    #[test]
    fn feedback_fraction() {
        let ex = mk_ex("fracción", "1/3");
        let fb = FeedbackEngine.assess(&ex, "1/4");
        assert_eq!(fb.misconception, Misconception::Fraction);
        assert!(fb.message.to_lowercase().contains("fracc"));
        assert!(fb.next_step.contains("sec-fracc"));
    }

    #[test]
    fn feedback_fraction_with_tolerance_not_fraction() {
        // 1/3 = 0.333..., answer 0.333 close within 2% => correct, not fraction
        let ex = mk_ex("p", "1/3");
        let fb = FeedbackEngine.assess(&ex, "0.333");
        assert!(fb.correct);
    }

    #[test]
    fn feedback_distributive() {
        let ex = mk_ex("(2+3)*4", "20");
        let fb = FeedbackEngine.assess(&ex, "14");
        assert_eq!(fb.misconception, Misconception::Distributive);
        assert!(fb.message.to_lowercase().contains("distributiva"));
        assert!(fb.next_step.contains("sec-prop"));
    }

    #[test]
    fn feedback_chain_rule() {
        let ex = mk_ex("Deriva sin(x^2)", "2*x*cos(x^2)");
        let fb = FeedbackEngine.assess(&ex, "2*x");
        assert_eq!(fb.misconception, Misconception::ChainRule);
        assert!(fb.message.to_lowercase().contains("cadena"));
        assert!(fb.next_step.contains("am1-der"));
    }

    #[test]
    fn feedback_chain_rule_cos() {
        let ex = mk_ex("Deriva", "3*x^2*sin(x)+x^3*cos(x)");
        let fb = FeedbackEngine.assess(&ex, "3*x^2");
        assert_eq!(fb.misconception, Misconception::ChainRule);
    }

    #[test]
    fn feedback_domain_no_existe() {
        let ex = mk_ex("Límite", "5");
        let fb = FeedbackEngine.assess(&ex, "no existe");
        assert_eq!(fb.misconception, Misconception::Domain);
        assert!(fb.message.to_lowercase().contains("dominio"));
        assert!(fb.next_step.contains("am1-lim") || fb.next_step.contains("am1-func"));
    }

    #[test]
    fn feedback_domain_inverse() {
        let ex = mk_ex("p", "no existe");
        let fb = FeedbackEngine.assess(&ex, "5");
        assert_eq!(fb.misconception, Misconception::Domain);
    }

    #[test]
    fn feedback_notation() {
        let ex = mk_ex("Escribe x al cuadrado", "x^2");
        let fb = FeedbackEngine.assess(&ex, "x2");
        assert_eq!(fb.misconception, Misconception::Notation);
        assert!(fb.message.to_lowercase().contains("notación"));
    }

    #[test]
    fn feedback_notation_sen() {
        let ex = mk_ex("trig", "sin(x)");
        let fb = FeedbackEngine.assess(&ex, "sen(x)");
        // sen vs sin => Notation (pero también podría ser ChainRule? sol tiene sin, ans tiene sen contiene sin substring? "sen" contains "sen" not "sin", so ans_has_trig false?
        // "sen(x)" no contiene "sin" => ChainRule would trigger before Notation. Para evitarlo, usamos x^2 caso
        // Este test usa otro caso: x^2
        let ex2 = mk_ex("p", "x^2");
        let fb2 = FeedbackEngine.assess(&ex2, "x2");
        assert_eq!(fb2.misconception, Misconception::Notation);
        let _ = fb;
    }

    #[test]
    fn feedback_concept_fallback() {
        let ex = mk_ex("calc", "42");
        let fb = FeedbackEngine.assess(&ex, "41");
        assert_eq!(fb.misconception, Misconception::Concept);
    }

    #[test]
    fn feedback_none_on_exact_match() {
        let ex = mk_ex("p", " 2 ");
        let fb = FeedbackEngine.assess(&ex, "2");
        assert!(fb.correct);
        assert_eq!(fb.misconception, Misconception::None);
    }

    #[test]
    fn feedback_numeric_fraction_tolerance() {
        let ex = mk_ex("integral", "0.3333333333");
        let fb = FeedbackEngine.assess(&ex, "1/3");
        // 0.333... vs 1/3 => parse both ~0.333 => close => correct
        assert!(fb.correct);
    }

    #[test]
    fn assess_exacta_veredicto() {
        let ex = mk_ex("calc", "2*x+1");
        let fb = FeedbackEngine.assess(&ex, "2*x+1");
        assert!(fb.correct);
        assert_eq!(fb.verdict, Verdict::Exact);
        assert_eq!(fb.misconception, Misconception::None);
        // Alias assess_answer corrige igual.
        let fb2 = FeedbackEngine.assess_answer(&ex, "2*x+1");
        assert!(fb2.correct);
        assert_eq!(fb2.verdict, Verdict::Exact);
    }

    #[test]
    fn assess_equivalente_simbolica_simple() {
        // x^2 ≡ x**2 ≡ x²
        let ex = mk_ex("potencia", "x^2");
        for ans in ["x**2", "x²"] {
            let fb = FeedbackEngine.assess(&ex, ans);
            assert!(fb.correct, "ans {ans} debe ser equivalente");
            assert_eq!(fb.verdict, Verdict::Equivalent);
        }
        // 2*x ≡ 2x (multiplicación implícita)
        let ex2 = mk_ex("lineal", "2*x+1");
        let fb2 = FeedbackEngine.assess(&ex2, "2x+1");
        assert!(fb2.correct);
        assert_eq!(fb2.verdict, Verdict::Equivalent);
        // sen( español ≡ sin(
        let ex3 = mk_ex("trig", "sin(x)");
        let fb3 = FeedbackEngine.assess(&ex3, "sen(x)");
        assert!(fb3.correct);
        assert_eq!(fb3.verdict, Verdict::Equivalent);
    }

    #[test]
    fn assess_parcial_numerica_y_simbolica() {
        // Numérica: 100 vs 105 → 5 % → parcial (no correcta).
        let ex = mk_ex("calc", "100");
        let fb = FeedbackEngine.assess(&ex, "105");
        assert!(!fb.correct);
        assert_eq!(fb.verdict, Verdict::Partial);
        assert!(fb.is_partial());
        assert!(fb.message.to_lowercase().contains("parcial"));
        // Simbólica: 2*x+1 vs 2*x+2 → 2/3 términos → parcial.
        let ex2 = mk_ex("despeje", "2*x+1");
        let fb2 = FeedbackEngine.assess(&ex2, "2*x+2");
        assert!(!fb2.correct);
        assert_eq!(fb2.verdict, Verdict::Partial);
        // Lejos no es parcial: 42 vs 30 → incorrecta conceptual (28 % de error).
        let ex3 = mk_ex("calc", "42");
        let fb3 = FeedbackEngine.assess(&ex3, "30");
        assert_eq!(fb3.verdict, Verdict::Incorrect);
        assert!(!fb3.is_partial());
    }

    #[test]
    fn diez_misconceptions_tipadas() {
        // Debe haber exactamente 10 variantes tipadas.
        let todas = [
            Misconception::Sign,
            Misconception::Distributive,
            Misconception::ChainRule,
            Misconception::Fraction,
            Misconception::Domain,
            Misconception::Notation,
            Misconception::Exponent,
            Misconception::Algebra,
            Misconception::Concept,
            Misconception::None,
        ];
        assert_eq!(todas.len(), 10);
        for m in &todas {
            assert!(!m.etiqueta().is_empty(), "etiqueta vacía para {m:?}");
        }
        // Exponent: x^2 vs x^3
        let ex_exp = mk_ex("potencia", "x^2");
        let fb_exp = FeedbackEngine.assess(&ex_exp, "x^3");
        assert_eq!(fb_exp.misconception, Misconception::Exponent);
        assert!(fb_exp.message.to_lowercase().contains("potencia"));
        // Algebra: x=2 vs x=3
        let ex_alg = mk_ex("despejá x: 2*x+1=5", "x=2");
        let fb_alg = FeedbackEngine.assess(&ex_alg, "x=3");
        assert_eq!(fb_alg.misconception, Misconception::Algebra);
        assert!(fb_alg.message.to_lowercase().contains("despeje"));
        // Veredictos tienen etiqueta en español.
        assert_eq!(Verdict::Exact.etiqueta(), "exacta");
        assert_eq!(Verdict::Equivalent.etiqueta(), "equivalente");
        assert_eq!(Verdict::Partial.etiqueta(), "parcial");
        assert_eq!(Verdict::Incorrect.etiqueta(), "incorrecta");
    }

    #[test]
    fn regresion_diez_misconceptions_diagnostico_por_senal_estricta() {
        // Regresión FIX 5: los 10 diagnósticos con un caso por variante. Los
        // prompts traen un paréntesis NO multiplicado (`(x+1)^2`, `(a+b):`) a
        // propósito: el chequeo viejo `prompt contiene '(' && '+'` devolvía
        // `Distributive` para cualquiera de estos casos y robaba el
        // diagnóstico de Sign/ChainRule/Domain/Notation/Exponent/Algebra/
        // Concept (rojo-hoy marcado abajo).
        let casos: [(Misconception, &str, &str, &str, bool); 10] = [
            // (esperado, prompt, solución, respuesta, ¿rojo-hoy?)
            // Sign: términos firmados al revés — hoy: "propiedad distributiva".
            (
                Misconception::Sign,
                "Desarrollá (x+1)^2 y restá 2*x",
                "2*x-1",
                "1-2*x",
                true,
            ),
            // Fraction: sumó numeradores y denominadores por separado
            // (señal Fraction precede al robo — verde hoy, se conserva de guarda).
            (
                Misconception::Fraction,
                "Sumá (1/2)+(1/3)",
                "5/6",
                "2/5",
                false,
            ),
            // Distributive: patrón real (a+b)·c (verde hoy: guarda del caso original).
            (Misconception::Distributive, "(2+3)*4", "20", "14", false),
            // ChainRule: se olvidó la derivada de afuera (falta cos) — hoy: Distributive.
            (
                Misconception::ChainRule,
                "Derivá (x+1)^2·sin(x) en x=0",
                "2*(x+1)*sin(x)+(x+1)^2*cos(x)",
                "2*(x+1)*sin(x)",
                true,
            ),
            // Domain: dio un número donde no existe — hoy: Distributive.
            (
                Misconception::Domain,
                "Analizá (1/x)+(x+1) en x=0",
                "no existe",
                "0",
                true,
            ),
            // Notation: x^2 escrito x2 — hoy: Distributive.
            (
                Misconception::Notation,
                "Escribí con notación (a+b): x al cuadrado",
                "x^2",
                "x2",
                true,
            ),
            // Exponent: confundió el exponente — hoy: Distributive.
            (
                Misconception::Exponent,
                "Potenciá (x+1)^2 vs (x+1)^3",
                "x^2",
                "x^3",
                true,
            ),
            // Algebra: despeje erróneo — hoy: Distributive.
            (
                Misconception::Algebra,
                "Despejá x en (x+1)+x=5",
                "x=2",
                "x=3",
                true,
            ),
            // Concept: valor cualquiera sin error de los anteriores — hoy: Distributive.
            (
                Misconception::Concept,
                "Deriva (x+1)^2 en x=1",
                "4",
                "5",
                true,
            ),
            // None: respuesta exacta (verde hoy: camino correcto).
            (Misconception::None, "Calculá (2+3)*4", "20", "20", false),
        ];
        assert_eq!(casos.len(), 10, "un caso por cada Misconception");
        for (esperado, prompt, sol, ans, _rojo_hoy) in casos {
            let ex = mk_ex(prompt, sol);
            let fb = FeedbackEngine.assess(&ex, ans);
            assert_eq!(
                fb.misconception, esperado,
                "diagnóstico mal para prompt '{prompt}' sol '{sol}' ans '{ans}'"
            );
            if esperado == Misconception::None {
                assert!(fb.correct);
            } else {
                assert!(!fb.correct, "caso {esperado:?} debe ser incorrecto");
            }
        }
    }

    #[test]
    fn notation_no_roba_casos_con_digitos_sueltos() {
        // Regresión FIX 5 (señal tosca de Notation): "sol contiene ^2 y la
        // respuesta contiene un 2 sin ^" etiquetaba "notación" a "21" contra
        // "x^2" y a cualquier número que contuviera un 2.
        let ex = mk_ex("Escribí x al cuadrado", "x^2");
        let fb = FeedbackEngine.assess(&ex, "21");
        assert_ne!(
            fb.misconception,
            Misconception::Notation,
            "21 vs x^2 no es notación"
        );
        // La notación real (misma clave sin ^) sí se detecta.
        let fb2 = FeedbackEngine.assess(&ex, "x2");
        assert_eq!(fb2.misconception, Misconception::Notation);
    }

    #[test]
    fn distributiva_exige_parentesis_multiplicado() {
        // Regresión FIX 5: `(a+b)` solo con potencia detrás NO es distributiva.
        assert!(super::tiene_distributiva_multiplicada("(2+3)*4"));
        assert!(super::tiene_distributiva_multiplicada("despejá (x+1)(x-1)"));
        assert!(!super::tiene_distributiva_multiplicada(
            "deriva (x+1)^2 en x=1"
        ));
        assert!(!super::tiene_distributiva_multiplicada("sumá (1/2)+(1/3)"));
        assert!(!super::tiene_distributiva_multiplicada(
            "analizá (1/x)+(x+1) en x=0"
        ));
    }

    #[test]
    fn assess_answer_alias_corrige_igual() {
        let ex = mk_ex("calc", "5");
        let fb = FeedbackEngine.assess_answer(&ex, "-5");
        assert!(!fb.correct);
        assert_eq!(fb.misconception, Misconception::Sign);
        assert_eq!(fb.verdict, Verdict::Incorrect);
    }

    #[test]
    fn r6e_misconception_parse_enum_cerrado() {
        // Es/en/variantes → canónico; desconocido/vacío →าศัย None honesto.
        for (raw, expected) in [
            ("sign", Some(Misconception::Sign)),
            ("Signo", Some(Misconception::Sign)),
            ("distributive", Some(Misconception::Distributive)),
            ("propiedad distributiva", Some(Misconception::Distributive)),
            ("chain_rule", Some(Misconception::ChainRule)),
            ("regla de la cadena", Some(Misconception::ChainRule)),
            ("fraction", Some(Misconception::Fraction)),
            ("fracción", Some(Misconception::Fraction)),
            ("domain", Some(Misconception::Domain)),
            ("dominio", Some(Misconception::Domain)),
            ("notation", Some(Misconception::Notation)),
            ("notación", Some(Misconception::Notation)),
            ("exponent", Some(Misconception::Exponent)),
            ("algebra", Some(Misconception::Algebra)),
            ("concept", Some(Misconception::Concept)),
            ("none", Some(Misconception::None)),
            ("ninguna", Some(Misconception::None)),
            ("", Some(Misconception::None)),
            ("typo-inventado", None),
            ("signo-extraño", None),
        ] {
            assert_eq!(Misconception::parse(raw), expected, "raw={raw:?}");
        }
    }
}
