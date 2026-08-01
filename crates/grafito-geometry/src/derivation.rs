//! Helpers deterministas para derivar polinomios de una variable de grado dos.
//!
//! El asistente usa este módulo para evitar heurísticas de modelos al resolver
//! ecuaciones lineales y cuadráticas. La derivación inspecciona el AST en vez
//! de inferir coeficientes desde muestras numéricas que otro polinomio podría
//! imitar.

use crate::ast::{parse_ast, Expr};

const MAX_NORMALIZED_EXPRESSION_BYTES: usize = 4_096;

/// Coeficientes de `a*x^2 + b*x + c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolynomialCoefficients {
    /// Coeficiente cuadrático.
    pub a: f64,
    /// Coeficiente lineal.
    pub b: f64,
    /// Término independiente.
    pub c: f64,
}

impl PolynomialCoefficients {
    /// Construye coeficientes sin transformar los valores recibidos.
    pub const fn new(a: f64, b: f64, c: f64) -> Self {
        Self { a, b, c }
    }

    /// Evalúa el polinomio en un valor de la variable independiente.
    pub fn evaluate(self, x: f64) -> f64 {
        self.a * x * x + self.b * x + self.c
    }
}

/// Obtiene coeficientes de una expresión que sea un polinomio de grado a lo sumo dos.
///
/// Rechaza operaciones de coeficientes que desbordan, subfluyen a cero o absorben
/// una contribución no nula para no cambiar silenciosamente el grado estructural
/// del polinomio.
pub fn derive_polynomial(
    expression: &str,
    variable: &str,
) -> Result<PolynomialCoefficients, String> {
    if expression.trim().is_empty() || expression.len() > MAX_NORMALIZED_EXPRESSION_BYTES {
        return Err("polynomial expression is empty or exceeds the local budget".into());
    }
    if !is_identifier(variable) {
        return Err("polynomial variable must be a simple identifier".into());
    }

    let expression = normalize_scientific_notation(expression)?;
    if expression.len() > MAX_NORMALIZED_EXPRESSION_BYTES {
        return Err("normalized polynomial expression exceeds the local budget".into());
    }
    let expression = parse_ast(&expression)
        .map_err(|_| "polynomial expression could not be parsed structurally".to_string())?;
    polynomial_from_expr(&expression, variable)
}

/// Expande literales científicos representables a decimales que acepta el parser AST.
///
/// El parser compartido trata `e` como un identificador, por lo que el asistente
/// normaliza sólo literales independientes como `1e-308`. La conversión usa una
/// precisión decimal suficiente para reconstruir cualquier `f64` finito. Los
/// literales no nulos que subfluyen a cero y los que desbordan se rechazan antes
/// de alterar el texto de entrada.
pub fn normalize_scientific_notation(expression: &str) -> Result<String, String> {
    let bytes = expression.as_bytes();
    let mut normalized = String::with_capacity(expression.len());
    let mut index = 0;

    while index < bytes.len() {
        if let Some(end) = scientific_literal_end(bytes, index) {
            let literal = &expression[index..end];
            let value = literal
                .parse::<f64>()
                .map_err(|_| "scientific polynomial literal is invalid".to_string())?;
            if !value.is_finite() || (value == 0.0 && scientific_significand_is_nonzero(literal)) {
                return Err(
                    "scientific literal is outside local f64 input precision and cannot be normalized safely"
                        .into(),
                );
            }
            normalized.push_str(&decimal_literal(value));
            index = end;
        } else {
            let character = expression[index..]
                .chars()
                .next()
                .expect("index remains within a valid UTF-8 expression");
            normalized.push(character);
            index += character.len_utf8();
        }
    }

    Ok(normalized)
}

fn scientific_significand_is_nonzero(literal: &str) -> bool {
    literal
        .bytes()
        .take_while(|byte| !matches!(byte, b'e' | b'E'))
        .any(|byte| byte.is_ascii_digit() && byte != b'0')
}

fn scientific_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let first = *bytes.get(start)?;
    if !(first.is_ascii_digit()
        || (first == b'.' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit)))
        || (start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b'.'))
    {
        return None;
    }

    let mut index = start;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    }
    if !matches!(bytes.get(index), Some(b'e' | b'E')) {
        return None;
    }

    index += 1;
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }
    let exponent_start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    (index > exponent_start).then_some(index)
}

