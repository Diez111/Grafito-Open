#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente P0.2: 41 comandos CAS puros (happy path + error honesto por comando).
//! Regla de oro: cada comando nuevo visible en paleta + responde por
//! `process_input` + test e2e. Nada muerto.

use grafito_command::{
    command_registry,
    commands::{process_input, CommandOutcome},
};
use grafito_core::Document;

fn run(command: &str) -> CommandOutcome {
    let mut document = Document::new();
    let mut input = command.to_owned();
    process_input(&mut document, &mut input)
}

fn assert_message_contains(command: &str, needle: &str) {
    match run(command) {
        CommandOutcome::Message(message) => assert!(
            message.contains(needle),
            "{command} → {message} (esperaba '{needle}')"
        ),
        CommandOutcome::Ok => panic!("{command} dio Ok, esperaba Message con '{needle}'"),
        CommandOutcome::Error(message) => {
            panic!("{command} dio Error: {message} (esperaba '{needle}')")
        }
    }
}

fn assert_error_contains(command: &str, needle: &str) {
    match run(command) {
        CommandOutcome::Error(message) => assert!(
            message.contains(needle),
            "{command} → Error {message} (esperaba '{needle}')"
        ),
        CommandOutcome::Message(message) => {
            panic!("{command} dio Message: {message} (esperaba Error con '{needle}')")
        }
        CommandOutcome::Ok => panic!("{command} dio Ok, esperaba Error con '{needle}'"),
    }
}

fn palette_must_be_visible(canonical: &str) {
    let spec =
        command_registry::resolve(canonical).unwrap_or_else(|| panic!("{canonical} registrado"));
    assert!(spec.palette_visible, "{canonical} visible en paleta");
    assert_eq!(spec.canonical, canonical);
}

#[test]
fn p02_conteos_y_paleta() {
    // Piso P0.2; las olas P0.3+ lo superan (ver oleada2 para el total vigente).
    assert!(command_registry::all().len() >= 379);
    assert!(command_registry::palette_commands().count() >= 334);
    for canonical in [
        "GCD",
        "LCM",
        "ExtendedGCD",
        "Divisors",
        "DivisorsSum",
        "NextPrime",
        "PreviousPrime",
        "ModularExponent",
        "Mod",
        "Div",
        "ToBase",
        "FromBase",
        "ContinuedFraction",
        "Substitute",
        "Polynomial",
        "Coefficients",
        "Degree",
        "Roots",
        "RootList",
        "ComplexRoot",
        "Numerator",
        "Denominator",
        "CommonDenominator",
        "Division",
        "IsFactored",
        "IsVertexForm",
        "MinimalPolynomial",
        "LetterToUnicode",
        "UnicodeToLetter",
        "TextToUnicode",
        "UnicodeToText",
        "CharacteristicPolynomial",
        "ReducedRowEchelonForm",
        "SVD",
        "LUDecomposition",
        "QRDecomposition",
        "JordanDiagonalization",
        "UnitPerpendicularVector",
        "Normalize",
        "PerpendicularVector",
        "CurvatureVector",
    ] {
        palette_must_be_visible(canonical);
    }
}

#[test]
fn p02_gcd() {
    assert_message_contains("GCD[12, 18]", "GCD[12, 18] = 6");
    assert_error_contains("GCD[2.5, 3]", "no es entero");
}

#[test]
fn p02_lcm() {
    assert_message_contains("LCM[4, 6]", "LCM[4, 6] = 12");
    assert_error_contains("LCM[2.5, 3]", "no es entero");
}

#[test]
fn p02_extended_gcd() {
    assert_message_contains("ExtendedGCD[30, 21]", "ExtendedGCD[30, 21] = (3,");
    assert_error_contains("ExtendedGCD[30]", "cantidad de argumentos inválida");
}

#[test]
fn p02_divisors() {
    assert_message_contains("Divisors[28]", "Divisors[28] = {1, 2, 4, 7, 14, 28}");
    assert_message_contains("DivisorsList[12]", "Divisors[12] = {1, 2, 3, 4, 6, 12}");
    assert_error_contains("Divisors[0]", "n >= 1");
}

#[test]
fn p02_divisors_sum() {
    assert_message_contains("DivisorsSum[12]", "DivisorsSum[12] = 28");
    assert_error_contains("DivisorsSum[0]", "n >= 1");
}

#[test]
fn p02_next_prime() {
    assert_message_contains("NextPrime[7]", "NextPrime[7] = 11");
    assert_error_contains("NextPrime[1000000000000]", "cota");
}

#[test]
fn p02_previous_prime() {
    assert_message_contains("PreviousPrime[10]", "PreviousPrime[10] = 7");
    assert_error_contains("PreviousPrime[2]", "no hay primo menor");
}

#[test]
fn p02_modular_exponent() {
    assert_message_contains(
        "ModularExponent[2, 10, 1000]",
        "ModularExponent[2, 10, 1000] = 24",
    );
    assert_error_contains("ModularExponent[2, -1, 7]", "exponente");
}

