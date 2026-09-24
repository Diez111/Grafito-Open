#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Regresión de correctitud matemática: gamma / erf / bessel / batch.
//!
//! Cubre los bugs 1–5 de la auditoría con valores conocidos independientes
//! (CPython `math.gamma`/`math.erf`; J₀/Y₀ por cuadratura trapezoidal
//! independiente en NumPy con 2M de puntos):
//! J₀(1) = 0.7651976865579665, Y₀(1) = 0.088256964215677,
//! J₀(300) = −0.033298554876305696, J₀(1000) = 0.024786686152420134.
use grafito_complex::math::complex_expr::{eval_complex_batch, parse};
use grafito_complex::{eval_complex_batch_with_complex_vars, parse_complex};
use num_complex::Complex64;
use std::collections::{BTreeMap, HashMap};

fn eval_at(input: &str, z: Complex64) -> Complex64 {
    parse(input)
        .expect("debe parsear")
        .eval(&HashMap::from([("z".to_string(), z)]))
        .expect("debe evaluar")
}

fn assert_close(actual: Complex64, expected: Complex64, tol: f64, msg: &str) {
    let error = (actual - expected).norm();
    assert!(
        error <= tol,
        "{msg}: esperado {expected:?}, fue {actual:?} (error {error:e} > tol {tol:e})"
    );
}

// ----- BUG 1: Lanczos con t = z + 6.5 -----

#[test]
fn gamma_enteros_positivos_dan_factoriales() {
    // math.gamma de CPython: Γ(n) = (n−1)!.
    for (z, expected) in [(1.0, 1.0), (2.0, 1.0), (3.0, 2.0), (4.0, 6.0), (5.0, 24.0)] {
        let value = eval_at("gamma(z)", Complex64::new(z, 0.0));
        assert_close(
            value,
            Complex64::new(expected, 0.0),
            1e-9,
            &format!("gamma({z})"),
        );
    }
}

#[test]
fn gamma_medios_enteros_conocidos() {
    // Γ(1/2) = √π; Γ(7/2) = 15√π/8 (math.gamma de CPython).
    assert_close(
        eval_at("gamma(z)", Complex64::new(0.5, 0.0)),
        Complex64::new(std::f64::consts::PI.sqrt(), 0.0),
        1e-12,
        "gamma(0.5) == sqrt(pi)",
    );
    assert_close(
        eval_at("gamma(z)", Complex64::new(3.5, 0.0)),
        Complex64::new(3.323350970447842, 0.0),
        1e-9,
        "gamma(3.5)",
    );
    assert_close(
        eval_at("gamma(z)", Complex64::new(10.0, 0.0)),
        Complex64::new(362880.0, 0.0),
        1e-6,
        "gamma(10)",
    );
}

#[test]
fn bessel_j_cero_en_uno_usa_gamma_bien() {
    // La serie de J₀ divide por Γ(n+1); con el Lanczos roto daba 0.3024.
    assert_close(
        eval_at("bessel_j(z)", Complex64::new(1.0, 0.0)),
        Complex64::new(0.7651976865579665, 0.0),
        1e-9,
        "bessel_j(1)",
    );
}

#[test]
fn bessel_y_cero_en_uno() {
    // Y₀ mezcla el J₀ con la serie logarítmica; con J₀ roto daba 0.1224.
    assert_close(
        eval_at("bessel_y(z)", Complex64::new(1.0, 0.0)),
        Complex64::new(0.088256964215677, 0.0),
        1e-9,
        "bessel_y(1)",
    );
}

// ----- BUG 2: polos de Γ -----

#[test]
fn gamma_polos_en_enteros_no_positivos_dan_nan() {
    for z in [0.0, -1.0, -2.0, -3.0, -10.0] {
        let value = eval_at("gamma(z)", Complex64::new(z, 0.0));
        assert!(
            value.re.is_nan() && value.im.is_nan(),
            "gamma({z}) debe ser polo (NaN), fue {value:?}"
        );
    }
}

// ----- BUG 3: erf -----

#[test]
fn erf_simetria_impar_exacta() {
    for z in [
        Complex64::new(5.0, 0.0),
        Complex64::new(-5.0, 0.0),
        Complex64::new(3.5, 0.0),
        Complex64::new(1.0, 2.0),
        Complex64::new(-4.0, -3.0),
        Complex64::new(0.5, -7.0),
    ] {
        let pos = eval_at("erf(z)", z);
        let neg = eval_at("erf(z)", -z);
        let expected = -pos;
        assert_eq!(
            neg, expected,
            "erf(-z) debe ser exactamente -erf(z) en {z:?}: {neg:?} vs {expected:?}"
        );
    }
}

#[test]
fn erf_valores_reales_conocidos() {
    // math.erf de CPython.
    for (z, expected) in [
        (1.0, 0.8427007929497149),
        (2.0, 0.9953222650189527),
        (-5.0, -0.9999999999984626),
    ] {
        assert_close(
            eval_at("erf(z)", Complex64::new(z, 0.0)),
            Complex64::new(expected, 0.0),
            1e-9,
            &format!("erf({z})"),
        );
    }
}