fn decimal_literal(value: f64) -> String {
    let mut literal = format!("{value:.324}");
    while literal.ends_with('0') {
        literal.pop();
    }
    if literal.ends_with('.') {
        literal.pop();
    }
    if literal.is_empty() || literal == "-0" {
        "0".into()
    } else {
        literal
    }
}

fn polynomial_from_expr(
    expression: &Expr,
    variable: &str,
) -> Result<PolynomialCoefficients, String> {
    use Expr::*;

    let coefficients = match expression {
        Const(value) if value.is_finite() => PolynomialCoefficients::new(0.0, 0.0, *value),
        Const(_) => return Err("polynomial constants must be finite".into()),
        Var(name) if name == variable => PolynomialCoefficients::new(0.0, 1.0, 0.0),
        Var(_) => return Err("polynomial expression contains an unsupported variable".into()),
        Neg(value) => negate(polynomial_from_expr(value, variable)?)?,
        Add(left, right) => add(
            polynomial_from_expr(left, variable)?,
            polynomial_from_expr(right, variable)?,
        )?,
        Sub(left, right) => subtract(
            polynomial_from_expr(left, variable)?,
            polynomial_from_expr(right, variable)?,
        )?,
        Mul(left, right) => multiply(
            polynomial_from_expr(left, variable)?,
            polynomial_from_expr(right, variable)?,
        )?,
        Div(numerator, denominator) => {
            let numerator = polynomial_from_expr(numerator, variable)?;
            let denominator = polynomial_from_expr(denominator, variable)?;
            if denominator.a != 0.0 || denominator.b != 0.0 || denominator.c == 0.0 {
                return Err("polynomial division must use a non-zero constant denominator".into());
            }
            scale(numerator, 1.0 / denominator.c)?
        }
        Pow(base, exponent) => {
            let base = polynomial_from_expr(base, variable)?;
            let exponent = polynomial_from_expr(exponent, variable)?;
            if exponent.a != 0.0 || exponent.b != 0.0 {
                return Err("polynomial powers must use a constant exponent".into());
            }
            match exponent.c {
                0.0 => PolynomialCoefficients::new(0.0, 0.0, 1.0),
                1.0 => base,
                2.0 => multiply(base, base)?,
                _ => return Err("polynomial expression exceeds the supported degree of two".into()),
            }
        }
        _ => {
            return Err("expression is not a one-variable polynomial of degree at most two".into())
        }
    };
    ensure_valid(coefficients)
}

fn negate(value: PolynomialCoefficients) -> Result<PolynomialCoefficients, String> {
    ensure_valid(PolynomialCoefficients::new(-value.a, -value.b, -value.c))
}

fn add(
    left: PolynomialCoefficients,
    right: PolynomialCoefficients,
) -> Result<PolynomialCoefficients, String> {
    ensure_valid(PolynomialCoefficients::new(
        coefficient_sum(left.a, right.a)?,
        coefficient_sum(left.b, right.b)?,
        coefficient_sum(left.c, right.c)?,
    ))
}

fn subtract(
    left: PolynomialCoefficients,
    right: PolynomialCoefficients,
) -> Result<PolynomialCoefficients, String> {
    ensure_valid(PolynomialCoefficients::new(
        coefficient_difference(left.a, right.a)?,
        coefficient_difference(left.b, right.b)?,
        coefficient_difference(left.c, right.c)?,
    ))
}

fn scale(value: PolynomialCoefficients, scalar: f64) -> Result<PolynomialCoefficients, String> {
    ensure_valid(PolynomialCoefficients::new(
        coefficient_product(value.a, scalar)?,
        coefficient_product(value.b, scalar)?,
        coefficient_product(value.c, scalar)?,
    ))
}

fn multiply(
    left: PolynomialCoefficients,
    right: PolynomialCoefficients,
) -> Result<PolynomialCoefficients, String> {
    let degree_four = coefficient_product(left.a, right.a)?;
    let degree_three = coefficient_sum(
        coefficient_product(left.a, right.b)?,
        coefficient_product(left.b, right.a)?,
    )?;
    if degree_four != 0.0 || degree_three != 0.0 {
        return Err("polynomial expression exceeds the supported degree of two".into());
    }
    ensure_valid(PolynomialCoefficients::new(
        coefficient_sum(
            coefficient_sum(
                coefficient_product(left.a, right.c)?,
                coefficient_product(left.b, right.b)?,
            )?,
            coefficient_product(left.c, right.a)?,
        )?,
        coefficient_sum(
            coefficient_product(left.b, right.c)?,
            coefficient_product(left.c, right.b)?,
        )?,
        coefficient_product(left.c, right.c)?,
    ))
}

