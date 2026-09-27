//! Gadgets geométricos Hadwiger-Nelson: builders puros punto → puntos/aristas.
//!
//! Port de `lab/gadF_kit.py` (341L), `lab/jcyJ_kit.py` (740L),
//! `lab/exoL_gadget.py` (158L) y `lab/sowL_gen.py` (127L).
//! Cada builder cita su origen exacto (archivo + líneas).
//!
//! Qué se porta y qué no:
//!
//! | Kit `.py` | Estado | Motivo / puntero |
//! |---|---|---|
//! | `gadF_kit.py` geometría (`unit_rhombus`, `rot_about`, `merge_points`, `build_moser`, `build_cycleJ`) | portado | builders de este módulo |
//! | `jcyJ_kit.py` escalados (`build_double_rhombus_83`, `scale_*_eta/rho`, `build_cycleJ_generic`, `rhombus_gadget`) | portado | builders de este módulo |
//! | `exoL_gadget.py` G40 (`dec`, `G40_RAW`, `build`) | portado | `exo_g40`, `exo_g40_mono113` |
//! | `sowL_gen.py` siembra (`H_set`, `rot_pts`, `union_all`, `minkowski`, `Hk`) | portado | `h_set`, `rot_points`, `union_all`, `minkowski_sum`, `h_power`, `v31` |
//! | CNF/DIMACS (`to_cnf`, `write_cnf`, `symI_cnf.build_cnf/to_dimacs`) | NO portado | ya existe `grafito-geometry::search::export_dimacs_kcoloring`; tool MCP `export_dimacs` |
//! | Runners kissat + checks SAT/UNSAT (`kissat_run`, `check`, `test_moser`, `test_cycleJ`, `test_j1_program`, `jcheck`) | NO portado | tool MCP `sat_check` (`grafito-mcp::sat`, kissat/cadical) + `is_k_colorable_bruteforce` (n ≤ 24) |
//! | Sweeps y loops (`__main__` de `sowL_gen`, `run_topp39_scan`, restarts) | NO portado | tools MCP `search_topp39`, `verify_search_run`, `topp39_best_of` |
//! | `colG_pack.py` (coloración neuronal torch/Adam) | NO portado | heurística ML no determinista, sin gadget geométrico; búsqueda equivalente vía `search_topp39` + `verify_coloring` + `sat_check` |
//! | `symI_cnf.py` symmetry-breaking (`find_triangle`, orden por grado, cláusulas unitarias) | NO portado | optimización del lado SAT, no gadget geométrico; el CNF base lo cubre `export_dimacs_kcoloring` (paridad de conteos en `tests/parity_gadget.rs`) |
//!
//! Convenciones: tolerancias del kit (`TOL_DEDUP = 1e-9`, `TOL_UNIT = 1e-6`),
//! aristas = todos los pares a distancia 1 vía `search::unit_graph_edges`
//! (no se duplica), errores honestos sin `unwrap` ni `assert`.

use grafito_geometry::search::{self, SearchError};
use grafito_geometry::Point2;
use thiserror::Error;

/// Presupuesto: máximo de puntos por gadget (G40 = 40, ciclo genérico = 25,
// V31 = 31, Minkowski = 58; margen amplio sin llegar al harness).
pub const MAX_GADGET_POINTS: usize = 512;
/// Dedup de puntos: dos coords a menos de esto son el mismo vértice.
pub const TOL_DEDUP: f64 = 1e-9;
/// Arista unidad si `|d − 1| <= tol` (vía `search::unit_graph_edges`).
pub const TOL_UNIT: f64 = 1e-6;
/// Pares mono `sqrt(11/3)` del G40 (Exoo-Ismailescu).
pub const D_MONO_11_3: f64 = 1.914_854_215_512_676_6; // sqrt(11/3)