#[test]
fn p02_mod() {
    assert_message_contains("Mod[-7, 3]", "Mod[-7, 3] = 2");
    assert_error_contains("Mod[7, 0]", "división por cero");
}

#[test]
fn p02_div() {
    assert_message_contains("Div[7, 3]", "Div[7, 3] = 2");
    assert_error_contains("Div[7, 0]", "división por cero");
}

#[test]
fn p02_to_base() {
    assert_message_contains("ToBase[255, 16]", "ToBase[255, 16] = ff");
    assert_error_contains("ToBase[5, 1]", "base debe estar entre 2 y 36");
}

#[test]
fn p02_from_base() {
    assert_message_contains("FromBase[ff, 16]", "FromBase[ff, 16] = 255");
    assert_error_contains("FromBase[ff, 10]", "inválido en base 10");
}

#[test]
fn p02_continued_fraction() {
    assert_message_contains("ContinuedFraction[4]", "ContinuedFraction[4] = [4]");
    assert_error_contains("ContinuedFraction[]", "cantidad de argumentos inválida");
}

#[test]
fn p02_substitute() {
    assert_message_contains("Substitute[x^2, x, 3]", "Substitute[x^2, x, 3] = 9");
    assert_error_contains("Substitute[x^2, 1, 3]", "variable válida");
}

#[test]
fn p02_polynomial() {
    assert_message_contains(
        "Polynomial[{1, 2, 1}]",
        "Polynomial[{1, 2, 1}] = x^2 + 2*x + 1",
    );
    assert_error_contains("Polynomial[5]", "requiere una lista");
}

#[test]
fn p02_coefficients() {
    assert_message_contains(
        "Coefficients[x^2+2*x+1]",
        "Coefficients[x^2+2*x+1] = {1, 2, 1}",
    );
    assert_error_contains("Coefficients[sin(x)]", "polinomio");
}

#[test]
fn p02_degree() {
    assert_message_contains("Degree[x^2*y+y^3]", "Degree[x^2*y+y^3] = 3");
    assert_message_contains("Degree[x^2+1, y]", "Degree[x^2+1, y] = 0");
    assert_error_contains("Degree[sin(x)]", "polinomio");
}

#[test]
fn p02_roots() {
    assert_message_contains("Roots[x^2-4]", "{-2, 2}");
    assert_error_contains("Roots[sin(x)]", "no es polinomio");
}

#[test]
fn p02_root_list() {
    assert_message_contains("RootList[x^2-4]", "RootList[x^2-4] = {-2, 2}");
    assert_message_contains("RootList[x^2+1]", "sin raíces reales");
}

#[test]
fn p02_complex_root() {
    assert_message_contains("ComplexRoot[x^2+1]", "1i");
    assert_error_contains("ComplexRoot[sin(x)]", "no es polinomio");
}

#[test]
fn p02_numerator() {
    assert_message_contains("Numerator[6/8]", "Numerator[6/8] = 3");
    assert_error_contains("Numerator[x+1]", "no es una fracción");
    assert_message_contains("Numerator[(x+1)/(x-1)]", "= x + 1");
    assert_error_contains("Numerator[1/0]", "cero");
}

#[test]
fn p02_denominator() {
    assert_message_contains("Denominator[6/8]", "Denominator[6/8] = 4");
    assert_error_contains("Denominator[1/0]", "cero");
}

#[test]
fn p02_common_denominator() {
    assert_message_contains(
        "CommonDenominator[1/2, 1/3]",
        "CommonDenominator[1/2, 1/3] = 6",
    );
    assert_error_contains("CommonDenominator[1/2, x]", "no es fracción constante");
}

#[test]
fn p02_division() {
    assert_message_contains(
        "Division[x^2-1, x-1]",
        "Division[x^2-1, x-1] = cociente: x + 1, resto: 0",
    );
    assert_error_contains("Division[x, 0]", "divisor nulo");
}

#[test]
fn p02_is_factored() {
    assert_message_contains("IsFactored[(x+1)^2]", "IsFactored[(x+1)^2] = true");
    assert_message_contains("IsFactored[x^2-4]", "IsFactored[x^2-4] = false");
}

#[test]
fn p02_is_vertex_form() {
    assert_message_contains(
        "IsVertexForm[2*(x-3)^2+1]",
        "IsVertexForm[2*(x-3)^2+1] = true",
    );
    assert_message_contains("IsVertexForm[x^2+2*x+1]", "IsVertexForm[x^2+2*x+1] = false");
}

#[test]
fn p02_minimal_polynomial() {
    assert_message_contains(
        "MinimalPolynomial[sqrt(2)]",
        "MinimalPolynomial[sqrt(2)] = x^2 - 2",
    );
    assert_message_contains(
        "MinimalPolynomial[sqrt(2)+sqrt(3)]",
        "MinimalPolynomial[sqrt(2)+sqrt(3)] = x^4 - 10*x^2 + 1",
    );
    assert_error_contains("MinimalPolynomial[x+1]", "racional");
}

