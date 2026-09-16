//! Demostración algebraica: `Relation` (conjetura) y `Prove` (Rabinowitsch).
//!
//! `Prove[h1, h2 ⊢ c]` calcula la base de Gröbner de `{h1, h2, 1-t·c}`
//! (truco de Rabinowitsch con variable fresca `t`): si la base contiene
//! una constante no nula, `c` está en el radical del ideal de hipótesis y
//! la proposición queda **demostrada**. Si el sistema excede las cotas
//! B2.4 o no es polinómico, el resultado es **indefinido** honesto, jamás
//! un falso positivo.
//!
//! `Relation[a, b]` es la conjetura previa: igualdad simbólica exacta,
//! y si no decide, muestreo numérico determinista que puede refutar
//! (contraejemplo) pero solo sugiere (nunca demuestra).

use crate::ast::parse_ast;
use crate::cas::{buchberger_basis_ordered, MonomialOrder};
use crate::symbolic::{expand, simplify};

/// Hipótesis máximas por demostración (cota B2.4 de Gröbner incluida).
pub const MAX_PROVE_HYPS: usize = 4;
/// Puntos de muestreo de `Relation` numérica.
pub const MAX_RELATION_SAMPLES: usize = 9;

/// Veredicto de demostración.
#[derive(Debug, Clone, PartialEq)]
pub enum ProveVerdict {
    /// Demostrado: la conclusión está en el radical del ideal.
    Proved {
        /// Tamaño de la base que certifica.
        basis_len: usize,
    },
    /// Refutado: el sistema hipótesis + ¬conclusión es consistente... no:
    /// refutado solo si la conclusión simplifica a constante no nula.
    Disproved,
    /// No se pudo decidir dentro de las cotas (honesto, no es "falso").
    Unknown { reason: String },
}

/// Veredicto de conjetura numérico-simbólica.
#[derive(Debug, Clone, PartialEq)]
pub enum RelationVerdict {
    /// Igualdad simbólica exacta.
    TrueSymbolic,
    /// Todos los puntos muestrales coinciden (sugiere, no demuestra).
    TrueNumeric { samples: usize },
    /// Contraejemplo explícito en `at`.
    False { at: f64, left: f64, right: f64 },
    /// Constante no nula tras simplificar.
    FalseConstant,
    /// No decidible (multivariable, no evaluable, ...).
    Unknown { reason: String },
}

/// Demuestra `conclusion` (`lhs = rhs` o expresión `= 0`) bajo `hyps`.
///
/// `vars` son las variables del sistema. Devuelve el veredicto con la
/// traza mínima para `ProveDetails` (vía `prove_trace`).
pub fn prove(conclusion: &str, hyps: &[String], vars: &[String]) -> Result<ProveVerdict, String> {
    prove_inner(conclusion, hyps, vars).map(|(verdict, _)| verdict)
}

/// Demuestra y devuelve la traza (base de Gröbner certificante).
pub fn prove_trace(
    conclusion: &str,
    hyps: &[String],
    vars: &[String],
) -> Result<(ProveVerdict, Vec<String>), String> {
    prove_inner(conclusion, hyps, vars)
}