#[test]
fn erf_costura_en_treinta_sin_salto() {
    // math.erf(3) = 0.9999779095030014; la asintótica de 1 término erraba ~1e-6.
    assert_close(
        eval_at("erf(z)", Complex64::new(3.0, 0.0)),
        Complex64::new(0.9999779095030014, 0.0),
        5e-7,
        "erf(3)",
    );
    // Continuidad serie ↔ asintótica a ambos lados de la costura.
    let below = eval_at("erf(z)", Complex64::new(2.999999, 0.0));
    let above = eval_at("erf(z)", Complex64::new(3.0, 0.0));
    assert!(
        (above - below).norm() < 1e-6,
        "salto en la costura: {below:?} vs {above:?}"
    );
}

// ----- BUG 4: bessel para |z| grande -----

#[test]
fn bessel_j_grande_coincide_con_cuadratura_independiente() {
    for (x, expected) in [
        (300.0, -0.033298554876305696),
        (1000.0, 0.024786686152420134),
    ] {
        assert_close(
            eval_at("bessel_j(z)", Complex64::new(x, 0.0)),
            Complex64::new(expected, 0.0),
            1e-9,
            &format!("bessel_j({x})"),
        );
    }
}

#[test]
fn bessel_j_grande_sigue_la_asintota_de_hankel() {
    // J₀(x) ≈ √(2/πx)·cos(x−π/4); el trapecio aliaseado erraba 4×.
    // Tolerancia absoluta contra la envolvente: cerca de los ceros de J₀ el
    // error *relativo* de la asíntota explota aunque el valor sea correcto
    // (en x=150 vale −0.00077409 por cuadratura independiente y la asíntota
    // da −0.00071981).
    for x in [150.0, 300.0, 1000.0] {
        let value = eval_at("bessel_j(z)", Complex64::new(x, 0.0));
        let envelope = (2.0 / (std::f64::consts::PI * x)).sqrt();
        let asym = envelope * (x - std::f64::consts::FRAC_PI_4).cos();
        let scaled = (value.re - asym).abs() / envelope;
        assert!(
            scaled < 0.05,
            "bessel_j({x}) = {value:?} lejos de la asintota {asym} (err/envolvente {scaled:e})"
        );
    }
}

// ----- BUG 5: batch -----

#[test]
fn batch_typo_en_variable_da_error_con_diagnostico() {
    let points = vec![Complex64::new(1.0, 0.0), Complex64::new(2.0, 0.0)];
    let result = eval_complex_batch("x+1", "z", points.into_iter(), &BTreeMap::new());
    let message = result.expect_err("un typo debe propagar Err, no grilla de None");
    assert!(
        message.contains("Unknown variable: x"),
        "diagnóstico inesperado: {message}"
    );
}

#[test]
fn batch_polo_local_queda_como_celda_vacia() {
    // 1/z en z = 0 es un polo puntual: None solo ahí, el resto intacto.
    let points = vec![
        Complex64::new(1.0, 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(2.0, 0.0),
    ];
    let result = eval_complex_batch("1/z", "z", points.into_iter(), &BTreeMap::new())
        .expect("los polos puntuales no abortan el batch");
    assert_eq!(result.len(), 3);
    assert_close(
        result[0].expect("z=1 finito"),
        Complex64::new(1.0, 0.0),
        1e-12,
        "1/1",
    );
    assert!(result[1].is_none(), "z=0 es polo: celda vacía");
    assert_close(
        result[2].expect("z=2 finito"),
        Complex64::new(0.5, 0.0),
        1e-12,
        "1/2",
    );
}

#[test]
fn batch_con_vars_complejas_coincide_con_eval_de_a_uno() {
    let vars = HashMap::from([("a".to_string(), Complex64::new(2.0, 1.0))]);
    let points = vec![
        Complex64::new(1.0, 0.0),
        Complex64::new(0.0, 1.0),
        Complex64::new(-1.0, 0.5),
    ];
    let batched =
        eval_complex_batch_with_complex_vars("a*z", "z", points.clone().into_iter(), &vars)
            .expect("batch con vars complejas");
    let expr = parse_complex("a*z").expect("parsea");
    for (z, got) in points.iter().zip(batched.iter()) {
        let mut single = vars.clone();
        single.insert("z".to_string(), *z);
        let expected = expr.eval(&single).expect("eval de a uno");
        assert_close(
            got.expect("punto finito"),
            expected,
            1e-12,
            &format!("batch vs uno en {z:?}"),
        );
    }
}

#[test]
fn batch_y_eval_de_a_uno_acuerdan_en_malla() {
    // Paridad del desempate único (bug 5): finito del bytecode o arbitraje
    // del walk, igual en ambos caminos.
    let expr = parse_complex("gamma(z)").expect("parsea");
    let points: Vec<Complex64> = [-2.5, -1.0, 0.5, 1.0, 3.0, 5.0]
        .iter()
        .map(|&x| Complex64::new(x, 0.1))
        .collect();
    let batched = eval_complex_batch(
        "gamma(z)",
        "z",
        points.clone().into_iter(),
        &BTreeMap::new(),
    )
    .expect("batch");
    for (z, got) in points.iter().zip(batched.iter()) {
        let single = expr
            .eval(&HashMap::from([("z".to_string(), *z)]))
            .ok()
            .filter(|v| v.re.is_finite() && v.im.is_finite());
        assert_eq!(
            *got, single,
            "desempate distinto en {z:?}: batch={got:?} uno={single:?}"
        );
    }
}