/// Error honesto de construcción de gadgets.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum GadgetError {
    /// Entrada vacía donde se requería al menos un punto.
    #[error("gadget: entrada vacía en {0}")]
    Vacio(&'static str),
    /// Se excedió el presupuesto de puntos.
    #[error("gadget: {n} puntos excede el máximo {MAX_GADGET_POINTS} en {donde}")]
    MuchosPuntos { n: usize, donde: &'static str },
    /// Punto no finito.
    #[error("gadget: punto {idx} no finito en {donde}")]
    NoFinito { idx: usize, donde: &'static str },
    /// Ángulo de rombo degenerado o no finito.
    #[error("gadget: ángulo theta={theta} inválido (debe estar en (0, 180))")]
    MalTheta { theta: f64 },
    /// Geometría degenerada (p. ej. terminales coincidentes).
    #[error("gadget: degenerado en {donde}: {detalle}")]
    Degenerado {
        donde: &'static str,
        detalle: String,
    },
    /// Colisión de vértices: ningún corte/signo da un merge limpio.
    #[error("gadget: colisión de vértices en {0}")]
    Colision(&'static str),
    /// Distancia terminal medida distinta de la esperada.
    #[error("gadget: distancia terminal {medida} != esperada {esperada} en {donde}")]
    MalTerminal {
        medida: f64,
        esperada: f64,
        donde: &'static str,
    },
    /// Error del harness reutilizado (`search`).
    #[error("gadget: {0}")]
    Harness(String),
}

impl From<SearchError> for GadgetError {
    fn from(e: SearchError) -> Self {
        Self::Harness(e.0)
    }
}

/// Grafo unit-distance con terminales opcionales (mono-pair).
#[derive(Debug, Clone)]
pub struct Gadget {
    /// Coordenadas por índice de vértice.
    pub puntos: Vec<Point2>,
    /// Aristas unidad `(a, b)` con `a < b`.
    pub aristas: Vec<(usize, usize)>,
    /// Par terminal (mono-pair) si el gadget lo define.
    pub terminales: Option<(usize, usize)>,
}

impl Gadget {
    /// Cantidad de vértices.
    pub fn n(&self) -> usize {
        self.puntos.len()
    }
    /// Cantidad de aristas.
    pub fn m(&self) -> usize {
        self.aristas.len()
    }
    /// Distancia euclídea entre terminales (`None` si no hay par definido).
    pub fn dist_terminal(&self) -> Option<f64> {
        let (a, b) = self.terminales?;
        let pa = self.puntos.get(a)?;
        let pb = self.puntos.get(b)?;
        Some(pa.distance(pb))
    }
}

// --- primitivas (gadF_kit.py:49-102) ----------------------------------------

/// Rota `pt` alrededor de `pivote` por `ang` radianes (gadF_kit.py:49-53).
pub fn rot_about(pt: Point2, pivote: Point2, ang: f64) -> Point2 {
    let (c, s) = (ang.cos(), ang.sin());
    let (dx, dy) = (pt.x - pivote.x, pt.y - pivote.y);
    Point2::new(pivote.x + c * dx - s * dy, pivote.y + s * dx + c * dy)
}

/// Dedup por tolerancia preservando orden (gadF_kit.py:75-90).
fn merge_points(pts: &[Point2]) -> Result<Vec<Point2>, GadgetError> {
    if pts.len() > MAX_GADGET_POINTS * 2 {
        return Err(GadgetError::MuchosPuntos {
            n: pts.len(),
            donde: "merge_points",
        });
    }
    let mut coords: Vec<Point2> = Vec::with_capacity(pts.len());
    for p in pts {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(GadgetError::NoFinito {
                idx: coords.len(),
                donde: "merge_points",
            });
        }
        let mut dup: Option<usize> = None;
        for (i, q) in coords.iter().enumerate() {
            if p.distance(q) < TOL_DEDUP {
                dup = Some(i);
                break;
            }
        }
        if dup.is_none() {
            coords.push(*p);
            if coords.len() > MAX_GADGET_POINTS * 2 {
                return Err(GadgetError::MuchosPuntos {
                    n: coords.len(),
                    donde: "merge_points",
                });
            }
        }
    }
    Ok(coords)
}

/// Aristas unidad de `pts` vía el harness (gadF_kit.py:93-102, sin duplicar).
fn unit_edges(pts: &[Point2]) -> Result<Vec<(usize, usize)>, GadgetError> {
    Ok(search::unit_graph_edges(pts, TOL_UNIT)?)
}

/// Índice del punto más cercano a `teo` (el `find` con `assert` de gadF:135-144).
fn find_nearest(coords: &[Point2], teo: Point2, donde: &'static str) -> Result<usize, GadgetError> {
    let mut best: Option<usize> = None;
    let mut bd = f64::INFINITY;
    for (i, q) in coords.iter().enumerate() {
        let d = teo.distance(q);
        if d < bd {
            bd = d;
            best = Some(i);
        }
    }
    match best {
        Some(i) if bd < TOL_DEDUP => Ok(i),
        _ => Err(GadgetError::Degenerado {
            donde,
            detalle: format!("terminal teórico sin vértice cercano (d={bd})"),
        }),
    }
}

fn check_budget(n: usize, donde: &'static str) -> Result<(), GadgetError> {
    if n == 0 {
        return Err(GadgetError::Vacio(donde));
    }
    if n > MAX_GADGET_POINTS {
        return Err(GadgetError::MuchosPuntos { n, donde });
    }
    Ok(())
}

fn check_finite(pts: &[Point2], donde: &'static str) -> Result<(), GadgetError> {
    for (i, p) in pts.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(GadgetError::NoFinito { idx: i, donde });
        }
    }
    Ok(())
}