#[test]
fn p02_letter_to_unicode() {
    assert_message_contains("LetterToUnicode[\"a\"]", "U+0061");
    assert_error_contains("LetterToUnicode[\"ab\"]", "exactamente un carácter");
}

#[test]
fn p02_unicode_to_letter() {
    assert_message_contains("UnicodeToLetter[97]", "UnicodeToLetter[97] = a");
    assert_message_contains("UnicodeToLetter[U+0061]", "= a");
    assert_error_contains("UnicodeToLetter[55296]", "válido");
}

#[test]
fn p02_text_to_unicode() {
    assert_message_contains(
        "TextToUnicode[\"hola\"]",
        "TextToUnicode[\"hola\"] = {104, 111, 108, 97}",
    );
    assert_error_contains("TextToUnicode[\"\"]", "no vacío");
}

#[test]
fn p02_unicode_to_text() {
    assert_message_contains(
        "UnicodeToText[\"65, 66\"]",
        "UnicodeToText[\"65, 66\"] = AB",
    );
    assert_error_contains("UnicodeToText[\"99999999\"]", "válido");
}

#[test]
fn p02_characteristic_polynomial() {
    assert_message_contains(
        "CharacteristicPolynomial[[[2, 1], [1, 2]]]",
        "CharacteristicPolynomial = λ^2 - 4*λ + 3",
    );
    assert_error_contains(
        "CharacteristicPolynomial[[[1, 2, 3], [4, 5, 6]]]",
        "cuadrada",
    );
}

#[test]
fn p02_rref() {
    assert_message_contains(
        "ReducedRowEchelonForm[[[1, 2], [3, 4]]]",
        "ReducedRowEchelonForm:",
    );
    assert_error_contains("ReducedRowEchelonForm[foo]", "matriz");
}

#[test]
fn p02_svd() {
    assert_message_contains("SVD[[[1, 0], [0, 1]]]", "Sigma");
    assert_error_contains("SVD[foo]", "matriz");
}

#[test]
fn p02_lu_decomposition() {
    assert_message_contains("LUDecomposition[[[2, 1], [1, 2]]]", "L:");
    assert_error_contains("LUDecomposition[[[1, 2, 3]]]", "cuadrada");
}

#[test]
fn p02_qr_decomposition() {
    assert_message_contains("QRDecomposition[[[1, 0], [0, 1]]]", "Q:");
    assert_error_contains("QRDecomposition[foo]", "QR");
}

#[test]
fn p02_jordan() {
    assert_message_contains("JordanDiagonalization[[[2, 1], [1, 2]]]", "D = diag(3, 1)");
    assert_error_contains("JordanDiagonalization[[[0, -1], [1, 0]]]", "complejo");
}

#[test]
fn p02_unit_perpendicular() {
    assert_message_contains(
        "UnitPerpendicularVector[[1, 0]]",
        "UnitPerpendicularVector = [0, 1]",
    );
    assert_error_contains("UnitPerpendicularVector[[0, 0]]", "nulo");
}

#[test]
fn p02_normalize() {
    assert_message_contains("Normalize[[3, 4]]", "Normalize = [0.6");
    assert_error_contains("Normalize[[0, 0]]", "nulo");
}

#[test]
fn p02_perpendicular_vector() {
    assert_message_contains(
        "PerpendicularVector[[1, 0]]",
        "PerpendicularVector = [0, 1]",
    );
    assert_error_contains("PerpendicularVector[[1, 2, 3]]", "solo vectores 2D");
}

#[test]
fn p02_curvature_vector() {
    assert_message_contains("CurvatureVector[x^2, 1]", "κ =");
    assert_error_contains("CurvatureVector[x^2]", "cantidad de argumentos inválida");
}

#[test]
fn p02_alias_espanol() {
    assert_message_contains("mcd[12, 18]", "= 6");
    assert_message_contains("divisores[28]", "{1, 2, 4, 7, 14, 28}");
    assert_message_contains("proximo_primo[7]", "= 11");
    assert_message_contains("polinomio_minimo[sqrt(2)]", "= x^2 - 2");
}

#[test]
fn p02_minusculas_resuelven_al_canonico() {
    assert_message_contains("gcd[12, 18]", "GCD[12, 18] = 6");
    assert_message_contains("lcm[4, 6]", "LCM[4, 6] = 12");
    assert_message_contains("divisorslist[12]", "Divisors[12] = {1, 2, 3, 4, 6, 12}");
}

#[test]
fn p02_con_variables_del_documento() {
    let mut document = Document::new();
    document.set_variable("n".into(), 28.0);
    let mut input = "Divisors[n]".to_owned();
    match process_input(&mut document, &mut input) {
        CommandOutcome::Message(message) => assert!(
            message.contains("{1, 2, 4, 7, 14, 28}"),
            "Divisors[n] → {message}"
        ),
        other => panic!("Divisors[n] dio {other:?}, esperaba Message"),
    }
}