fn prove_inner(
    conclusion: &str,
    hyps: &[String],
    vars: &[String],
) -> Result<(ProveVerdict, Vec<String>), String> {
    if hyps.len() > MAX_PROVE_HYPS {
        return Err(format!(
            "Prove admite hasta {MAX_PROVE_HYPS} hipótesis (recibidas {})",
            hyps.len()
        ));
    }
    // Conclusión `lhs = rhs` → `d = lhs - rhs`; si no hay `=` (o hay
    // relacional `<=/>=/==/!=`), `d = conclusion` y el motor dirá Unknown.
    let single_eq = conclusion.chars().filter(|&c| c == '=').count() == 1
        && !conclusion.contains(['<', '>', '!']);
    let difference = if single_eq {
        match conclusion.split_once('=') {
            Some((lhs, rhs)) => format!("({lhs}) - ({rhs})"),
            None => conclusion.to_string(),
        }
    } else {
        conclusion.to_string()
    };
    // Atajo barato: si `d` expande+simplifica a 0, vale sin hipótesis;
    // si da constante no nula, refutado. Sin variables que validar.
    let canonical = expand(&difference).unwrap_or_else(|_| difference.clone());
    if let Ok(simple) = simplify(&canonical) {
        if simple.replace(' ', "") == "0" {
            return Ok((ProveVerdict::Proved { basis_len: 0 }, Vec::new()));
        }
        if let Ok(value) = simple.replace(' ', "").parse::<f64>() {
            if value != 0.0 {
                return Ok((ProveVerdict::Disproved, Vec::new()));
            }
        }
    }
    if vars.is_empty() || vars.len() > crate::cas::MAX_GROEBNER_VARS {
        return Err(format!(
            "Prove admite 1..={} variables",
            crate::cas::MAX_GROEBNER_VARS
        ));
    }
    // Rabinowitsch: variable fresca `t` y polinomio `1 - t·d`.
    let mut fresh = "t_prove".to_string();
    while vars.iter().any(|v| v == &fresh) {
        fresh.push('_');
    }
    let mut system: Vec<String> = hyps.to_vec();
    system.push(format!("1 - {fresh}*({difference})"));
    let mut all_vars: Vec<String> = vars.to_vec();
    all_vars.push(fresh);
    let basis = match buchberger_basis_ordered(&system, &all_vars, MonomialOrder::Lex) {
        Ok(outcome) => outcome.basis,
        Err(_) => {
            return Ok((
                ProveVerdict::Unknown {
                    reason: "el sistema excede las cotas de Gröbner o no es polinómico".to_string(),
                },
                Vec::new(),
            ));
        }
    };
    // Constante no nula en la base ⟺ 1 ∈ ideal ⟺ demostrado.
    for poly in &basis {
        let compact = poly.replace(' ', "");
        if let Ok(value) = compact.parse::<f64>() {
            if value != 0.0 {
                return Ok((
                    ProveVerdict::Proved {
                        basis_len: basis.len(),
                    },
                    basis,
                ));
            }
        }
    }
    Ok((
        ProveVerdict::Unknown {
            reason: "sin constante en la base: ni demostrado ni refutado en estas cotas"
                .to_string(),
        },
        basis,
    ))
}

/// Conjetura `left = right` (o `expr = 0` si `right` es `None`).
pub fn relation(left: &str, right: &str) -> RelationVerdict {
    let difference = format!("({left}) - ({right})");
    // 1) Igualdad simbólica exacta (canoniza con expand primero).
    let canonical = expand(&difference).unwrap_or_else(|_| difference.clone());
    if let Ok(simple) = simplify(&canonical) {
        let compact = simple.replace(' ', "");
        if compact == "0" {
            return RelationVerdict::TrueSymbolic;
        }
        if let Ok(value) = compact.parse::<f64>() {
            if value != 0.0 {
                return RelationVerdict::FalseConstant;
            }
        }
    }
    // 2) Muestreo numérico determinista (una sola variable).
    let (Ok(ast_l), Ok(ast_r)) = (
        parse_ast(&left.replace(' ', "")),
        parse_ast(&right.replace(' ', "")),
    ) else {
        return RelationVerdict::Unknown {
            reason: "no se pudo parsear la conjetura".to_string(),
        };
    };
    let mut vars = std::collections::BTreeSet::new();
    collect_vars(&ast_l, &mut vars);
    collect_vars(&ast_r, &mut vars);
    if vars.len() != 1 {
        return RelationVerdict::Unknown {
            reason: "la conjetura numérica requiere exactamente una variable".to_string(),
        };
    }
    let var = vars.into_iter().next().unwrap_or_else(|| "x".to_string());
    const POINTS: [f64; MAX_RELATION_SAMPLES] = [-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 3.0, 10.0];
    let mut checked = 0usize;
    for at in POINTS {
        let (Ok(l), Ok(r)) = (eval_at(&ast_l, &var, at), eval_at(&ast_r, &var, at)) else {
            continue;
        };
        if !l.is_finite() || !r.is_finite() {
            continue;
        }
        checked += 1;
        if (l - r).abs() > 1e-9 * (1.0 + l.abs().max(r.abs())) {
            return RelationVerdict::False {
                at,
                left: l,
                right: r,
            };
        }
    }
    if checked == 0 {
        return RelationVerdict::Unknown {
            reason: "ningún punto muestral evaluable".to_string(),
        };
    }
    RelationVerdict::TrueNumeric { samples: checked }
}