// --- rombo + Moser (gadF_kit.py:56-72, 105-168) ------------------------------

/// Rombo unidad `[O, U, V, W]` con lado 1 y ángulo `theta_deg`
/// (gadF_kit.py:56-72). Con 60°: diagonal corta 1, `|O−W| = sqrt(3).
pub fn unit_rhombus(theta_deg: f64) -> Result<[Point2; 4], GadgetError> {
    if !theta_deg.is_finite() || theta_deg <= 0.0 || theta_deg >= 180.0 {
        return Err(GadgetError::MalTheta { theta: theta_deg });
    }
    let t = theta_deg.to_radians();
    let (c, s) = (t.cos(), t.sin());
    Ok([
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(c, s),
        Point2::new(1.0 + c, s),
    ])
}

/// Moser spindle vía kit: 2 rombos sobre O, el 2do rotado por `phi` con
/// `2·r·sin(phi/2) = 1` (gadF_kit.py:105-168).
/// Cerrado: 7v/11a. Abierto: 7v/10a (mono-pair k=3 sin arista terminal).
pub fn moser_spindle(cerrado: bool) -> Result<Gadget, GadgetError> {
    let r1 = unit_rhombus(60.0)?;
    let w1 = r1[3];
    let r = w1.x.hypot(w1.y);
    if r < 0.5 {
        return Err(GadgetError::Degenerado {
            donde: "moser_spindle",
            detalle: format!("radio de terminal r={r} < 0.5"),
        });
    }
    let phi = 2.0 * (1.0 / (2.0 * r)).asin();
    let o = r1[0];
    let r2 = unit_rhombus(60.0)?;
    let p2 = [r2[0], r2[1], r2[2], r2[3]].map(|p| rot_about(p, o, phi));
    let named = [o, r1[1], r1[2], w1, p2[1], p2[2], p2[3]];
    let coords = merge_points(&named)?;
    check_budget(coords.len(), "moser_spindle")?;
    let t0 = find_nearest(&coords, w1, "moser_spindle")?;
    let t1 = find_nearest(&coords, p2[3], "moser_spindle")?;
    let mut aristas = unit_edges(&coords)?;
    if !cerrado {
        aristas.retain(|(a, b)| !(*a == t0 && *b == t1 || *a == t1 && *b == t0));
    }
    Ok(Gadget {
        puntos: coords,
        aristas,
        terminales: Some((t0, t1)),
    })
}

// --- ciclo-J (gadF_kit.py:171-220; jcyJ_kit.py:548-591) -----------------------

