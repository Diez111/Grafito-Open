//! Contrato de API pública de `grafito-geometry` (tarea 1).
//!
//! Fija que estas 3 rutas son alcanzables desde fuera del crate para que un
//! refactor futuro no las vuelva a privar:
//! - `grafito_geometry::symbolic::poly_gcd_subresultant`
//! - `grafito_geometry::solve::BiPoly`
//! - `grafito_geometry::solve::sylvester_resultant`

use grafito_geometry::solve::{sylvester_resultant, BiPoly};
use grafito_geometry::symbolic::poly_gcd_subresultant;
use std::collections::HashMap;

#[test]
fn contrato_gcd_bipoly_resultante_alcanzables() {
    // GCD(x²−1, x−1) = x−1, mónico, coeficientes ascendentes.
    let g = poly_gcd_subresultant(vec![-1.0, 0.0, 1.0], vec![-1.0, 1.0]);
    assert_eq!(g.len(), 2, "GCD inesperado: {g:?}");
    assert!((g[0] + 1.0).abs() < 1e-9, "GCD inesperado: {g:?}");
    assert!((g[1] - 1.0).abs() < 1e-9, "GCD inesperado: {g:?}");

    // `BiPoly` construible con la API estándar de `HashMap`.
    let mut f1: BiPoly = HashMap::new();
    let _ = f1.insert((1, 0), -1.0); // −x
    let _ = f1.insert((0, 1), 1.0); // +y  → f1 = y − x
    let mut f2: BiPoly = HashMap::new();
    let _ = f2.insert((0, 0), -1.0); // −1
    let _ = f2.insert((0, 1), 1.0); // +y  → f2 = y − 1

    // Res_y(f1, f2)(x) = x − 1 (convención estándar textbook/Mathematica:
    // producto de diferencias de raíces; ver doc de `sylvester_resultant`).
    let res = sylvester_resultant(&f1, &f2, 1, 1);
    match res {
        Some(coeffs) => {
            assert!(coeffs.len() >= 2, "resultante inesperada: {coeffs:?}");
            assert!((coeffs[0] + 1.0).abs() < 1e-6, "resultante: {coeffs:?}");
            assert!((coeffs[1] - 1.0).abs() < 1e-6, "resultante: {coeffs:?}");
        }
        None => panic!("sylvester_resultant devolvió None con entrada válida"),
    }

    // Res_y(y²+1, y−1) = 2 (raíces ±i evaluadas en 1: (1+i²)+... = 2).
    let mut g1: BiPoly = HashMap::new();
    let _ = g1.insert((0, 0), 1.0);
    let _ = g1.insert((0, 2), 1.0);
    let mut g2: BiPoly = HashMap::new();
    let _ = g2.insert((0, 0), -1.0);
    let _ = g2.insert((0, 1), 1.0);
    let res2 = sylvester_resultant(&g1, &g2, 2, 1);
    match res2 {
        Some(coeffs) => {
            assert!((coeffs[0] - 2.0).abs() < 1e-6, "resultante: {coeffs:?}");
        }
        None => panic!("sylvester_resultant devolvió None con entrada válida"),
    }

    // Antisimetría: Res(g,f) = (−1)^{mn}·Res(f,g); con m=n=1 cambia el signo.
    let res3 = sylvester_resultant(&f2, &f1, 1, 1);
    match res3 {
        Some(coeffs) => {
            assert!((coeffs[0] - 1.0).abs() < 1e-6, "antisimetría: {coeffs:?}");
            assert!((coeffs[1] + 1.0).abs() < 1e-6, "antisimetría: {coeffs:?}");
        }
        None => panic!("sylvester_resultant devolvió None con entrada válida"),
    }
}
