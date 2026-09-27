//! Paridad con `lab/parse874.py` y `lab/hn_hunt.py`.
//!
//! Oráculos generados corriendo el `.py` UNA vez con `python3` (2026-09-27)
//! y hardcodeados acá: este test NO invoca Python en runtime.
//! Los fixtures chicos pinean byte-identidad (`assert_eq!` exacto en `f64`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use grafito_geometry::Point2;
use grafito_lab::edge::{numeric_unit_edges, parse_edge_text, parse_edge_text_validated};
use grafito_lab::points::{parse_points_json, parse_pt_line, parse_pt_text};

// --- `.pt`: C4 entero, byte-idéntico al `parse_pt` del `.py` ---

#[test]
fn c4_pt_byte_identico() {
    let got: Vec<Point2> = ["{0, 0}", "{1, 0}", "{1, 1}", "{0, 1}"]
        .iter()
        .map(|l| parse_pt_line(l).unwrap())
        .collect();
    let want = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ];
    assert_eq!(got, want);
}

// --- `.pt`: fracciones y `Sqrt`, oráculos del `.py` hardcodeados ---

#[test]
fn sqrt_y_fracciones_byte_identicos() {
    // "{1/2, Sqrt[3]/2}" -> (0.5, 0.8660254037844386)
    let p = parse_pt_line("{1/2, Sqrt[3]/2}").unwrap();
    assert_eq!(p.x, 0.5);
    assert_eq!(p.y, 0.8660254037844386);
    // "{-1/2 + 1/Sqrt[6], (3*Sqrt[2] + Sqrt[3])/6}" -> (-0.0917..., 0.9957...)
    let p = parse_pt_line("{-1/2 + 1/Sqrt[6], (3*Sqrt[2] + Sqrt[3])/6}").unwrap();
    assert_eq!(p.x, -0.09175170953613693);
    assert_eq!(p.y, 0.9957819157813604);
    // "{Sqrt[2/3], 1/Sqrt[3]}" -> (0.8164..., 0.5773...)
    let p = parse_pt_line("{Sqrt[2/3], 1/Sqrt[3]}").unwrap();
    assert_eq!(p.x, 0.816496580927726);
    assert_eq!(p.y, 0.5773502691896258);
    // "{(4 + Sqrt[2] + Sqrt[6])/4, (Sqrt[2] - Sqrt[6])/4}"
    let p = parse_pt_line("{(4 + Sqrt[2] + Sqrt[6])/4, (Sqrt[2] - Sqrt[6])/4}").unwrap();
    assert_eq!(p.x, 1.9659258262890682);
    assert_eq!(p.y, -0.2588190451025207);
}

// --- `.pt`: muestra de `lab/witnesses/hn/t703_zach.vtx`, oráculos del `.py` ---

#[test]
fn witness_t703_muestra_paridad() {
    let lines = [
        "{-1, 0}",
        "{1, 0}",
        "{0, 0}",
        "{-1/12*((-2 + Sqrt[2])*(-3 + Sqrt[3])), (-6 + 3*Sqrt[2] + 2*Sqrt[3] + Sqrt[6])/12}",
        "{((-2 + Sqrt[2])*(-3 + Sqrt[3]))/12, (6 - 3*Sqrt[2] - 2*Sqrt[3] - Sqrt[6])/12}",
    ];
    let got = parse_pt_text(&lines.join("\n")).unwrap();
    assert_eq!(got.len(), 5);
    assert_eq!(got[0], Point2::new(-1.0, 0.0));
    assert_eq!(got[1], Point2::new(1.0, 0.0));
    assert_eq!(got[2], Point2::new(0.0, 0.0));
    assert_eq!(got[3].x, -0.06189562004384486);
    assert_eq!(got[3].y, 0.34635267042001816);
    assert_eq!(got[4].x, 0.06189562004384486);
    assert_eq!(got[4].y, -0.34635267042001816);
}

#[test]
fn pt_text_saltea_blancos_y_falla_honesto() {
    let pts = parse_pt_text("{0, 0}\n\n  \n{1, 0}\n").unwrap();
    assert_eq!(pts.len(), 2);
    assert!(parse_pt_line("0, 0").is_err()); // sin llaves
    assert!(parse_pt_line("{0 0}").is_err()); // sin coma top-level
    assert!(parse_pt_line("{0, 0} extra").is_err()); // sin cierre
    assert!(parse_pt_line("{1/0, 0}").is_err()); // inf -> no finito
}

// --- JSON de `hn_hunt.load_points_json` ---

#[test]
fn json_c4_paridad() {
    let pts = parse_points_json("[[0, 0], [1, 0], [1, 1], [0, 1]]").unwrap();
    assert_eq!(
        pts,
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ]
    );
}

#[test]
fn json_malo_falla_honesto() {
    assert!(parse_points_json("no-json").is_err());
    assert!(parse_points_json("[[0, 0, 0]]").is_err()); // terna, no par
    assert!(parse_points_json("[[0]]").is_err());
}

// --- `.edge`: `hn_hunt.load_edge`, 1-based -> 0-based normalizado ---

#[test]
fn edge_sample_paridad() {
    // `load_edge` del `.py` sobre este texto da {(0,1),(1,2),(2,3),(0,3)}.
    let text = "p edge 4 4\ne 1 2\ne 2 3\ne 3 4\ne 1 4\nc comentario\n";
    assert_eq!(
        parse_edge_text(text).unwrap(),
        vec![(0, 1), (0, 3), (1, 2), (2, 3)]
    );
}

#[test]
fn edge_dedup_y_errores_honestos() {
    let dup = parse_edge_text("e 2 1\ne 1 2\ne 1 2\n").unwrap();
    assert_eq!(dup, vec![(0, 1)]);
    assert!(parse_edge_text("e 1\n").is_err()); // aridad
    assert!(parse_edge_text("e a b\n").is_err()); // no enteros
    assert!(parse_edge_text("e 0 2\n").is_err()); // 0 no es 1-based
    assert!(parse_edge_text_validated("e 1 9\n", 4).is_err()); // fuera de rango
    assert!(parse_edge_text_validated("e 2 2\n", 4).is_err()); // lazo
    assert_eq!(
        parse_edge_text_validated("e 1 2\ne 3 4\n", 4).unwrap(),
        vec![(0, 1), (2, 3)]
    );
}

// --- Aristas numéricas: `hn_hunt.numeric_edges` vía `search` ---

#[test]
fn numeric_c4_y_triangulo_paridad() {
    // C4 del `--test` de hn_hunt: 4 aristas, diagonales ausentes.
    let c4 = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ];
    assert_eq!(
        numeric_unit_edges(&c4, 1e-9).unwrap(),
        vec![(0, 1), (0, 3), (1, 2), (2, 3)]
    );
    // Triángulo equilátero lado 1 del `--test`: K3.
    let tri = parse_pt_line("{1/2, Sqrt[3]/2}").unwrap();
    let tri_pts = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0), tri];
    assert_eq!(
        numeric_unit_edges(&tri_pts, 1e-9).unwrap(),
        vec![(0, 1), (0, 2), (1, 2)]
    );
}