/// Ensamblador ciclo-J genérico: 2 gadgets abiertos con terminales a
/// distancia `r_term` en ciclo spindle (jcyJ_kit.py:548-591).
/// Con `r_term = 1` reproduce `build_cycleJ` (gadF_kit.py:171-220).
pub fn assemble_cycle_j(a: &Gadget, b: &Gadget, r_term: f64) -> Result<Gadget, GadgetError> {
    if !r_term.is_finite() || r_term <= 0.0 {
        return Err(GadgetError::Degenerado {
            donde: "assemble_cycle_j",
            detalle: format!("r_term={r_term} no positivo"),
        });
    }
    let (pa_i, qa_i) = a.terminales.ok_or(GadgetError::Degenerado {
        donde: "assemble_cycle_j",
        detalle: "gadget A sin terminales".into(),
    })?;
    let (pb_i, qb_i) = b.terminales.ok_or(GadgetError::Degenerado {
        donde: "assemble_cycle_j",
        detalle: "gadget B sin terminales".into(),
    })?;
    let (Some(pa), Some(qa), Some(pb), Some(qb)) = (
        a.puntos.get(pa_i),
        a.puntos.get(qa_i),
        b.puntos.get(pb_i),
        b.puntos.get(qb_i),
    ) else {
        return Err(GadgetError::Degenerado {
            donde: "assemble_cycle_j",
            detalle: "índice terminal fuera de rango".into(),
        });
    };
    check_finite(&a.puntos, "assemble_cycle_j")?;
    check_finite(&b.puntos, "assemble_cycle_j")?;
    let dvec = (qb.x - pb.x, qb.y - pb.y);
    let d_med = dvec.0.hypot(dvec.1);
    if (d_med - r_term).abs() > 1e-6 {
        return Err(GadgetError::MalTerminal {
            medida: d_med,
            esperada: r_term,
            donde: "assemble_cycle_j",
        });
    }
    // 1) trasladar B: P_B -> Q_A.
    let t = (qa.x - pb.x, qa.y - pb.y);
    let bmov: Vec<Point2> = b
        .puntos
        .iter()
        .map(|p| Point2::new(p.x + t.0, p.y + t.1))
        .collect();
    // 2) corte círculo(pA,1) / círculo(qA,r_term), centros a distancia r_term.
    let d = r_term;
    let ux = (pa.x - qa.x) / d;
    let uy = (pa.y - qa.y) / d;
    let ar = (r_term * r_term - 1.0 + d * d) / (2.0 * d);
    let h2 = r_term * r_term - ar * ar;
    let h = if h2 <= 0.0 { 0.0 } else { h2.sqrt() };
    let mx = qa.x + ar * ux;
    let my = qa.y + ar * uy;
    let (px, py) = (-uy, ux);
    let base = dvec.1.atan2(dvec.0);
    let cortes = [
        Point2::new(mx + h * px, my + h * py),
        Point2::new(mx - h * px, my - h * py),
    ];
    for s in cortes {
        let u = (s.x - qa.x, s.y - qa.y);
        let psi = u.1.atan2(u.0) - base;
        let brot: Vec<Point2> = bmov.iter().map(|p| rot_about(*p, *qa, psi)).collect();
        let mut named: Vec<Point2> = Vec::with_capacity(a.puntos.len() + brot.len());
        named.extend_from_slice(&a.puntos);
        named.extend_from_slice(&brot);
        let coords = merge_points(&named)?;
        if coords.len() == a.puntos.len() + b.puntos.len() - 1 {
            check_budget(coords.len(), "assemble_cycle_j")?;
            let aristas = unit_edges(&coords)?;
            return Ok(Gadget {
                puntos: coords,
                aristas,
                terminales: None,
            });
        }
    }
    Err(GadgetError::Colision("assemble_cycle_j"))
}

/// Ciclo-J mínimo: 2 Mosers abiertos en spindle, 13v (gadF_kit.py:171-220).
pub fn cycle_j2() -> Result<Gadget, GadgetError> {
    let a = moser_spindle(false)?;
    let b = moser_spindle(false)?;
    assemble_cycle_j(&a, &b, 1.0)
}

// --- escalados eta/rho (jcyJ_kit.py:381-545) ----------------------------------

/// `2·arg(eta) = arccos(5/6)`: cuerda `2·r·sin(arg eta) = r/sqrt(3)`
/// (jcyJ_kit.py:381-384, 511-530).
pub fn psi_eta2() -> f64 {
    (5.0f64 / 6.0).acos()
}
/// `arg(rho) = arccos(7/8)`: cuerda `r/2` (jcyJ_kit.py:381-384, 533-545).
pub fn beta_rho() -> f64 {
    (7.0f64 / 8.0).acos()
}
/// Rotación del doble rombo 8/3: `2·sqrt(3)·sin(phi/2) = 8/3`
/// (jcyJ_kit.py:384, 433-474).
pub fn phi_83() -> f64 {
    2.0 * (4.0 / (3.0 * 3.0f64.sqrt())).asin()
}
/// Longitud del mono-pair 8/3 de Harder (jcyJ_kit.py:385).
pub const R83: f64 = 8.0 / 3.0;

