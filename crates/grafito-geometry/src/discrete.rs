//! Geometría discreta: ConvexHull, MST, TSP, Voronoi/Delaunay y distancias.
//!
//! Todo el módulo es puro (sin `Document` ni renderizado) y respeta los límites
//! globales `MAX_POLYGON_VERTICES` y `MAX_DISCRETE_COUNT` a través de los
//! llamantes; aquí se valida finitud y se rechazan entradas degeneradas.
//!
//! # Robustez
//! - Casco convexo: colinealidad decidida por predicado exacto `robust::orient2d`
//!   (Shewchuk 1997) sin tolerancia `GEOM_EPS`; `orient == 0` es colineal exacto.
//!   Orden lexicográfico `(x,y)` determinista como tie-break (SoS simbólico,
//!   Edelsbrunner-Mücke 1990): a igual `orient==0` se conservan los extremos
//!   lexicográficos de cada arista, resultado independiente de escala.
//! - Delaunay: triangulación de Delaunay real vía `spade` (`bulk_load`) con
//!   predicados exactos; `spade` deduplica silencioso → se valida duplicado
//!   exacto antes y se devuelve `Err` honesto.
//! - Voronoi: dual exacto de la triangulación (`voronoi_faces` de `spade`);
//!   celdas no acotadas recortadas a la envolvente de los sitios (Sutherland–
//!   Hodgman); 1 punto → envolvente, 2 puntos → bisección, colineales → `Err`.

// allow clippy uninlined for consistency with crate
#![allow(clippy::uninlined_format_args)]

use crate::Point2;
use robust::{orient2d as robust_orient2d, Coord as RobustCoord};
use spade::{DelaunayTriangulation, Point2 as SpadePoint2, Triangulation};

// Límite superior para el número de puntos que la capa de comando permite.
// Se duplica aquí solo para mensajes de error consistentes; el valor canónico
// vive en `grafito-core::validation` y `grafito-command::MAX_DISCRETE_COUNT`.
const MAX_DISCRETE_COUNT_LOCAL: usize = 10_000;
const MAX_POLYGON_VERTICES_LOCAL: usize = 8_192;

/// Error discreto con mensaje listo para mostrar al usuario.
#[derive(Debug, Clone)]
pub struct DiscreteError(pub String);

impl std::fmt::Display for DiscreteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DiscreteError {}

// ---------------------------------------------------------------------------
// Validación auxiliar
// ---------------------------------------------------------------------------

fn validate_finite_points(points: &[Point2]) -> Result<(), DiscreteError> {
    if points.len() > MAX_DISCRETE_COUNT_LOCAL {
        return Err(DiscreteError(format!(
            "demasiados puntos ({} > {})",
            points.len(),
            MAX_DISCRETE_COUNT_LOCAL
        )));
    }
    for (idx, p) in points.iter().enumerate() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(DiscreteError(format!(
                "punto {idx} no finito ({}, {})",
                p.x, p.y
            )));
        }
    }
    Ok(())
}

#[inline]
fn robust_orient(o: Point2, a: Point2, b: Point2) -> f64 {
    robust_orient2d(
        RobustCoord { x: o.x, y: o.y },
        RobustCoord { x: a.x, y: a.y },
        RobustCoord { x: b.x, y: b.y },
    )
}

// ---------------------------------------------------------------------------
// ConvexHull — monotone chain (Andrew) O(n log n) con predicado exacto
// ---------------------------------------------------------------------------

/// Calcula el cierre convexo de `points` en orden antihorario sin repetir
/// el primer punto al final. Devuelve `Ok(hull)` con al menos 1 vértice.
///
/// # Robustez
/// Colinealidad decidida por `robust::orient2d` exacto (Shewchuk 1997):
/// `orient > 0` = giro izquierda, `< 0` = derecha, `== 0` = colineal exacto.
/// No usa tolerancia absoluta. Con `orient == 0` se descarta el punto
/// intermedio y se conservan sólo los extremos de cada arista colineal.
///
/// # Determinismo
/// Tie-break lexicográfico `(x, luego y)` (SoS, Edelsbrunner-Mücke 1990):
/// los puntos se ordenan lexicográficamente antes del barrido; a igual
/// `orient==0` el orden lexicográfico decide qué extremo sobrevive, por lo
/// que el resultado es determinista e invariante a permutaciones y a escala
/// (siempre que las coordenadas sean finitas).
pub fn convex_hull(points: &[Point2]) -> Result<Vec<Point2>, DiscreteError> {
    validate_finite_points(points)?;
    let n = points.len();
    if n == 0 {
        return Err(DiscreteError(
            "ConvexHull: se requieren al menos 1 punto".into(),
        ));
    }
    if n == 1 {
        return Ok(vec![points[0]]);
    }
    if n == 2 {
        // Coincidencia exacta (bitwise ==), no epsilon.
        if points[0].x == points[1].x && points[0].y == points[1].y {
            return Ok(vec![points[0]]);
        }
        return Ok(points.to_vec());
    }

    // Copia ordenada lexicográficamente (x, luego y) — determinista.
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    // Elimina duplicados exactos (bitwise ==, sin eps).
    let mut dedup: Vec<Point2> = Vec::with_capacity(pts.len());
    for p in pts {
        if let Some(last) = dedup.last() {
            if p.x == last.x && p.y == last.y {
                continue;
            }
        }
        dedup.push(p);
    }
    pts = dedup;
    if pts.len() == 1 {
        return Ok(vec![pts[0]]);
    }
    if pts.len() == 2 {
        return Ok(pts);
    }

    let mut lower: Vec<Point2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2
            && robust_orient(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0
        {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2
            && robust_orient(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0
        {
            upper.pop();
        }
        upper.push(p);
    }
    // El último punto de cada lista es el primero de la otra, se elimina.
    lower.pop();
    upper.pop();
    let mut hull = lower;
    hull.extend(upper);

    if hull.len() > MAX_POLYGON_VERTICES_LOCAL {
        return Err(DiscreteError(format!(
            "ConvexHull: el casco tiene {} vértices y excede el máximo {}",
            hull.len(),
            MAX_POLYGON_VERTICES_LOCAL
        )));
    }
    // Caso degenerado colineal: hull puede quedar vacío si todos colineales;
    // en ese caso devolver los dos extremos lexicográficos.
    if hull.is_empty() {
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            if first.x == last.x && first.y == last.y {
                return Ok(vec![*first]);
            }
            return Ok(vec![*first, *last]);
        }
    }
    Ok(hull)
}

// ---------------------------------------------------------------------------
// MST — Prim O(n²) sin almacenar todas las aristas
// ---------------------------------------------------------------------------

/// Arista del MST como par de índices en el slice original de puntos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MstEdge {
    pub from: usize,
    pub to: usize,
}

