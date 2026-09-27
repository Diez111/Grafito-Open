//! Paridad Rust ↔ `lab/*.py` para gadgets geométricos.
//!
//! Oráculos obtenidos corriendo cada `.py` UNA vez con `python3` (2026-09-27)
//! y hardcodeados aquí: este test es autocontenido, sin Python en runtime.
//! Cada gadget pinea conteo exacto de puntos/aristas + hash FNV-1a de la
//! lista de aristas ordenada (`"a,b;a,b;..."`), más distancias terminales.

use grafito_geometry::search;
use grafito_lab::gadget;

/// FNV-1a 64 igual que el oráculo Python (`hashlib`-manual del script).
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    h
}

fn edge_str(edges: &[(usize, usize)]) -> String {
    let mut e = edges.to_vec();
    e.sort_unstable();
    e.iter()
        .map(|(a, b)| format!("{a},{b}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn check_edges(edges: &[(usize, usize)], n: usize, m: usize, hash: u64) {
    assert_eq!(edges.len(), m, "aristas: {edges:?}");
    for (a, b) in edges {
        assert!(*a < *b && *b < n, "arista inválida ({a}, {b}) n={n}");
    }
    assert_eq!(fnv1a(&edge_str(edges)), hash, "hash FNV de aristas");
}

#[test]
fn moser_cerrado_7v11a_no3coloreable() {
    // gadF_kit.py:105-168 (test_moser: k3 UNSAT + k4 SAT).
    let g = gadget::moser_spindle(true).unwrap();
    assert_eq!(g.n(), 7);
    check_edges(&g.aristas, 7, 11, 4_850_810_569_950_129_782);
    let d = g.dist_terminal().unwrap();
    assert!((d - 1.0).abs() < 1e-9, "d terminal={d}");
    // Paridad SAT sin solver: backtracking honesto (n=7 <= 24).
    assert!(!search::is_k_colorable_bruteforce(7, &g.aristas, 3).unwrap());
    assert!(search::is_k_colorable_bruteforce(7, &g.aristas, 4).unwrap());
    // Paridad CNF: symI_cnf.build_cnf(7, moser, 3) -> (21 vars, 61 cláusulas).
    let cnf = search::export_dimacs_kcoloring(7, &g.aristas, 3).unwrap();
    assert!(
        cnf.starts_with("p cnf 21 61\n"),
        "header: {}",
        cnf.lines().next().unwrap()
    );
}

#[test]
fn moser_abierto_7v10a_mono_k3() {
    // gadF_kit.py:105-168, closed=False (forcing_abierto_k3 SAT).
    let g = gadget::moser_spindle(false).unwrap();
    assert_eq!(g.n(), 7);
    check_edges(&g.aristas, 7, 10, 16_735_131_410_208_824_906);
    assert!(search::is_k_colorable_bruteforce(7, &g.aristas, 3).unwrap());
}

#[test]
fn ciclo_j2_13v() {
    // gadF_kit.py:171-220 (test_cycleJ: n=13).
    let g = gadget::cycle_j2().unwrap();
    assert_eq!(g.n(), 13);
    check_edges(&g.aristas, 13, 23, 13_263_629_989_005_767_799);
    assert_eq!(
        edge_str(&g.aristas),
        "0,1;0,2;0,4;0,5;1,2;1,3;2,3;3,6;3,12;4,5;4,6;5,6;6,8;6,9;6,12;\
         7,8;7,9;7,10;7,11;8,9;10,11;10,12;11,12"
    );
}

#[test]
fn doble_rombo_83() {
    // jcyJ_kit.py:433-474 (T2_mono83: 7v/10a, r=8/3).
    let g = gadget::double_rhombus_83().unwrap();
    assert_eq!(g.n(), 7);
    check_edges(&g.aristas, 7, 10, 16_735_131_410_208_824_906);
    let d = g.dist_terminal().unwrap();
    assert!((d - 8.0 / 3.0).abs() < 1e-9, "d={d}");
    assert!(search::is_k_colorable_bruteforce(7, &g.aristas, 3).unwrap());
}

#[test]
fn rombo_y_escalado_eta() {
    // jcyJ_kit.py:594-607 (rombo 4v/5a, r=sqrt3) y 511-530 (eta -> r=1).
    let r = gadget::rhombus_gadget().unwrap();
    assert_eq!(r.n(), 4);
    check_edges(&r.aristas, 4, 5, 13_845_481_894_582_856_670);
    let d = r.dist_terminal().unwrap();
    assert!((d - 3.0f64.sqrt()).abs() < 1e-9, "d={d}");
    let s = gadget::scale_eta(&r).unwrap();
    assert_eq!(s.n(), 7);
    check_edges(&s.aristas, 7, 10, 16_735_131_410_208_824_906);
    let ds = s.dist_terminal().unwrap();
    assert!((ds - 1.0).abs() < 1e-9, "d escalado={ds}");
}

#[test]
fn escalado_rho_y_ciclo_generico() {
    // jcyJ_kit.py:533-545 (rho: 8/3 -> 4/3, 13v/20a) y 548-591 + T4 (25v/41a).
    let g83 = gadget::double_rhombus_83().unwrap();
    let s = gadget::scale_rho(&g83).unwrap();
    assert_eq!(s.n(), 13);
    check_edges(&s.aristas, 13, 20, 9_506_094_006_305_262_280);
    let d = s.dist_terminal().unwrap();
    assert!((d - 4.0 / 3.0).abs() < 1e-9, "d={d}");
    let b = gadget::scale_rho(&gadget::double_rhombus_83().unwrap()).unwrap();
    let j = gadget::assemble_cycle_j(&s, &b, 4.0 / 3.0).unwrap();
    assert_eq!(j.n(), 25);
    check_edges(&j.aristas, 25, 41, 9_019_334_397_625_918_879);
}

#[test]
fn exo_g40_40v() {
    // exoL_gadget.py:74-86 (40 pts, 82 aristas, 59 pares sqrt(11/3), dPQ=8/3).
    let g = gadget::exo_g40().unwrap();
    assert_eq!(g.n(), 40);
    check_edges(&g.aristas, 40, 82, 1_106_942_156_409_829_485);
    let d = g.dist_terminal().unwrap();
    assert!((d - 8.0 / 3.0).abs() < 1e-9, "dPQ={d}");
    let me = gadget::exo_g40_mono113(&g.puntos).unwrap();
    assert_eq!(me.len(), 59);
    assert_eq!(fnv1a(&edge_str(&me)), 12_689_724_249_994_845_009);
    // Paridad DIMACS k4: 40 vars, 40 + 40*6 + 82*4 = 608 cláusulas.
    let cnf = search::export_dimacs_kcoloring(40, &g.aristas, 4).unwrap();
    assert!(
        cnf.starts_with("p cnf 160 608\n"),
        "header: {}",
        cnf.lines().next().unwrap()
    );
}

#[test]
fn sow_siembra() {
    // sowL_gen.py: H (7), Hk(2) (13), V31 (31v/60a), Minkowski (58v/139a).
    let h = gadget::h_set();
    assert_eq!(h.len(), 7);
    let eh = search::unit_graph_edges(&h, 1e-6).unwrap();
    check_edges(&eh, 7, 12, 11_068_642_533_050_348_457);

    let h2 = gadget::h_power(2).unwrap();
    assert_eq!(h2.len(), 13);
    let eh2 = search::unit_graph_edges(&h2, 1e-6).unwrap();
    assert_eq!(eh2.len(), 24);
    assert_eq!(fnv1a(&edge_str(&eh2)), 18_042_738_865_917_727_683);

    let v31 = gadget::v31().unwrap();
    assert_eq!(v31.len(), 31);
    let ev = search::unit_graph_edges(&v31, 1e-6).unwrap();
    check_edges(&ev, 31, 60, 16_350_226_479_087_709_176);

    let m = gadget::minkowski_sum(
        &v31,
        &[
            grafito_geometry::Point2::new(0.0, 0.0),
            grafito_geometry::Point2::new(1.0, 0.0),
        ],
    )
    .unwrap();
    assert_eq!(m.len(), 58);
    let em = search::unit_graph_edges(&m, 1e-6).unwrap();
    assert_eq!(em.len(), 139);

    assert!((gadget::eta_angle() - 0.292_842_771_728_6).abs() < 1e-12);
    assert!((gadget::rho_angle() - 0.505_360_510_284_2).abs() < 1e-12);
    assert!((gadget::psi_eta2() - 0.585_685_543_457_2).abs() < 1e-12);
    assert!((gadget::phi_83() - 1.757_056_630_371_5).abs() < 1e-12);
}

#[test]
fn errores_honestos() {
    assert!(gadget::unit_rhombus(180.0).is_err());
    assert!(gadget::unit_rhombus(f64::NAN).is_err());
    assert!(gadget::unit_rhombus(60.0).is_ok());
    // Presupuesto: 20 copias desplazadas (620 pts) exceden MAX_GADGET_POINTS.
    let base = gadget::v31().unwrap();
    let big: Vec<Vec<grafito_geometry::Point2>> = (0..20)
        .map(|k| {
            base.iter()
                .map(|p| grafito_geometry::Point2::new(p.x + 2.0 * f64::from(k), p.y))
                .collect()
        })
        .collect();
    assert!(gadget::union_all(&big).is_err());
    // r_term inconsistente con el gadget.
    let a = gadget::moser_spindle(false).unwrap();
    let b = gadget::moser_spindle(false).unwrap();
    assert!(gadget::assemble_cycle_j(&a, &b, 2.0).is_err());
    // Escalar sin terminales falla honesto.
    let mut s = gadget::moser_spindle(false).unwrap();
    s.terminales = None;
    assert!(gadget::scale_eta(&s).is_err());
}