/// Doble rombo: mono-pair de longitud 8/3 a nivel k=3, 7v/10a
/// (jcyJ_kit.py:433-474).
pub fn double_rhombus_83() -> Result<Gadget, GadgetError> {
    let r1 = unit_rhombus(60.0)?;
    let o = r1[0];
    let phi = phi_83();
    let p2 = [r1[0], r1[1], r1[2], r1[3]].map(|p| rot_about(p, o, phi));
    let named = [o, r1[1], r1[2], r1[3], p2[1], p2[2], p2[3]];
    let coords = merge_points(&named)?;
    check_budget(coords.len(), "double_rhombus_83")?;
    let t0 = find_nearest(&coords, r1[3], "double_rhombus_83")?;
    let t1 = find_nearest(&coords, p2[3], "double_rhombus_83")?;
    let aristas = unit_edges(&coords)?;
    Ok(Gadget {
        puntos: coords,
        aristas,
        terminales: Some((t0, t1)),
    })
}

/// Rombo unidad como gadget mono-pair `(O, W)`, r = sqrt(3), 4v/5a
/// (jcyJ_kit.py:594-607).
pub fn rhombus_gadget() -> Result<Gadget, GadgetError> {
    let r = unit_rhombus(60.0)?;
    let puntos = vec![r[0], r[1], r[2], r[3]];
    let aristas = unit_edges(&puntos)?;
    Ok(Gadget {
        puntos,
        aristas,
        terminales: Some((0, 3)),
    })
}

/// Núcleo del escalado: 2 copias comparten el vértice `share`; B rota por
/// `ang`; nuevos terminales = `out` de cada copia; abierto (sin arista
/// terminal-terminal), como `build_moser(open)` (jcyJ_kit.py:477-508).
fn scaled_from_pair(
    pts: &[Point2],
    share: usize,
    out: usize,
    ang: f64,
) -> Result<Option<Gadget>, GadgetError> {
    let (Some(t), Some(qa_teo)) = (pts.get(share), pts.get(out)) else {
        return Err(GadgetError::Degenerado {
            donde: "scaled_from_pair",
            detalle: "índice share/out fuera de rango".into(),
        });
    };
    let (t, qa_teo) = (*t, *qa_teo);
    check_finite(pts, "scaled_from_pair")?;
    let bmov: Vec<Point2> = pts.iter().map(|p| rot_about(*p, t, ang)).collect();
    let qb_teo = rot_about(qa_teo, t, ang);
    let mut named: Vec<Point2> = Vec::with_capacity(2 * pts.len());
    named.extend_from_slice(pts);
    named.extend_from_slice(&bmov);
    let coords = merge_points(&named)?;
    if coords.len() != 2 * pts.len() - 1 {
        return Ok(None); // colisión con este signo: el llamador prueba el otro
    }
    let qa = find_nearest(&coords, qa_teo, "scaled_from_pair")?;
    let qb = find_nearest(&coords, qb_teo, "scaled_from_pair")?;
    let mut aristas = unit_edges(&coords)?;
    aristas.retain(|(a, b)| !(*a == qa && *b == qb || *a == qb && *b == qa));
    Ok(Some(Gadget {
        puntos: coords,
        aristas,
        terminales: Some((qa, qb)),
    }))
}

/// Escalado de mono-pair por `ang`: prueba `+ang` y luego `−ang`
/// (primer merge limpio `2n−1` gana).
fn scale_mono_pair(g: &Gadget, ang: f64) -> Result<Gadget, GadgetError> {
    let (share, out) = g.terminales.ok_or(GadgetError::Degenerado {
        donde: "scale_mono_pair",
        detalle: "gadget sin terminales".into(),
    })?;
    for a in [ang, -ang] {
        match scaled_from_pair(&g.puntos, share, out, a)? {
            Some(g2) => {
                check_budget(g2.puntos.len(), "scale_mono_pair")?;
                return Ok(g2);
            }
            None => continue,
        }
    }
    Err(GadgetError::Colision("scale_mono_pair"))
}

/// Escalado eta: `r → r/sqrt(3)` (jcyJ_kit.py:511-530).
pub fn scale_eta(g: &Gadget) -> Result<Gadget, GadgetError> {
    scale_mono_pair(g, psi_eta2())
}

/// Escalado rho: `r → r/2` (jcyJ_kit.py:533-545).
pub fn scale_rho(g: &Gadget) -> Result<Gadget, GadgetError> {
    scale_mono_pair(g, beta_rho())
}

