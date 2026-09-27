//! Paridad con `lab/shrink.py`, `lab/symI_shrink2.py` y `lab/hypL_minimize.py`.
//!
//! El test Rust NO invoca Python: los oráculos se hardcodean acá.
//! Nota honesta: `python3 lab/shrink.py --test` no corre en este box
//! (falla con `FileNotFoundError: kissat` y requiere `/tmp/opencode` con
//! `hn_hunt`/`symI_cnf`, ausentes). Los oráculos pineados salen de los
//! asserts de `run_test` en los propios `.py` (`shrink.py:62-77`,
//! `symI_shrink2.py:141-156`): triángulo+pendant k=2 -> se saca el pendant
//! y queda `[0, 1, 2]`; triángulo solo -> intacto. Ambos resultados no
//! dependen del orden del shuffle, así que valen aunque el shuffle
//! determinista de Rust difiera del `random.Random(seed)` de Python.
//! C4 (bipartito -> base SAT, nada que minimizar) y Moser spindle
//! (4-crítico -> intacto en toda variante) completan las fixtures.

use grafito_geometry::Point2;
use grafito_lab::shrink::{
    shrink_greedy, shrink_hypl, shrink_preserving_non_kcolorable, shrink_two_phase, ShrinkConfig,
    ShrinkError,
};

/// Triángulo unitario + pendant en (-1, 0) (`shrink.py::run_test`).
fn triangle_pendant() -> Vec<Point2> {
    let h = 3.0_f64.sqrt() / 2.0;
    vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.5, h),
        Point2::new(-1.0, 0.0),
    ]
}

fn triangle() -> Vec<Point2> {
    triangle_pendant().into_iter().take(3).collect()
}

/// Cuadrado unitario C4, bipartito -> k=2 SAT.
fn square() -> Vec<Point2> {
    vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ]
}

/// Moser spindle abstracto: 2 diamantes que comparten el 0 + arista 2-5.
/// 11 aristas, no-3-coloreable y 4-crítico (todo subgrafo propio inducido
/// es 3-coloreable), así que ninguna variante puede quitar nada.
fn moser() -> (usize, Vec<(usize, usize)>) {
    let edges = vec![
        (0, 1),
        (1, 3),
        (0, 3),
        (1, 2),
        (2, 3),
        (0, 4),
        (4, 6),
        (0, 6),
        (4, 5),
        (5, 6),
        (2, 5),
    ];
    (7, edges)
}

fn cfg() -> ShrinkConfig {
    ShrinkConfig::default()
}

#[test]
fn greedy_saca_pendant_oraculo_py() {
    // `shrink.py::run_test`: pendant (3) afuera, n=3, base UNSAT.
    let r = shrink_preserving_non_kcolorable(&triangle_pendant(), 2, cfg()).unwrap();
    assert!(r.base_unsat);
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2]);
    assert_eq!(r.tests, 4);
    assert_eq!(r.removed_coarse, 0);
    assert_eq!(r.removed_fine, 1);
}

#[test]
fn greedy_triangulo_intacto_oraculo_py() {
    // `shrink.py::run_test`: triángulo solo, nada que sacar.
    let r = shrink_preserving_non_kcolorable(&triangle(), 2, cfg()).unwrap();
    assert!(r.base_unsat);
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2]);
    assert_eq!(r.tests, 3);
}

#[test]
fn greedy_c4_base_sat_nada_que_minimizar() {
    // C4 es bipartito: base SAT -> se devuelve todo, sin tests.
    let r = shrink_preserving_non_kcolorable(&square(), 2, cfg()).unwrap();
    assert!(!r.base_unsat);
    assert!(!r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2, 3]);
    assert_eq!(r.tests, 0);
}

#[test]
fn greedy_moser_intacto() {
    // Moser 4-crítico k=3: 7 trials SAT, queda todo.
    let (n, edges) = moser();
    let r = shrink_greedy(n, &edges, 3, cfg()).unwrap();
    assert!(r.base_unsat);
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(r.tests, 7);
}

#[test]
fn two_phase_paridad_pendant() {
    // `symI_shrink2.py::run_test`: pendant afuera, invariante UNSAT.
    // Lote único de 4 = vaciado total -> se salta; fina: 4 tests.
    let pts = triangle_pendant();
    let n = pts.len();
    let r = shrink_preserving_non_kcolorable(&pts, 2, cfg()).unwrap();
    assert_eq!(r.kept, vec![0, 1, 2]);
    let edges = vec![(0, 1), (0, 2), (1, 2), (0, 3)];
    let r2 = shrink_two_phase(n, &edges, 2, cfg()).unwrap();
    assert!(r2.base_unsat);
    assert!(r2.invariant);
    assert_eq!(r2.kept, vec![0, 1, 2]);
    assert_eq!(r2.tests, 4);
    assert_eq!(r2.removed_coarse, 0);
    assert_eq!(r2.removed_fine, 1);
}

#[test]
fn two_phase_triangulo_y_moser_intactos() {
    let tri_edges = vec![(0, 1), (0, 2), (1, 2)];
    let r = shrink_two_phase(3, &tri_edges, 2, cfg()).unwrap();
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2]);
    assert_eq!(r.tests, 3);
    let (n, edges) = moser();
    let m = shrink_two_phase(n, &edges, 3, cfg()).unwrap();
    assert!(m.invariant);
    assert_eq!(m.kept, vec![0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(m.tests, 7);
}

#[test]
fn hypl_paridad_pendant() {
    // Fase 1 ascendente saca el 3 (4 tests); fase 2 subdivide el triángulo
    // (4 tests, todo SAT; el bloque entero = vaciado total no consulta al
    // oráculo y no cuenta); fina 3 tests sin cambios. Total 11.
    let edges = vec![(0, 1), (0, 2), (1, 2), (0, 3)];
    let r = shrink_hypl(4, &edges, 2, cfg()).unwrap();
    assert!(r.base_unsat);
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2]);
    assert_eq!(r.tests, 11);
    assert_eq!(r.removed_coarse, 1);
    assert_eq!(r.removed_fine, 0);
}

#[test]
fn hypl_moser_intacto() {
    // 4-crítico: ninguna fase (ni lotes recursivos) puede quitar nada.
    let (n, edges) = moser();
    let r = shrink_hypl(n, &edges, 3, cfg()).unwrap();
    assert!(r.base_unsat);
    assert!(r.invariant);
    assert_eq!(r.kept, vec![0, 1, 2, 3, 4, 5, 6]);
    assert!(r.tests <= ShrinkConfig::default().max_tests);
}

#[test]
fn limites_honestos() {
    // n=0, k=0, n>24, k>8: errores tipados, sin pánicos.
    assert_eq!(
        shrink_greedy(0, &[], 2, cfg()).unwrap_err(),
        ShrinkError::Empty
    );
    assert_eq!(
        shrink_greedy(3, &[(0, 1)], 0, cfg()).unwrap_err(),
        ShrinkError::BadK
    );
    assert_eq!(
        shrink_greedy(25, &[], 2, cfg()).unwrap_err(),
        ShrinkError::TooLarge { n: 25 }
    );
    assert_eq!(
        shrink_greedy(3, &[(0, 1)], 9, cfg()).unwrap_err(),
        ShrinkError::KTooLarge { k: 9 }
    );
    assert_eq!(
        shrink_preserving_non_kcolorable(&[], 2, cfg()).unwrap_err(),
        ShrinkError::Empty
    );
}
