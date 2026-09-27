#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Regresión del ciclo de integración por partes en Fourier (2026-09-26).
//!
//! `try_generic_parts` (`symbolic.rs`) cicló con `∫x²·cos` (el árbol se
//! reproduce dos niveles después con otras constantes y crece exponencial:
//! 18 GB antes del tope de profundidad). El fix acota con `MAX_PARTS_DEPTH`
//! 24 + aborto si el integrando supera 4096 bytes de `Debug`, con fallback a
//! la ruta numérica Gauss–Legendre. `cas_steps.rs` tenía la variante con
//! pasos desalineada (`MAX_PARTS_DEPTH` 256 sin tope de tamaño); este archivo
//! pinnea que ambas rutas terminan en las tres familias:
//! polinomio×trig (`x^2`), trig×exp cíclica (`exp(x)`) y polinomio de grado
//! alto (`x^4`/`x^6·cos).
//!
//! Presupuesto de ejecución documentado (auditoría geometría 2/20):
//! `timeout 60 systemd-run --user --scope -p MemoryMax=6G cargo test -p
//! grafito-geometry --test fourier_parts_regression`. Cada test usa `terms`
//! pequeño (2–3) y presupuestos públicos (`MAX_FOURIER_TERMS` 128); si el
//! ciclo regresa, el test cuelga/OOM en vez de fallar silencioso.

use grafito_geometry::cas_steps::steps_for_integral;
use grafito_geometry::fourier::fourier_coefficients;
use grafito_geometry::symbolic::integrate;

const TAU: f64 = 2.0 * std::f64::consts::PI;

fn assert_finite_coeffs(an: &[f64], bn: &[f64], label: &str) {
    assert!(
        an.iter().chain(bn.iter()).all(|v| v.is_finite()),
        "{label}: coeficientes no finitos: an={an:?} bn={bn:?}"
    );
}

#[test]
fn fourier_x_squared_terminates() {
    // Caso original del cuelgue: FourierSeries[x^2,x,1,2].
    let c = fourier_coefficients("x^2", "x", TAU, 2).expect("fourier de x^2 termina");
    assert_eq!(c.an.len(), 2);
    assert_eq!(c.bn.len(), 2);
    assert_finite_coeffs(&c.an, &c.bn, "x^2");
}

#[test]
fn fourier_trig_times_exp_terminates() {
    // ∫exp(x)·cos(nx) se reproduce por partes (ciclo algebraico, no
    // reducción de grado): debe abortar a numérico, nunca colgar.
    let c = fourier_coefficients("exp(x)", "x", TAU, 2).expect("fourier de exp(x) termina");
    assert_finite_coeffs(&c.an, &c.bn, "exp(x)");
    // Ruta simbólica directa: Ok (primitiva) o Err honesto, pero retorna.
    let _ = integrate("exp(x)*cos(x)", "x");
    let _ = integrate("exp(x)*sin(x)", "x");
}

#[test]
fn fourier_high_degree_poly_terminates() {
    // Grado alto: necesita N niveles de partes (x^4 → 4, x^6 → 6 < 24).
    let c = fourier_coefficients("x^4", "x", TAU, 2).expect("fourier de x^4 termina");
    assert_finite_coeffs(&c.an, &c.bn, "x^4");
    // Primitiva directa de grado alto × trig debe existir dentro de la cota.
    let prim = integrate("x^6*cos(x)", "x").expect("x^6·cos(x) integra por partes");
    assert!(!prim.is_empty(), "primitiva vacía");
}

#[test]
fn stepped_parts_matches_symbolic_bounds() {
    // La variante con pasos (`cas_steps.rs`) comparte la cota 24 + 4096:
    // los mismos tres integrandos retornan (Ok con pasos o Err honesto).
    for expr in ["x^2*cos(x)", "exp(x)*cos(x)", "x^6*cos(x)"] {
        let result = steps_for_integral(expr, "x");
        match result {
            Ok(steps) => assert!(!steps.is_empty(), "{expr}: pasos vacíos"),
            Err(reason) => assert!(!reason.is_empty(), "{expr}: error vacío"),
        }
    }
}