// --- G40 Exoo-Ismailescu (exoL_gadget.py:25-86) -------------------------------

/// Coords exactas del paper `[a,b,c,d] := (a√3/36 + b√11/36, c/36 + d√33/36)`
/// (exoL_gadget.py:30-71). P = índice 0, Q = índice 1, `|PQ| = 8/3`.
pub const G40_RAW: [[i32; 4]; 40] = [
    [0, 0, 0, 0],
    [0, 0, 96, 0],
    [-33, -3, 33, -3],
    [-33, 3, 33, -9],
    [-33, 3, 33, 3],
    [-33, 3, 63, -3],
    [-33, 9, 63, 3],
    [-18, 0, 48, -6],
    [-18, 0, 48, 6],
    [-18, 6, 48, 0],
    [-15, -9, 15, 3],
    [-15, -3, 15, -3],
    [-15, -3, 45, 3],
    [-15, 3, -15, -3],
    [-15, 3, 15, -9],
    [-15, 3, 15, 3],
    [-15, 3, 81, -3],
    [-15, 3, 111, 3],
    [-15, 9, 15, -3],
    [-15, 9, 81, 3],
    [0, -12, 0, 0],
    [0, -6, 30, 0],
    [0, -6, 66, 0],
    [0, 0, 30, -6],
    [0, 0, 30, 6],
    [0, 0, 66, -6],
    [0, 0, 66, 6],
    [0, 6, 0, -6],
    [0, 6, 0, 6],
    [0, 6, 30, 0],
    [0, 6, 66, 0],
    [0, 6, 96, 6],
    [0, 12, 30, 6],
    [0, 12, 66, 6],
    [15, 3, 15, -3],
    [15, 3, 81, 3],
    [18, 0, 48, -6],
    [18, 6, 48, 0],
    [33, 3, 33, -3],
    [33, 3, 63, 3],
];

/// Decodifica una tupla `[a,b,c,d]` a punto (exoL_gadget.py:25-27).
pub fn dec_g40(t: [i32; 4]) -> Point2 {
    let (s3, s11, s33) = (3.0f64.sqrt(), 11.0f64.sqrt(), 33.0f64.sqrt());
    Point2::new(
        f64::from(t[0]) * s3 / 36.0 + f64::from(t[1]) * s11 / 36.0,
        f64::from(t[2]) / 36.0 + f64::from(t[3]) * s33 / 36.0,
    )
}

/// G40: 40 pts, 82 aristas unidad, terminales P=0/Q=1 a 8/3
/// (exoL_gadget.py:74-86). El claim del paper es CONDICIONAL (mono 8/3 solo
/// si se evita mono `sqrt(11/3)`): ver `exo_g40_mono113`.
pub fn exo_g40() -> Result<Gadget, GadgetError> {
    let puntos: Vec<Point2> = G40_RAW.iter().map(|t| dec_g40(*t)).collect();
    check_budget(puntos.len(), "exo_g40")?;
    check_finite(&puntos, "exo_g40")?;
    let aristas = unit_edges(&puntos)?;
    Ok(Gadget {
        puntos,
        aristas,
        terminales: Some((0, 1)),
    })
}

/// Pares a distancia `sqrt(11/3)` del G40 (59 pares; exoL_gadget.py:74-86).
/// Forzarlos bicolor + `P != Q` da UNSAT a k=4 (T3 del paper, p.3-4).
pub fn exo_g40_mono113(puntos: &[Point2]) -> Result<Vec<(usize, usize)>, GadgetError> {
    check_budget(puntos.len(), "exo_g40_mono113")?;
    check_finite(puntos, "exo_g40_mono113")?;
    let d0 = (11.0f64 / 3.0).sqrt();
    let mut pares = Vec::new();
    for i in 0..puntos.len() {
        let Some(pi) = puntos.get(i) else {
            return Err(GadgetError::Degenerado {
                donde: "exo_g40_mono113",
                detalle: "índice fuera de rango".into(),
            });
        };
        for j in (i + 1)..puntos.len() {
            let Some(pj) = puntos.get(j) else {
                return Err(GadgetError::Degenerado {
                    donde: "exo_g40_mono113",
                    detalle: "índice fuera de rango".into(),
                });
            };
            if (pi.distance(pj) - d0).abs() < TOL_UNIT {
                pares.push((i, j));
            }
        }
    }
    Ok(pares)
}