fn coefficient_sum(left: f64, right: f64) -> Result<f64, String> {
    let sum = left + right;
    if !sum.is_finite() {
        return Err("polynomial coefficient sum overflowed".into());
    }
    if sum == 0.0 && left != -right {
        return Err(
            "polynomial coefficient sum underflowed and cannot be represented safely".into(),
        );
    }
    if left != 0.0 && right != 0.0 && (sum == left || sum == right) {
        return Err(
            "polynomial coefficient sum absorbed a non-zero term and cannot be represented safely"
                .into(),
        );
    }
    Ok(sum)
}

fn coefficient_difference(left: f64, right: f64) -> Result<f64, String> {
    coefficient_sum(left, -right)
}

fn coefficient_product(left: f64, right: f64) -> Result<f64, String> {
    let product = left * right;
    if !product.is_finite() {
        return Err("polynomial coefficient product overflowed".into());
    }
    if product == 0.0 && left != 0.0 && right != 0.0 {
        return Err(
            "polynomial coefficient product underflowed and cannot be represented safely".into(),
        );
    }
    Ok(product)
}

fn ensure_valid(coefficients: PolynomialCoefficients) -> Result<PolynomialCoefficients, String> {
    if coefficients.a.is_finite() && coefficients.b.is_finite() && coefficients.c.is_finite() {
        Ok(coefficients)
    } else {
        Err("polynomial coefficients must be finite".into())
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_coefficients_from_a_one_variable_quadratic() {
        let coefficients = derive_polynomial("x^2 - 5*x + 6", "x").unwrap();
        assert_eq!(coefficients, PolynomialCoefficients::new(1.0, -5.0, 6.0));
    }

    #[test]
    fn rejects_non_polynomial_expressions() {
        assert!(derive_polynomial("sin(x)", "x").is_err());
        assert!(derive_polynomial("sin(pi*x)", "x").is_err());
        assert!(derive_polynomial("x^3", "x").is_err());
    }

    #[test]
    fn rejects_a_degree_five_polynomial_that_matches_all_legacy_samples() {
        let disguised = "x^2 + x * (x + 1) * (x - 0.5) * (x - 1) * (x - 2)";

        assert!(derive_polynomial(disguised, "x").is_err());
    }

    #[test]
    fn preserves_small_nonzero_constants_in_structurally_valid_polynomials() {
        let coefficients = derive_polynomial("x^2 + 0.00000000001", "x").unwrap();

        assert_eq!(coefficients.c, 0.00000000001);
    }

    #[test]
    fn preserves_subnormal_quadratic_coefficients() {
        let coefficients = derive_polynomial("1e-308*x^2 - 1e-308", "x").unwrap();

        assert_eq!(
            coefficients,
            PolynomialCoefficients::new(1e-308, 0.0, -1e-308)
        );
    }

    #[test]
    fn rejects_nonzero_scientific_literals_outside_f64_input_precision() {
        for literal in ["1e-324", "-1e-324", "1e309", "-1e309"] {
            let error = normalize_scientific_notation(literal).unwrap_err();

            assert!(error.contains("precision"), "{literal}: {error}");
        }
    }

    #[test]
    fn preserves_representable_scientific_literals() {
        let coefficients = derive_polynomial("x - 2.5e1", "x").unwrap();

        assert_eq!(coefficients, PolynomialCoefficients::new(0.0, 1.0, -25.0));
    }

    #[test]
    fn rejects_degree_loss_from_underflowed_structural_coefficient_products() {
        let error = derive_polynomial("x - ((1e-200*x)*(1e-200*x))*x", "x").unwrap_err();

        assert!(error.contains("underflow"));
    }

    #[test]
    fn rejects_degree_loss_from_rounded_structural_coefficient_sums() {
        let error = derive_polynomial("x^2 + (1e-200*x^2 - x^2)", "x").unwrap_err();

        assert!(error.contains("sum"));
    }
}
