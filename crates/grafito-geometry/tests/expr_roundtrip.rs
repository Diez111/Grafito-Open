use grafito_geometry::ast::{parse_ast, Expr};
use grafito_geometry::expr::{evaluate, evaluate_cached, CompiledExpr};
use grafito_geometry::symbolic;
use std::collections::HashMap;

fn assert_constant_round_trip(value: f64) {
    let printed = Expr::Const(value).to_expr_string();
    let reparsed = parse_ast(&printed)
        .unwrap_or_else(|error| panic!("failed to reparse {value:?} as {printed:?}: {error}"));
    let actual = reparsed.eval_at("x", 0.0);
    assert_eq!(
        actual.to_bits(),
        value.to_bits(),
        "{value:?} printed as {printed:?}"
    );
}

fn assert_semantic_round_trip(source: &str, variables: &[(&str, f64)]) {
    let original = parse_ast(source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
    let printed = original.to_expr_string();
    let reparsed = parse_ast(&printed)
        .unwrap_or_else(|error| panic!("failed to reparse {source:?} as {printed:?}: {error}"));

    let values: HashMap<String, f64> = variables
        .iter()
        .map(|(name, value)| ((*name).to_string(), *value))
        .collect();
    let expected = original.substitute_vars(&values, &[]);
    let actual = reparsed.substitute_vars(&values, &[]);
    assert_eq!(
        actual.eval_at("__unused", 0.0).to_bits(),
        expected.eval_at("__unused", 0.0).to_bits(),
        "{source:?} printed as {printed:?}"
    );
}

#[test]
fn finite_constants_print_with_exact_f64_round_trip_precision() {
    for value in [
        1e-7,
        std::f64::consts::PI,
        f64::from_bits(1),
        f64::MAX,
        1.000_000_000_05,
        -0.0,
    ] {
        assert_constant_round_trip(value);
    }
}

#[test]
fn scientific_literals_parse_and_evaluate_without_becoming_euler_products() {
    for (source, expected) in [
        ("1e-3", 0.001_f64),
        ("2.5E+1", 25.0_f64),
        (".5e2", 50.0_f64),
        ("1.e+2", 100.0_f64),
        ("1.e-2", 0.01_f64),
    ] {
        let parsed = parse_ast(source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
        assert_eq!(parsed.eval_at("x", 0.0).to_bits(), expected.to_bits());
        assert_eq!(
            evaluate(source, &[]).unwrap().to_bits(),
            expected.to_bits(),
            "evaluate({source:?})"
        );
    }

    for source in ["1e+", "1e-", "1e309", "1e-4000"] {
        assert!(parse_ast(source).is_err(), "{source:?} must be rejected");
    }

    for source in ["1e309", "1e-4000"] {
        assert!(evaluate(source, &[]).is_err(), "evaluate({source:?})");
        assert!(
            evaluate_cached(source, &[]).is_err(),
            "evaluate_cached({source:?})"
        );
        assert!(
            CompiledExpr::new(source, &HashMap::new()).is_err(),
            "CompiledExpr::new({source:?})"
        );
    }

    let error = parse_ast("π+1e+").expect_err("malformed exponent must be rejected");
    assert!(error.contains("byte offset 4"), "{error}");

    for (source, x, expected) in [
        ("2e+x", 1.0_f64, 2.0 * std::f64::consts::E + 1.0),
        ("2e-x", 1.0_f64, 2.0 * std::f64::consts::E - 1.0),
        ("2e+sin(x)", 0.0_f64, 2.0 * std::f64::consts::E),
    ] {
        let value = evaluate(source, &[("x".to_string(), x)]).unwrap_or_else(|error| {
            panic!("Euler multiplication {source:?} must remain supported: {error}")
        });
        assert_eq!(value.to_bits(), expected.to_bits(), "{source:?}");
    }
}

#[test]
fn operator_printing_preserves_grouping_and_evaluation_order() {
    for source in [
        "(-2)^2",
        "(2^3)^2",
        "2^(3^2)",
        "10000000000000000 + (-10000000000000000 + 1)",
        "2 < (0 < 1)",
    ] {
        assert_semantic_round_trip(source, &[]);
    }
}

#[test]
fn piecewise_printing_uses_the_parser_grammar() {
    for source in [
        "piecewise(7)",
        "piecewise(x < 0, x^2, 7)",
        "piecewise(x < 0, -1, x > 0, 1, 0)",
    ] {
        assert_semantic_round_trip(source, &[("x", -2.0)]);
        assert_semantic_round_trip(source, &[("x", 2.0)]);
    }
}

#[test]
fn trig_simplification_compares_structure_not_printed_collisions() {
    let source = "sin((x^y)^z)^2 + cos(x^(y^z))^2";
    let expression = parse_ast(source).unwrap();
    let simplified = expression.simplify();

    assert_ne!(simplified, Expr::Const(1.0));
    let variables = [("x", 2.0), ("y", 2.0), ("z", 3.0)];
    let values: HashMap<String, f64> = variables
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    let original_value = expression.substitute_vars(&values, &[]);
    let simplified_value = simplified.substitute_vars(&values, &[]);
    assert_eq!(
        simplified_value.eval_at("__unused", 0.0).to_bits(),
        original_value.eval_at("__unused", 0.0).to_bits()
    );
}

#[test]
fn structural_simplification_distinguishes_signed_zero() {
    let source = "sin(atan2(0,x)/3+0.3)^2 + cos(atan2(-0.0,x)/3+0.3)^2";
    let expression = parse_ast(source).unwrap();
    let simplified = expression.simplify();
    let variables = HashMap::from([("x".to_string(), -1.0)]);
    let expected = expression
        .substitute_vars(&variables, &[])
        .eval_at("__unused", 0.0);
    let actual = simplified
        .substitute_vars(&variables, &[])
        .eval_at("__unused", 0.0);

    assert_ne!(simplified, Expr::Const(1.0));
    assert_eq!(actual.to_bits(), expected.to_bits());

    let result = symbolic::simplify("(atan2(0,x) - atan2(-0.0,x)) / 3").unwrap();
    let value = parse_ast(&result).unwrap().eval_at("x", -1.0);
    assert!(
        (value - 2.0 * std::f64::consts::PI / 3.0).abs() < 1e-12,
        "{result}"
    );
}

#[test]
fn algebraic_identities_preserve_undefined_subexpressions() {
    for inner in ["x + NaN", "1 / 0"] {
        let source = format!("sin({inner})^2 + cos({inner})^2");
        let expression = parse_ast(&source).unwrap();
        let simplified = expression.simplify();

        assert_ne!(simplified, Expr::Const(1.0), "{source}");
        assert!(expression.eval_at("x", 1.0).is_nan(), "{source}");
        assert!(simplified.eval_at("x", 1.0).is_nan(), "{source}");
    }

    let result = symbolic::simplify("(x+NaN)-(x+NaN)").unwrap();
    assert!(
        parse_ast(&result).unwrap().eval_at("x", 1.0).is_nan(),
        "{result}"
    );
}