fn collect_vars(ast: &crate::ast::Expr, out: &mut std::collections::BTreeSet<String>) {
    use crate::ast::Expr;
    match ast {
        Expr::Const(_) => {}
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Neg(u)
        | Expr::Sin(u)
        | Expr::Cos(u)
        | Expr::Tan(u)
        | Expr::Asin(u)
        | Expr::Acos(u)
        | Expr::Atan(u)
        | Expr::Exp(u)
        | Expr::Ln(u)
        | Expr::Log(u)
        | Expr::Sqrt(u)
        | Expr::Abs(u)
        | Expr::Sinh(u)
        | Expr::Cosh(u)
        | Expr::Tanh(u)
        | Expr::Floor(u)
        | Expr::Ceil(u)
        | Expr::Round(u)
        | Expr::Sec(u)
        | Expr::Csc(u)
        | Expr::Cot(u)
        | Expr::Asinh(u)
        | Expr::Acosh(u)
        | Expr::Atanh(u)
        | Expr::Sign(u)
        | Expr::Heaviside(u)
        | Expr::Cbrt(u)
        | Expr::Re(u)
        | Expr::Im(u)
        | Expr::Arg(u)
        | Expr::Conj(u)
        | Expr::Erf(u)
        | Expr::Erfc(u)
        | Expr::Gamma(u)
        | Expr::LnGamma(u)
        | Expr::Digamma(u)
        | Expr::Trigamma(u) => collect_vars(u, out),
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b)
        | Expr::Atan2(a, b)
        | Expr::Modulo(a, b)
        | Expr::Min(a, b)
        | Expr::Max(a, b)
        | Expr::Beta(a, b)
        | Expr::BesselJ(a, b)
        | Expr::BesselY(a, b)
        | Expr::BesselI(a, b)
        | Expr::Lt(a, b)
        | Expr::Gt(a, b)
        | Expr::Le(a, b)
        | Expr::Ge(a, b)
        | Expr::Eq(a, b)
        | Expr::Ne(a, b) => {
            collect_vars(a, out);
            collect_vars(b, out);
        }
        Expr::Clamp(x, lo, hi) => {
            collect_vars(x, out);
            collect_vars(lo, out);
            collect_vars(hi, out);
        }
        Expr::Sum(body, v, s, e) | Expr::Product(body, v, s, e) => {
            collect_vars(body, out);
            collect_vars(s, out);
            collect_vars(e, out);
            out.remove(v);
        }
        Expr::Piecewise(pieces, default) => {
            for (c, v) in pieces {
                collect_vars(c, out);
                collect_vars(v, out);
            }
            collect_vars(default, out);
        }
    }
}

fn eval_at(ast: &crate::ast::Expr, var: &str, at: f64) -> Result<f64, String> {
    let value = ast.eval_at(var, at);
    if value.is_finite() {
        Ok(value)
    } else {
        Err("no finito".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_pythagoras_identity() {
        // sin²x + cos²x = 1 no es polinómico → Unknown honesto (no falso).
        let verdict = prove("sin(x)^2+cos(x)^2=1", &[], &["x".to_string()])
            .expect("no debe fallar el invocador");
        assert!(
            matches!(
                verdict,
                ProveVerdict::Unknown { .. } | ProveVerdict::Proved { .. }
            ),
            "got {verdict:?}"
        );
    }

    #[test]
    fn prove_linear_consequence() {
        // De {x + y = 3, x − y = 1} se sigue x = 2.
        let hyps = vec!["x+y-3".to_string(), "x-y-1".to_string()];
        let verdict =
            prove("x=2", &hyps, &["x".to_string(), "y".to_string()]).expect("sistema lineal");
        assert!(
            matches!(verdict, ProveVerdict::Proved { .. }),
            "got {verdict:?}"
        );
    }

    #[test]
    fn prove_refutes_false_constant() {
        let verdict = prove("1=2", &[], &["x".to_string()]).expect("falso constante");
        assert_eq!(verdict, ProveVerdict::Disproved);
    }

    #[test]
    fn relation_symbolic_and_counterexample() {
        // Lo que el simplificador decide (`x-x`): simbólico.
        assert_eq!(relation("x-x", "0"), RelationVerdict::TrueSymbolic);
        // El simplificador no cancela x*x−x*x: conjetura numérica honesta.
        assert_eq!(
            relation("(x+1)^2", "x^2+2*x+1"),
            RelationVerdict::TrueNumeric { samples: 9 }
        );
        match relation("x^2", "2*x") {
            RelationVerdict::False { at, left, right } => {
                // x²≠2x en el primer punto muestral que difiere (x=-2).
                assert!((left - right).abs() > 1e-9, "at={at}");
            }
            other => panic!("esperaba contraejemplo, got {other:?}"),
        }
    }

    #[test]
    fn relation_hyp_cap_is_honest() {
        let hyps: Vec<String> = (0..9).map(|i| format!("x+{i}")).collect();
        let err = prove("x=0", &hyps, &["x".to_string()]).expect_err("cota");
        assert!(err.contains('4'), "got {err}");
    }
}