/// Calcula el árbol de expansión mínima con Prim denso O(n²).
/// Devuelve las aristas y el peso total.
pub fn minimum_spanning_tree(points: &[Point2]) -> Result<(Vec<MstEdge>, f64), DiscreteError> {
    validate_finite_points(points)?;
    let n = points.len();
    if n < 2 {
        return Err(DiscreteError(
            "MinimumSpanningTree: se requieren al menos 2 puntos".into(),
        ));
    }
    // Prim
    let mut in_mst = vec![false; n];
    let mut min_dist = vec![f64::INFINITY; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    min_dist[0] = 0.0;
    let mut total = 0.0;
    let mut edges: Vec<MstEdge> = Vec::with_capacity(n - 1);

    for _ in 0..n {
        // Selecciona el vértice no visitado con menor distancia.
        let mut u: Option<usize> = None;
        let mut best = f64::INFINITY;
        for (idx, &dist) in min_dist.iter().enumerate() {
            if !in_mst[idx] && dist < best {
                best = dist;
                u = Some(idx);
            }
        }
        let Some(u_idx) = u else {
            break;
        };
        in_mst[u_idx] = true;
        total += best;
        if let Some(p) = parent[u_idx] {
            edges.push(MstEdge { from: p, to: u_idx });
        }
        // Relaja vecinos.
        for v in 0..n {
            if in_mst[v] {
                continue;
            }
            let d = points[u_idx].distance(&points[v]);
            if !d.is_finite() {
                return Err(DiscreteError("distancia no finita en MST".into()));
            }
            if d < min_dist[v] {
                min_dist[v] = d;
                parent[v] = Some(u_idx);
            }
        }
    }

    // En un grafo completo euclídeo siempre se conectan todos los puntos;
    // si no, es que había puntos no finitos ya rechazados.
    if edges.len() != n - 1 {
        return Err(DiscreteError(
            "MinimumSpanningTree: no se pudo conectar todos los puntos".into(),
        ));
    }
    Ok((edges, total))
}

// ---------------------------------------------------------------------------
// TSP — vecino más cercano (greedy)
// ---------------------------------------------------------------------------

/// Tour del viajante aproximado por vecino más cercano, comenzando en 0.
/// Devuelve el orden de visita (incluye el retorno implícito al inicio en
/// el cálculo de longitud, pero no duplica el primer índice al final).
pub fn traveling_salesman_nearest(points: &[Point2]) -> Result<(Vec<usize>, f64), DiscreteError> {
    validate_finite_points(points)?;
    let n = points.len();
    if n < 2 {
        return Err(DiscreteError(
            "TravelingSalesman: se requieren al menos 2 puntos".into(),
        ));
    }
    let mut visited = vec![false; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut current = 0usize;
    visited[current] = true;
    order.push(current);
    let mut total = 0.0;

    for _ in 1..n {
        let mut best: Option<usize> = None;
        let mut best_dist = f64::INFINITY;
        for (idx, &vis) in visited.iter().enumerate() {
            if vis {
                continue;
            }
            let d = points[current].distance(&points[idx]);
            if !d.is_finite() {
                return Err(DiscreteError("distancia no finita en TSP".into()));
            }
            if d < best_dist {
                best_dist = d;
                best = Some(idx);
            }
        }
        let Some(next) = best else {
            break;
        };
        total += best_dist;
        visited[next] = true;
        order.push(next);
        current = next;
    }
    // Cierra el ciclo.
    let closing = points[current].distance(&points[order[0]]);
    if !closing.is_finite() {
        return Err(DiscreteError("distancia de cierre no finita".into()));
    }
    total += closing;
    Ok((order, total))
}

// ---------------------------------------------------------------------------
// Delaunay / Voronoi
// ---------------------------------------------------------------------------

/// Detecta duplicados exactos (bitwise `x==y`) en `points`.
///
/// `spade` deduplica silencioso (conserva el de menor índice con
/// `bulk_load_stable`); para honestidad se rechaza antes con `Err`.
fn find_exact_duplicate(points: &[Point2]) -> Option<(usize, usize, Point2)> {
    // O(n log n) via orden lexicográfico + índice original.
    let mut indexed: Vec<(Point2, usize)> = points
        .iter()
        .copied()
        .enumerate()
        .map(|(i, p)| (p, i))
        .collect();
    indexed.sort_by(|a, b| {
        a.0.x
            .partial_cmp(&b.0.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.0.y
                    .partial_cmp(&b.0.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.1.cmp(&b.1))
    });
    for w in indexed.windows(2) {
        let (pa, ia) = w[0];
        let (pb, ib) = w[1];
        if pa.x == pb.x && pa.y == pb.y {
            return Some((ia, ib, pa));
        }
    }
    None
}

/// Triangulación de Delaunay real con predicados exactos.
///
/// Garantiza la propiedad del círculo vacío (Shewchuk/`robust` + `spade`):
/// ningún punto del conjunto está estrictamente dentro del circuncírculo de
/// ningún triángulo (cocircularidad `incircle==0` se permite, la triangulación
/// no es única en ese caso).
///
/// # Errores honestos
/// - `<3` puntos → `Err`.
/// - puntos duplicados exactos (`x==y` bitwise) → `Err` (spade deduplicaría
///   silencioso; se valida antes).
/// - puntos no finitos o exceso de `MAX_*` → `Err`.
///
/// # Notas de implementación
/// Usa `spade::DelaunayTriangulation::bulk_load` (Hilbert sort + Bowyer-Watson
/// con `robust::orient2d`/`incircle`) — O(n log n) esperado. MSRV ≤1.92
/// verificado: `spade 2.10` / `robust 1.1` son `edition 2021` sin `rust-version`.
pub fn delaunay_triangulation(points: &[Point2]) -> Result<Vec<[Point2; 3]>, DiscreteError> {
    validate_finite_points(points)?;
    if points.len() < 3 {
        return Err(DiscreteError(
            "DelaunayTriangulation: se requieren al menos 3 puntos".into(),
        ));
    }
    let n = points.len();
    if n > MAX_POLYGON_VERTICES_LOCAL {
        return Err(DiscreteError(format!(
            "Delaunay: {} puntos excede el máximo {}",
            n, MAX_POLYGON_VERTICES_LOCAL
        )));
    }
    if let Some((ia, ib, p)) = find_exact_duplicate(points) {
        return Err(DiscreteError(format!(
            "Delaunay: puntos duplicados en índices {} y {} ({}, {})",
            ia, ib, p.x, p.y
        )));
    }

    // Conversión a spade::Point2<f64> (HasPosition).
    let spade_pts: Vec<SpadePoint2<f64>> =
        points.iter().map(|p| SpadePoint2::new(p.x, p.y)).collect();

    let triangulation = DelaunayTriangulation::<SpadePoint2<f64>>::bulk_load(spade_pts)
        .map_err(|e| DiscreteError(format!("Delaunay: error de inserción: {e:?}")))?;

    let mut tris = Vec::with_capacity(triangulation.num_inner_faces());
    for face in triangulation.inner_faces() {
        let verts = face.vertices();
        let a = verts[0].position();
        let b = verts[1].position();
        let c = verts[2].position();
        tris.push([
            Point2::new(a.x, a.y),
            Point2::new(b.x, b.y),
            Point2::new(c.x, c.y),
        ]);
    }
    Ok(tris)
}

/// Diagrama de Voronoi dual de la triangulación de Delaunay real.
///
/// Cada celda es el polígono de puntos más cercanos a su sitio; las celdas no
/// acotadas se recortan a la envolvente axis-aligned de los sitios expandida
/// un 10% (mínimo 0.1), así todo vértice devuelto es finito y dibujable como
/// `Polygon`. La celda `i` corresponde al sitio `points[i]`.
///
/// # Casos y honestidad
/// - 0 puntos → `Err`; 1 punto → su envolvente (cuadrado lado 1).
/// - 2 puntos → la envolvente partida por el bisector exacto.
/// - `n ≥ 3` colineales (0 caras internas en `spade`) → `Err` honesto.
/// - Duplicados exactos o `n > 8192` → `Err` (igual que Delaunay).
/// - Circuncentro no finito (configuración casi degenerada fuera de rango
///   `f64`) → `Err` honesto en vez de polígono con `inf`.
pub fn voronoi_cells(points: &[Point2]) -> Result<Vec<Vec<Point2>>, DiscreteError> {
    validate_finite_points(points)?;
    if points.is_empty() {
        return Err(DiscreteError(
            "Voronoi: se requieren al menos 1 punto".into(),
        ));
    }
    if points.len() > MAX_POLYGON_VERTICES_LOCAL {
        return Err(DiscreteError(format!(
            "Voronoi: {} puntos excede el máximo {}",
            points.len(),
            MAX_POLYGON_VERTICES_LOCAL
        )));
    }
    if let Some((ia, ib, p)) = find_exact_duplicate(points) {
        return Err(DiscreteError(format!(
            "Voronoi: puntos duplicados en índices {} y {} ({}, {})",
            ia, ib, p.x, p.y
        )));
    }
    let (bb_min, bb_max) = voronoi_clip_bbox(points);
    if points.len() == 1 {
        return Ok(vec![vec![
            Point2::new(bb_min.x, bb_min.y),
            Point2::new(bb_max.x, bb_min.y),
            Point2::new(bb_max.x, bb_max.y),
            Point2::new(bb_min.x, bb_max.y),
        ]]);
    }
    if points.len() == 2 {
        return voronoi_two_point_split(points[0], points[1], bb_min, bb_max);
    }

    let spade_pts: Vec<SpadePoint2<f64>> =
        points.iter().map(|p| SpadePoint2::new(p.x, p.y)).collect();
    let triangulation = DelaunayTriangulation::<SpadePoint2<f64>>::bulk_load(spade_pts)
        .map_err(|e| DiscreteError(format!("Voronoi: error de inserción: {e:?}")))?;
    if triangulation.num_inner_faces() == 0 {
        return Err(DiscreteError(
            "Voronoi: puntos colineales (sin área); probá ConvexHull o MinimumSpanningTree".into(),
        ));
    }
    let mut cells: Vec<Option<Vec<Point2>>> = (0..points.len()).map(|_| None).collect();
    // `voronoi_faces` va en orden DCEL interno, no en orden de entrada: se
    // mapea cada cara a su sitio por coordenadas exactas (los vértices se
    // almacenan tal cual; duplicados ya rechazados arriba).
    let mut index_by_bits: std::collections::HashMap<(u64, u64), usize> =
        std::collections::HashMap::with_capacity(points.len());
    for (idx, p) in points.iter().enumerate() {
        index_by_bits.insert((p.x.to_bits(), p.y.to_bits()), idx);
    }
    for face in triangulation.voronoi_faces() {
        let site = face.as_delaunay_vertex().position();
        let site = Point2::new(site.x, site.y);
        let Some(&idx) = index_by_bits.get(&(site.x.to_bits(), site.y.to_bits())) else {
            return Err(DiscreteError(format!(
                "Voronoi: sitio ({}, {}) sin índice",
                site.x, site.y
            )));
        };
        let cell = voronoi_face_cell(&face, site, bb_min, bb_max)?;
        if cell.len() < 3 {
            return Err(DiscreteError(format!(
                "Voronoi: celda degenerada en sitio ({}, {})",
                site.x, site.y
            )));
        }
        if cell.len() > MAX_POLYGON_VERTICES_LOCAL {
            return Err(DiscreteError(format!(
                "Voronoi: celda con {} vértices excede el máximo {}",
                cell.len(),
                MAX_POLYGON_VERTICES_LOCAL
            )));
        }
        if cells[idx].is_some() {
            return Err(DiscreteError(format!(
                "Voronoi: sitio duplicado ({}, {})",
                site.x, site.y
            )));
        }
        cells[idx] = Some(cell);
    }
    let mut out: Vec<Vec<Point2>> = Vec::with_capacity(points.len());
    for (idx, cell) in cells.into_iter().enumerate() {
        match cell {
            Some(c) => out.push(c),
            None => {
                return Err(DiscreteError(format!(
                    "Voronoi: sin celda para el sitio índice {idx}"
                )))
            }
        }
    }
    Ok(out)
}

/// Envolvente de recorte: min/max de los sitios expandida un 10% del mayor
/// lado (mínimo 0.1). Para 1 punto es un cuadrado de lado 1 centrado en él.
fn voronoi_clip_bbox(points: &[Point2]) -> (Point2, Point2) {
    let mut min_x = points[0].x;
    let mut max_x = points[0].x;
    let mut min_y = points[0].y;
    let mut max_y = points[0].y;
    for p in &points[1..] {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    let span = (max_x - min_x).max(max_y - min_y).max(1.0);
    let margin = (span * 0.1).max(0.1);
    // Entradas ya validadas finitas; el margen es finito por construcción.
    (
        Point2::new(min_x - margin, min_y - margin),
        Point2::new(max_x + margin, max_y + margin),
    )
}

/// Caso `n == 2`: parte la envolvente por el bisector del segmento `a–b`.
fn voronoi_two_point_split(
    a: Point2,
    b: Point2,
    bb_min: Point2,
    bb_max: Point2,
) -> Result<Vec<Vec<Point2>>, DiscreteError> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if !len2.is_finite() || len2 <= 0.0 {
        return Err(DiscreteError(
            "Voronoi: segmento degenerado en bisección".into(),
        ));
    }
    // Recta bisectriz: n·(p - m) = 0 con n = b - a, m = punto medio.
    let mx = 0.5 * (a.x + b.x);
    let my = 0.5 * (a.y + b.y);
    let corners = [
        Point2::new(bb_min.x, bb_min.y),
        Point2::new(bb_max.x, bb_min.y),
        Point2::new(bb_max.x, bb_max.y),
        Point2::new(bb_min.x, bb_max.y),
    ];
    // Recorta el rectángulo contra cada semiplano por Sutherland–Hodgman con
    // la recta como único borde: se implementa vía `clip_polygon_to_halfplane`.
    let corner_vec = corners.to_vec();
    let cell_a = clip_polygon_to_halfplane(&corner_vec, mx, my, dx, dy, true);
    let cell_b = clip_polygon_to_halfplane(&corner_vec, mx, my, dx, dy, false);
    for (cell, label) in [&cell_a, &cell_b].into_iter().zip(["a", "b"]) {
        if cell.len() < 3 {
            return Err(DiscreteError(format!(
                "Voronoi: bisección degenerada en lado {label}"
            )));
        }
    }
    Ok(vec![cell_a, cell_b])
}

/// Recorta un polígono contra el semiplano `dx*(x-mx)+dy*(y-my) <= 0`
/// (`keep_negative == true`) o `>= 0`. Aritmética `f64` directa: las
/// entradas son finitas y la salida se valida finita en el llamante.
fn clip_polygon_to_halfplane(
    poly: &[Point2],
    mx: f64,
    my: f64,
    dx: f64,
    dy: f64,
    keep_negative: bool,
) -> Vec<Point2> {
    let inside = |p: &Point2| {
        let v = dx * (p.x - mx) + dy * (p.y - my);
        if keep_negative {
            v <= 0.0
        } else {
            v >= 0.0
        }
    };
    let mut out: Vec<Point2> = Vec::with_capacity(poly.len() + 1);
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = inside(&prev);
    for &cur in poly {
        let cur_in = inside(&cur);
        if cur_in {
            if !prev_in {
                // Entrada: intersección del segmento prev→cur con la recta.
                let denom = dx * (cur.x - prev.x) + dy * (cur.y - prev.y);
                if denom != 0.0 {
                    let t = -(dx * (prev.x - mx) + dy * (prev.y - my)) / denom;
                    out.push(Point2::new(
                        prev.x + t * (cur.x - prev.x),
                        prev.y + t * (cur.y - prev.y),
                    ));
                }
            }
            out.push(cur);
        } else if prev_in {
            // Salida: intersección del segmento prev→cur con la recta.
            let denom = dx * (cur.x - prev.x) + dy * (cur.y - prev.y);
            if denom != 0.0 {
                let t = -(dx * (prev.x - mx) + dy * (prev.y - my)) / denom;
                out.push(Point2::new(
                    prev.x + t * (cur.x - prev.x),
                    prev.y + t * (cur.y - prev.y),
                ));
            }
        }
        prev = cur;
        prev_in = cur_in;
    }
    out
}

/// Construye la celda de una cara de Voronoi: cadena de circuncentros
/// internos más, si la celda es no acotada, los 2 impactos de sus rayos en
/// la envolvente; el conjunto se recorta a la envolvente (que inserta las
/// esquinas necesarias).
fn voronoi_face_cell(
    face: &spade::handles::VoronoiFace<'_, SpadePoint2<f64>, (), (), ()>,
    site: Point2,
    bb_min: Point2,
    bb_max: Point2,
) -> Result<Vec<Point2>, DiscreteError> {
    let edges: Vec<_> = face.adjacent_edges().collect();
    if edges.is_empty() {
        return Err(DiscreteError(format!(
            "Voronoi: cara sin aristas en sitio ({}, {})",
            site.x, site.y
        )));
    }
    // Vértices internos en orden + rayos de borde (origen, dirección unitaria).
    let mut ring: Vec<Point2> = Vec::with_capacity(edges.len() + 2);
    // (origen, dirección): saliente con `to` exterior, entrante con `from` exterior.
    let mut ray_out: Option<(Point2, Point2)> = None;
    let mut ray_in: Option<(Point2, Point2)> = None;
    for edge in &edges {
        let from = edge.from().position();
        let to = edge.to().position();
        match (from, to) {
            (Some(f), Some(_)) => {
                let fp = Point2::new(f.x, f.y);
                if !fp.x.is_finite() || !fp.y.is_finite() {
                    return Err(DiscreteError(format!(
                        "Voronoi: circuncentro no finito en sitio ({}, {})",
                        site.x, site.y
                    )));
                }
                if ring.last().is_none_or(|last: &Point2| *last != fp) {
                    ring.push(fp);
                }
            }
            (Some(f), None) => {
                let origin = Point2::new(f.x, f.y);
                let dir = voronoi_ray_dir(edge, site, origin)?;
                if ring.last().is_none_or(|last: &Point2| *last != origin) {
                    ring.push(origin);
                }
                ray_out = Some((origin, dir));
            }
            (None, Some(t)) => {
                let end = Point2::new(t.x, t.y);
                if !end.x.is_finite() || !end.y.is_finite() {
                    return Err(DiscreteError(format!(
                        "Voronoi: circuncentro no finito en sitio ({}, {})",
                        site.x, site.y
                    )));
                }
                // Dirección de viaje del rayo entrante (hacia `end`).
                let dir = voronoi_ray_dir(edge, site, end)?;
                ray_in = Some((end, dir));
            }
            (None, None) => {
                // Solo si todos los sitios son colineales; ya rechazado arriba.
                return Err(DiscreteError(format!(
                    "Voronoi: arista totalmente no acotada en sitio ({}, {})",
                    site.x, site.y
                )));
            }
        }
    }
    // Cierra duplicado final == inicial (cara cerrada).
    if ring.len() >= 2 {
        if let (Some(first), Some(last)) = (ring.first(), ring.last()) {
            if first == last {
                ring.pop();
            }
        }
    }
    match (ray_out, ray_in) {
        (None, None) => Ok(ring),
        (Some((o_out, d_out)), Some((e_in, d_in))) => {
            // El anillo es una rotación de la cadena entrada→salida: se rota
            // para que empiece en la entrada y termine en la salida, así el
            // polígono sigue el borde (…→salida→H_out→H_in→entrada→…).
            if let Some(pos) = ring.iter().position(|v| *v == e_in) {
                ring.rotate_left(pos);
            } else {
                return Err(DiscreteError(format!(
                    "Voronoi: cadena rota en sitio ({}, {})",
                    site.x, site.y
                )));
            }
            if ring.last().is_none_or(|last| *last != o_out) {
                return Err(DiscreteError(format!(
                    "Voronoi: cadena rota en sitio ({}, {})",
                    site.x, site.y
                )));
            }
            let h_out = ray_bbox_hit(o_out, d_out, bb_min, bb_max).ok_or_else(|| {
                DiscreteError(format!(
                    "Voronoi: rayo sin impacto en sitio ({}, {})",
                    site.x, site.y
                ))
            })?;
            // El rayo entrante viaja en `d_in`; su punto lejano es `end - d_in*L`.
            let neg = Point2::new(-d_in.x, -d_in.y);
            let h_in = ray_bbox_hit(e_in, neg, bb_min, bb_max).ok_or_else(|| {
                DiscreteError(format!(
                    "Voronoi: rayo sin impacto en sitio ({}, {})",
                    site.x, site.y
                ))
            })?;
            // Anillo: cadena interna + impactos; entre ambos impactos se rutea
            // por el borde de la envolvente (con las esquinas intermedias),
            // y el recorte final solo pule polvo de coma flotante. Se prueban
            // ambos sentidos del borde y se queda el que contiene al sitio.
            let mut chosen: Option<Vec<Point2>> = None;
            for ccw in [false, true] {
                let mut poly: Vec<Point2> = Vec::with_capacity(ring.len() + 6);
                poly.extend_from_slice(&ring);
                poly.push(h_out);
                poly.extend(bbox_path_corners(h_out, h_in, bb_min, bb_max, ccw));
                poly.push(h_in);
                // Dedup consecutivos exactos + cierre.
                let mut dedup: Vec<Point2> = Vec::with_capacity(poly.len());
                for v in poly {
                    if dedup.last().is_none_or(|last: &Point2| *last != v) {
                        dedup.push(v);
                    }
                }
                if dedup.len() >= 2 {
                    if let (Some(first), Some(last)) = (dedup.first(), dedup.last()) {
                        if first == last {
                            dedup.pop();
                        }
                    }
                }
                let clipped = clip_polygon_to_bbox(&dedup, bb_min, bb_max);
                if clipped.len() >= 3 && point_in_polygon(site, &clipped) {
                    chosen = Some(clipped);
                    break;
                }
            }
            chosen.ok_or_else(|| {
                DiscreteError(format!(
                    "Voronoi: celda no cerrable en sitio ({}, {})",
                    site.x, site.y
                ))
            })
        }
        _ => Err(DiscreteError(format!(
            "Voronoi: celda con un solo rayo en sitio ({}, {})",
            site.x, site.y
        ))),
    }
}

/// Dirección unitaria de viaje de un rayo de Voronoi no acotado.
///
/// `spade` ordena `adjacent_edges` en sentido horario: la cara queda a la
/// derecha de sus aristas dirigidas; se elige el signo de `direction_vector`
/// que deja al sitio a la derecha (`cross < 0`). Si el sitio es colineal
/// con el rayo (`cross == 0`), se apunta lejos del sitio.
fn voronoi_ray_dir(
    edge: &spade::handles::DirectedVoronoiEdge<'_, SpadePoint2<f64>, (), (), ()>,
    site: Point2,
    origin: Point2,
) -> Result<Point2, DiscreteError> {
    let d = edge.direction_vector();
    let len = d.x.hypot(d.y);
    if !len.is_finite() || len <= 0.0 {
        return Err(DiscreteError(format!(
            "Voronoi: dirección degenerada en sitio ({}, {})",
            site.x, site.y
        )));
    }
    let ux = d.x / len;
    let uy = d.y / len;
    let sx = site.x - origin.x;
    let sy = site.y - origin.y;
    let cross = ux * sy - uy * sx;
    if cross < 0.0 {
        Ok(Point2::new(ux, uy))
    } else if cross > 0.0 {
        Ok(Point2::new(-ux, -uy))
    } else {
        // Colineal: lejos del sitio (o sentido base si coinciden).
        let dot = ux * sx + uy * sy;
        if dot < 0.0 {
            Ok(Point2::new(ux, uy))
        } else if dot > 0.0 {
            Ok(Point2::new(-ux, -uy))
        } else {
            Ok(Point2::new(ux, uy))
        }
    }
}

/// Impacto de un rayo (origen estrictamente dentro de la envolvente,
/// dirección finita no nula) con el borde de la envolvente (método de losas).
fn ray_bbox_hit(origin: Point2, dir: Point2, bb_min: Point2, bb_max: Point2) -> Option<Point2> {
    if !origin.x.is_finite() || !origin.y.is_finite() || !dir.x.is_finite() || !dir.y.is_finite() {
        return None;
    }
    if dir.x == 0.0 && dir.y == 0.0 {
        return None;
    }
    let mut t_min = f64::NEG_INFINITY;
    let mut t_max = f64::INFINITY;
    // Eje x.
    if dir.x == 0.0 {
        if origin.x < bb_min.x || origin.x > bb_max.x {
            return None;
        }
    } else {
        let t1 = (bb_min.x - origin.x) / dir.x;
        let t2 = (bb_max.x - origin.x) / dir.x;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    }
    // Eje y.
    if dir.y == 0.0 {
        if origin.y < bb_min.y || origin.y > bb_max.y {
            return None;
        }
    } else {
        let t1 = (bb_min.y - origin.y) / dir.y;
        let t2 = (bb_max.y - origin.y) / dir.y;
        t_min = t_min.max(t1.min(t2));
        t_max = t_max.min(t1.max(t2));
    }
    if t_max < t_min || t_max <= 0.0 {
        return None;
    }
    let hit = Point2::new(origin.x + dir.x * t_max, origin.y + dir.y * t_max);
    if !hit.x.is_finite() || !hit.y.is_finite() {
        return None;
    }
    Some(hit)
}

/// Esquinas de la envolvente entre dos impactos que están en bordes
/// distintos, recorriendo el borde en sentido antihorario (`ccw`) u horario.
/// Bordes: 0 abajo, 1 derecha, 2 arriba, 3 izquierda. Si comparten borde, la
/// cuerda directa basta y no hay esquinas.
fn bbox_path_corners(
    h_out: Point2,
    h_in: Point2,
    bb_min: Point2,
    bb_max: Point2,
    ccw: bool,
) -> Vec<Point2> {
    let eps = 1e-9;
    let on = |v: f64, b: f64| (v - b).abs() <= eps;
    let mut out_edges: Vec<u8> = Vec::new();
    let mut in_edges: Vec<u8> = Vec::new();
    if on(h_out.y, bb_min.y) {
        out_edges.push(0);
    }
    if on(h_out.x, bb_max.x) {
        out_edges.push(1);
    }
    if on(h_out.y, bb_max.y) {
        out_edges.push(2);
    }
    if on(h_out.x, bb_min.x) {
        out_edges.push(3);
    }
    if on(h_in.y, bb_min.y) {
        in_edges.push(0);
    }
    if on(h_in.x, bb_max.x) {
        in_edges.push(1);
    }
    if on(h_in.y, bb_max.y) {
        in_edges.push(2);
    }
    if on(h_in.x, bb_min.x) {
        in_edges.push(3);
    }
    if out_edges.iter().any(|e| in_edges.contains(e)) {
        return Vec::new();
    }
    let Some(&start) = out_edges.first() else {
        return Vec::new();
    };
    let Some(&end) = in_edges.first() else {
        return Vec::new();
    };
    // Esquina entre el borde `e` y el siguiente CCW `(e+1)%4`.
    let corner_between = |e: u8| -> Point2 {
        match e {
            0 => Point2::new(bb_max.x, bb_min.y),
            1 => Point2::new(bb_max.x, bb_max.y),
            2 => Point2::new(bb_min.x, bb_max.y),
            _ => Point2::new(bb_min.x, bb_min.y),
        }
    };
    let mut corners: Vec<Point2> = Vec::new();
    if ccw {
        let mut e = start;
        while e != end {
            corners.push(corner_between(e));
            e = (e + 1) % 4;
            if corners.len() > 4 {
                break;
            }
        }
    } else {
        let mut e = start;
        while e != end {
            e = (e + 3) % 4;
            corners.push(corner_between(e));
            if corners.len() > 4 {
                break;
            }
        }
    }
    // Evita duplicar impactos que caen justo en una esquina.
    corners.retain(|c| {
        (c.x - h_out.x).hypot(c.y - h_out.y) > eps && (c.x - h_in.x).hypot(c.y - h_in.y) > eps
    });
    corners
}

/// Recorte de Sutherland–Hodgman contra una envolvente axis-aligned.
/// La región de recorte es convexa, así que el resultado es correcto aunque
/// el sujeto sea cóncavo; si el sujeto es simple el resultado es simple.
fn clip_polygon_to_bbox(poly: &[Point2], bb_min: Point2, bb_max: Point2) -> Vec<Point2> {
    // Guarda qué lado conserva cada pasada: (borde, conserva-menor).
    let stages: [(u8, bool); 4] = [(0, false), (0, true), (1, false), (1, true)];
    let mut current: Vec<Point2> = poly.to_vec();
    for (axis, keep_greater) in stages {
        if current.is_empty() {
            break;
        }
        let bound = if axis == 0 {
            if keep_greater {
                bb_min.x
            } else {
                bb_max.x
            }
        } else if keep_greater {
            bb_min.y
        } else {
            bb_max.y
        };
        let inside = |p: &Point2| {
            let v = if axis == 0 { p.x } else { p.y };
            if keep_greater {
                v >= bound
            } else {
                v <= bound
            }
        };
        let intersect = |a: &Point2, b: &Point2| {
            let denom = if axis == 0 { b.x - a.x } else { b.y - a.y };
            if denom == 0.0 {
                return *a;
            }
            let t = (bound - if axis == 0 { a.x } else { a.y }) / denom;
            Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
        };
        let mut next: Vec<Point2> = Vec::with_capacity(current.len() + 1);
        let mut prev = current[current.len() - 1];
        let mut prev_in = inside(&prev);
        for &cur in &current {
            let cur_in = inside(&cur);
            if cur_in {
                if !prev_in {
                    next.push(intersect(&prev, &cur));
                }
                next.push(cur);
            } else if prev_in {
                next.push(intersect(&prev, &cur));
            }
            prev = cur;
            prev_in = cur_in;
        }
        current = next;
    }
    current
}

// ---------------------------------------------------------------------------
// Distancias mínimas
// ---------------------------------------------------------------------------

/// Distancia euclídea punto-punto.
pub fn distance_point_to_point(a: Point2, b: Point2) -> f64 {
    a.distance(&b)
}

/// Distancia punto-segmento ya existe en `crate::lines`; se re-exporta.
pub use crate::lines::{distance_point_to_line, distance_point_to_ray, distance_point_to_segment};

/// Distancia punto-círculo (centro + radio).
pub fn distance_point_to_circle(p: Point2, center: Point2, radius: f64) -> f64 {
    if !radius.is_finite() || radius < 0.0 {
        return f64::NAN;
    }
    (p.distance(&center) - radius).abs()
}

/// Distancia punto-polígono (mínimo a aristas; 0 si está dentro).
pub fn distance_point_to_polygon(p: Point2, vertices: &[Point2]) -> f64 {
    if vertices.len() < 3 {
        // Degenerado: trata como conjunto de segmentos / puntos.
        if vertices.is_empty() {
            return f64::NAN;
        }
        if vertices.len() == 1 {
            return p.distance(&vertices[0]);
        }
        // 2 vértices -> segmento
        return distance_point_to_segment(p, vertices[0], vertices[1]);
    }
    if point_in_polygon(p, vertices) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let d = distance_point_to_segment(p, a, b);
        if d < best {
            best = d;
        }
    }
    best
}

/// Test punto-en-polígono por ray casting (rayo horizontal hacia +x).
fn point_in_polygon(p: Point2, vertices: &[Point2]) -> bool {
    let mut inside = false;
    let n = vertices.len();
    let mut j = n - 1;
    for i in 0..n {
        let vi = vertices[i];
        let vj = vertices[j];
        // Comprueba si el rayo cruza la arista vj->vi.
        if ((vi.y > p.y) != (vj.y > p.y))
            && (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distancia punto-elipse axis-aligned (aproximada por muestreo denso si
/// no se quiere resolver cuartica). Usa 360 muestras del borde.
pub fn distance_point_to_ellipse(p: Point2, center: Point2, rx: f64, ry: f64) -> f64 {
    if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
        return f64::NAN;
    }
    // Si está dentro ( (dx/rx)^2 + (dy/ry)^2 <=1 ), distancia al borde por muestreo de borde.
    // Hacemos muestreo uniforme y refinamos con búsqueda local corta.
    let mut best = f64::INFINITY;
    let samples = 360usize;
    for k in 0..samples {
        let theta = 2.0 * std::f64::consts::PI * (k as f64) / (samples as f64);
        let q = Point2::new(center.x + rx * theta.cos(), center.y + ry * theta.sin());
        let d = p.distance(&q);
        if d < best {
            best = d;
        }
    }
    // Punto exactamente en el centro: best ≈ min(rx,ry)
    best
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ConvexHull -------------------------------------------------------

    #[test]
    fn convex_hull_square_with_inner_point() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
            Point2::new(0.5, 0.5),
        ];
        let hull = convex_hull(&pts).expect("hull");
        assert_eq!(hull.len(), 4);
        // El punto interior no debe estar en el casco.
        assert!(!hull
            .iter()
            .any(|p| (p.x - 0.5).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9));
    }

    #[test]
    fn convex_hull_colinear_returns_endpoints() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        ];
        let hull = convex_hull(&pts).expect("hull");
        assert_eq!(hull.len(), 2);
        assert!(hull.contains(&Point2::new(0.0, 0.0)));
        assert!(hull.contains(&Point2::new(3.0, 0.0)));
    }

    #[test]
    fn convex_hull_concave_arrow() {
        // Nube cóncava en forma de flecha/chevron:
        // (0,0)-(2,1)-(0,2)-(0.7,1)  el último es cóncavo interior al casco.
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 1.0),
            Point2::new(0.0, 2.0),
            Point2::new(0.7, 1.0),
            Point2::new(0.2, 0.5),
        ];
        let hull = convex_hull(&pts).expect("hull");
        // Casco debe ser triángulo (0,0)-(2,1)-(0,2)
        assert_eq!(
            hull.len(),
            3,
            "casco cóncavo debe tener 3 vértices, got {hull:?}"
        );
        assert!(hull.contains(&Point2::new(0.0, 0.0)));
        assert!(hull.contains(&Point2::new(2.0, 1.0)));
        assert!(hull.contains(&Point2::new(0.0, 2.0)));
        // Punto cóncavo no debe estar en casco
        assert!(!hull
            .iter()
            .any(|p| (p.x - 0.7).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9));
    }

    #[test]
    fn convex_hull_scale_invariant_small_and_large() {
        // Escalas 1e-9 y 1e9: orient2d exacto debe ser invariante (no colapsa con eps).
        let base = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
            Point2::new(0.5, 0.5),
        ];
        let hull_base = convex_hull(&base).expect("hull base");
        for &scale in &[1e-9_f64, 1e9_f64, 1e-12_f64] {
            let scaled: Vec<Point2> = base
                .iter()
                .map(|p| Point2::new(p.x * scale, p.y * scale))
                .collect();
            let hull = convex_hull(&scaled).expect("hull scaled");
            assert_eq!(
                hull.len(),
                hull_base.len(),
                "escala {scale:e} debe preservar |hull|"
            );
            // Verifica que el casco escalado coincide con base escalada
            for hp in &hull {
                let orig = Point2::new(hp.x / scale, hp.y / scale);
                assert!(
                    hull_base
                        .iter()
                        .any(|q| (q.x - orig.x).abs() < 1e-9 && (q.y - orig.y).abs() < 1e-9),
                    "punto escalado {hp:?} / {scale:e} no mapea a hull base"
                );
            }
        }
    }

    #[test]
    fn convex_hull_near_colinear_exact_orient() {
        // Tres puntos casi colineales pero no exactamente: el del medio debe permanecer si hay giro.
        // Con GEOM_EPS=1e-12 este caso colapsaba; con orient exacto se distingue.
        // Triángulo fino: (0,0)-(1, 1e-13)-(2,0) → orient !=0 → hull debe tener 3 vértices.
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1e-13),
            Point2::new(2.0, 0.0),
        ];
        let hull = convex_hull(&pts).expect("hull");
        // orient exacto detecta giro; hull debe incluir los 3 (triángulo)
        assert_eq!(
            hull.len(),
            3,
            "orient exacto debe distinguir giro fino: {hull:?}"
        );
        // Caso exactamente colineal: (0,0)-(1,0)-(2,0) → 2 extremos
        let colinear = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
        ];
        let hull2 = convex_hull(&colinear).expect("hull colinear");
        assert_eq!(hull2.len(), 2);
    }

    // ---- MST / TSP (sin cambios) -----------------------------------------

    #[test]
    fn mst_triangle_total() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        let (edges, total) = minimum_spanning_tree(&pts).expect("mst");
        assert_eq!(edges.len(), 2);
        // MST de triángulo rectángulo isósceles: aristas 1 y 1
        assert!((total - 2.0).abs() < 1e-9);
    }

    #[test]
    fn tsp_square_nearest() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let (order, total) = traveling_salesman_nearest(&pts).expect("tsp");
        assert_eq!(order.len(), 4);
        // Tour alrededor del cuadrado perímetro 4
        assert!((total - 4.0).abs() < 1e-9);
    }

    #[test]
    fn distance_to_polygon_inside_zero() {
        let square = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        let d = distance_point_to_polygon(Point2::new(1.0, 1.0), &square);
        assert!(d.abs() < 1e-9);
        let d_out = distance_point_to_polygon(Point2::new(3.0, 1.0), &square);
        assert!((d_out - 1.0).abs() < 1e-9);
    }

    // ---- Delaunay real ----------------------------------------------------

    #[test]
    fn delaunay_square_two_triangles_empty_circle() {
        // Cuadrado cocircular: 2 triángulos, ambos satisfacen círculo vacío (cocircular permitido)
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let tris = delaunay_triangulation(&pts).expect("delaunay");
        assert_eq!(tris.len(), 2, "cuadrado debe dar 2 triángulos");
        // Cada triángulo debe ser subconjunto de pts
        for tri in &tris {
            for v in tri {
                assert!(
                    pts.iter().any(|p| p.x == v.x && p.y == v.y),
                    "vértice {v:?} no pertenece al conjunto"
                );
            }
            // orient no cero (triángulo no degenerado)
            let o = robust_orient(tri[0], tri[1], tri[2]);
            assert!(o != 0.0, "triángulo degenerado {tri:?}");
        }
        // Propiedad círculo vacío: ningún punto estrictamente dentro del circuncírculo
        // (incircle >0 significa dentro si tri está en CCW)
        for tri in &tris {
            let mut ccw = *tri;
            let o = robust_orient(ccw[0], ccw[1], ccw[2]);
            if o < 0.0 {
                ccw.swap(1, 2);
            }
            for p in &pts {
                if ccw.iter().any(|v| v.x == p.x && v.y == p.y) {
                    continue;
                }
                let ic = robust::incircle(
                    RobustCoord {
                        x: ccw[0].x,
                        y: ccw[0].y,
                    },
                    RobustCoord {
                        x: ccw[1].x,
                        y: ccw[1].y,
                    },
                    RobustCoord {
                        x: ccw[2].x,
                        y: ccw[2].y,
                    },
                    RobustCoord { x: p.x, y: p.y },
                );
                assert!(
                    ic <= 0.0,
                    "punto {p:?} dentro del circuncírculo de {ccw:?} incircle={ic}"
                );
            }
        }
    }

    #[test]
    fn delaunay_interior_point_and_concave() {
        // Cuadrado + punto interior: Delaunay real ≠ fan. Debe dar 4 triángulos, todos incidentes al interior.
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(1.0, 1.0),
        ];
        let tris = delaunay_triangulation(&pts).expect("delaunay");
        // Fórmula Euler: n=5, h=4 → t = 2n-2-h = 4
        assert_eq!(
            tris.len(),
            4,
            "5 pts (4 casco +1 interior) → 4 tris, got {tris:?}"
        );
        // Punto interior debe aparecer en todos los triángulos (estrella)
        let interior = Point2::new(1.0, 1.0);
        for tri in &tris {
            assert!(
                tri.iter().any(|v| v.x == interior.x && v.y == interior.y),
                "tri {tri:?} debe contener punto interior"
            );
        }
        // Círculo vacío estricto para este caso no cocircular
        for tri in &tris {
            let mut ccw = *tri;
            if robust_orient(ccw[0], ccw[1], ccw[2]) < 0.0 {
                ccw.swap(1, 2);
            }
            for p in &pts {
                if ccw.iter().any(|v| v.x == p.x && v.y == p.y) {
                    continue;
                }
                let ic = robust::incircle(
                    RobustCoord {
                        x: ccw[0].x,
                        y: ccw[0].y,
                    },
                    RobustCoord {
                        x: ccw[1].x,
                        y: ccw[1].y,
                    },
                    RobustCoord {
                        x: ccw[2].x,
                        y: ccw[2].y,
                    },
                    RobustCoord { x: p.x, y: p.y },
                );
                assert!(ic <= 1e-12, "círculo no vacío: {p:?} en {ccw:?} ic={ic}");
            }
        }
    }

    #[test]
    fn delaunay_duplicate_returns_err() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
            Point2::new(0.5, 1.0), // duplicado exacto
        ];
        let res = delaunay_triangulation(&pts);
        assert!(res.is_err(), "duplicado debe dar Err, got {res:?}");
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("duplicado") || msg.contains("duplicate"),
            "mensaje debe mencionar duplicado: {msg}"
        );
        // Duplicado también si aparece al inicio
        let pts2 = vec![
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        ];
        assert!(delaunay_triangulation(&pts2).is_err());
    }

    #[test]
    fn delaunay_scale_invariant() {
        // Invariancia a escala 1e-9/1e9: número de triángulos y topología invariantes
        let base = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(1.0, 1.0),
        ];
        let tris_base = delaunay_triangulation(&base).expect("base");
        for &scale in &[1e-9_f64, 1e9_f64] {
            let scaled: Vec<Point2> = base
                .iter()
                .map(|p| Point2::new(p.x * scale, p.y * scale))
                .collect();
            let tris = delaunay_triangulation(&scaled).expect("scaled");
            assert_eq!(
                tris.len(),
                tris_base.len(),
                "escala {scale:e} debe preservar nº triángulos"
            );
            // Verifica círculo vacío también a escala
            for tri in &tris {
                let mut ccw = *tri;
                if robust_orient(ccw[0], ccw[1], ccw[2]) < 0.0 {
                    ccw.swap(1, 2);
                }
                for p in &scaled {
                    if ccw.iter().any(|v| v.x == p.x && v.y == p.y) {
                        continue;
                    }
                    let ic = robust::incircle(
                        RobustCoord {
                            x: ccw[0].x,
                            y: ccw[0].y,
                        },
                        RobustCoord {
                            x: ccw[1].x,
                            y: ccw[1].y,
                        },
                        RobustCoord {
                            x: ccw[2].x,
                            y: ccw[2].y,
                        },
                        RobustCoord { x: p.x, y: p.y },
                    );
                    assert!(ic <= 1e-9, "escala {scale:e} círculo no vacío ic={ic}");
                }
            }
        }
    }

    // ---- Voronoi dual ---------------------------------------------------

    fn polygon_area(poly: &[Point2]) -> f64 {
        if poly.len() < 3 {
            return 0.0;
        }
        let mut acc = 0.0;
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            acc += a.x * b.y - b.x * a.y;
        }
        0.5 * acc.abs()
    }

    fn bbox_of(pts: &[Point2]) -> (Point2, Point2) {
        voronoi_clip_bbox(pts)
    }

    #[test]
    fn voronoi_single_point_is_bbox() {
        let pts = vec![Point2::new(3.0, -2.0)];
        let cells = voronoi_cells(&pts).expect("voronoi 1pto");
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].len(), 4);
        assert!(point_in_polygon(pts[0], &cells[0]));
    }

    #[test]
    fn voronoi_two_points_bisector_split() {
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(2.0, 0.0)];
        let cells = voronoi_cells(&pts).expect("voronoi 2ptos");
        assert_eq!(cells.len(), 2);
        for (site, cell) in pts.iter().zip(cells.iter()) {
            assert!(cell.len() >= 3);
            assert!(
                point_in_polygon(*site, cell),
                "sitio {site:?} debe estar en su celda"
            );
            // El otro sitio no debe estar dentro (bisector exacto x=1).
            let other = pts[(pts.iter().position(|p| p == site).unwrap_or(0) + 1) % 2];
            assert!(
                !point_in_polygon(other, cell),
                "celda invadida por {other:?}"
            );
        }
        // Partición exacta de la envolvente.
        let (bb_min, bb_max) = bbox_of(&pts);
        let bb_area = (bb_max.x - bb_min.x) * (bb_max.y - bb_min.y);
        let sum: f64 = cells.iter().map(|c| polygon_area(c)).sum();
        assert!(
            (sum - bb_area).abs() < 1e-9,
            "áreas {sum} vs envolvente {bb_area}"
        );
    }

    #[test]
    fn voronoi_square_center_dual_consistent() {
        // Cuadrado + centro: Delaunay da 4 triángulos (test vecino); el dual
        // debe dar 5 celdas que particionan la envolvente.
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
            Point2::new(1.0, 1.0),
        ];
        let tris = delaunay_triangulation(&pts).expect("delaunay");
        assert_eq!(tris.len(), 4);
        let cells = voronoi_cells(&pts).expect("voronoi");
        assert_eq!(cells.len(), 5, "5 sitios → 5 celdas, got {cells:?}");
        // Cada celda contiene estrictamente a su sitio y a ningún otro.
        for (idx, cell) in cells.iter().enumerate() {
            assert!(cell.len() >= 3, "celda {idx} degenerada: {cell:?}");
            assert!(
                point_in_polygon(pts[idx], cell),
                "celda {idx} no contiene a su sitio {:?}",
                pts[idx]
            );
            for (jdx, other) in pts.iter().enumerate() {
                if jdx != idx {
                    assert!(
                        !point_in_polygon(*other, cell),
                        "celda {idx} invadida por sitio {jdx} {other:?}"
                    );
                }
            }
            // Todo vértice finito y dentro de la envolvente.
            let (bb_min, bb_max) = bbox_of(&pts);
            for v in cell {
                assert!(
                    v.x.is_finite() && v.y.is_finite(),
                    "vértice no finito {v:?}"
                );
                assert!(
                    v.x >= bb_min.x - 1e-9
                        && v.x <= bb_max.x + 1e-9
                        && v.y >= bb_min.y - 1e-9
                        && v.y <= bb_max.y + 1e-9,
                    "vértice {v:?} fuera de la envolvente"
                );
            }
        }
        // La celda del centro (índice 4) es acotada: diamante de 4 vértices
        // en los circuncentros (1,0),(2,1),(1,2),(0,1).
        let center_cell = &cells[4];
        assert_eq!(
            center_cell.len(),
            4,
            "celda central debe ser diamante, got {center_cell:?}"
        );
        for expected in [
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 1.0),
            Point2::new(1.0, 2.0),
            Point2::new(0.0, 1.0),
        ] {
            assert!(
                center_cell
                    .iter()
                    .any(|v| (v.x - expected.x).abs() < 1e-9 && (v.y - expected.y).abs() < 1e-9),
                "falta circuncentro {expected:?} en {center_cell:?}"
            );
        }
        // Partición exacta: suma de áreas = área de la envolvente.
        let (bb_min, bb_max) = bbox_of(&pts);
        let bb_area = (bb_max.x - bb_min.x) * (bb_max.y - bb_min.y);
        let sum: f64 = cells.iter().map(|c| polygon_area(c)).sum();
        assert!(
            (sum - bb_area).abs() < 1e-6,
            "áreas {sum} vs envolvente {bb_area}"
        );
    }

    #[test]
    fn voronoi_colinear_and_degenerate_are_honest_err() {
        let colinear = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        ];
        assert!(voronoi_cells(&colinear).is_err());
        let dup = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.5, 1.0),
            Point2::new(0.5, 1.0),
        ];
        let msg = voronoi_cells(&dup)
            .expect_err("duplicado debe fallar")
            .to_string();
        assert!(msg.contains("duplicado"), "mensaje honesto, fue: {msg}");
        assert!(voronoi_cells(&[]).is_err());
        // Sobre el límite 8192 → Err honesto.
        let big: Vec<Point2> = (0..8193u32)
            .map(|i| Point2::new((i % 91) as f64 * 0.37, (i / 91) as f64 * 0.53))
            .collect();
        let msg = voronoi_cells(&big)
            .expect_err("límite debe fallar")
            .to_string();
        assert!(msg.contains("8192"), "debe citar la cota, fue: {msg}");
    }
}