// --- siembra sowL (sowL_gen.py:10-95) ----------------------------------------

/// `eta = arccos(5/6)/2` en radianes (sowL_gen.py:11).
pub fn eta_angle() -> f64 {
    0.5 * (5.0f64 / 6.0).acos()
}

/// `rho = arccos(7/8)` en radianes (sowL_gen.py:12).
pub fn rho_angle() -> f64 {
    (7.0f64 / 8.0).acos()
}

/// Conjunto H exacto de 7 puntos (sowL_gen.py:17-26).
pub fn h_set() -> Vec<Point2> {
    let s = 3.0f64.sqrt() / 2.0;
    vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(-1.0, 0.0),
        Point2::new(0.5, s),
        Point2::new(-0.5, s),
        Point2::new(0.5, -s),
        Point2::new(-0.5, -s),
    ]
}

/// Rota puntos sobre el origen por `ang` (sowL_gen.py:29-31).
pub fn rot_points(pts: &[Point2], ang: f64) -> Result<Vec<Point2>, GadgetError> {
    check_budget(pts.len(), "rot_points")?;
    check_finite(pts, "rot_points")?;
    if !ang.is_finite() {
        return Err(GadgetError::Degenerado {
            donde: "rot_points",
            detalle: format!("ángulo={ang} no finito"),
        });
    }
    let (c, s) = (ang.cos(), ang.sin());
    Ok(pts
        .iter()
        .map(|p| Point2::new(p.x * c - p.y * s, p.x * s + p.y * c))
        .collect())
}

/// Unión con dedup por coordenada (`|dx|<tol && |dy|<tol`, sowL_gen.py:34-40).
pub fn union_all(sets: &[Vec<Point2>]) -> Result<Vec<Point2>, GadgetError> {
    let mut out: Vec<Point2> = Vec::new();
    for s in sets {
        check_finite(s, "union_all")?;
        for p in s {
            let dup = out
                .iter()
                .any(|q| (q.x - p.x).abs() < TOL_DEDUP && (q.y - p.y).abs() < TOL_DEDUP);
            if !dup {
                out.push(*p);
            }
        }
        check_budget(out.len(), "union_all")?;
    }
    if out.is_empty() {
        return Err(GadgetError::Vacio("union_all"));
    }
    Ok(out)
}

/// Suma de Minkowski con dedup (sowL_gen.py:43-50).
pub fn minkowski_sum(a: &[Point2], b: &[Point2]) -> Result<Vec<Point2>, GadgetError> {
    check_budget(a.len(), "minkowski_sum")?;
    check_budget(b.len(), "minkowski_sum")?;
    check_finite(a, "minkowski_sum")?;
    check_finite(b, "minkowski_sum")?;
    let mut out: Vec<Point2> = Vec::new();
    for pa in a {
        for pb in b {
            let p = Point2::new(pa.x + pb.x, pa.y + pb.y);
            let dup = out
                .iter()
                .any(|q| (q.x - p.x).abs() < TOL_DEDUP && (q.y - p.y).abs() < TOL_DEDUP);
            if !dup {
                out.push(p);
            }
        }
        check_budget(out.len(), "minkowski_sum")?;
    }
    if out.is_empty() {
        return Err(GadgetError::Vacio("minkowski_sum"));
    }
    Ok(out)
}

/// `H^k` = unión de `k` copias de H rotadas por múltiplos de eta
/// (`H^0 = H`; sowL_gen.py:92-95).
pub fn h_power(k: usize) -> Result<Vec<Point2>, GadgetError> {
    let h = h_set();
    let n = k.max(1);
    if n > MAX_GADGET_POINTS {
        return Err(GadgetError::MuchosPuntos {
            n,
            donde: "h_power",
        });
    }
    let mut sets: Vec<Vec<Point2>> = Vec::with_capacity(n);
    for i in 0..n {
        sets.push(rot_points(&h, i as f64 * eta_angle())?);
    }
    union_all(&sets)
}

/// `V31` = unión de 5 copias (`i = 0..4`): 31 pts (sowL_gen.py:102-103).
pub fn v31() -> Result<Vec<Point2>, GadgetError> {
    let h = h_set();
    let mut sets: Vec<Vec<Point2>> = Vec::with_capacity(5);
    for i in 0..5 {
        sets.push(rot_points(&h, i as f64 * eta_angle())?);
    }
    union_all(&sets)
}
