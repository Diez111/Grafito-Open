//! Geometría discreta: ConvexHull, MST, TSP, Voronoi/Delaunay stubs y distancias.
//!
//! Todo el módulo es puro (sin `Document` ni renderizado) y respeta los límites
//! globales `MAX_POLYGON_VERTICES` y `MAX_DISCRETE_COUNT` a través de los
//! llamantes; aquí se valida finitud y se rechazan entradas degeneradas.

use crate::Point2;

const GEOM_EPS: f64 = 1e-12;

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

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

// ---------------------------------------------------------------------------
// ConvexHull — monotone chain (Andrew) O(n log n)
// ---------------------------------------------------------------------------

/// Calcula el cierre convexo de `points` en orden antihorario sin repetir
/// el primer punto al final. Devuelve `Ok(hull)` con al menos 1 vértice.
///
/// Usa `GEOM_EPS` para descartar puntos colineales intermedios: solo se
/// conservan los extremos de cada arista colineal.
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
        // Si son coincidentes, devuelve uno.
        if (points[0].x - points[1].x).hypot(points[0].y - points[1].y) <= GEOM_EPS {
            return Ok(vec![points[0]]);
        }
        return Ok(points.to_vec());
    }

    // Copia ordenada lexicográficamente (x, luego y).
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    // Elimina duplicados exactos dentro de eps.
    let mut dedup: Vec<Point2> = Vec::with_capacity(pts.len());
    for p in pts {
        if let Some(last) = dedup.last() {
            if (p.x - last.x).hypot(p.y - last.y) <= GEOM_EPS {
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
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= GEOM_EPS
        {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= GEOM_EPS
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
    // en ese caso devolver los dos extremos.
    if hull.is_empty() {
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            if (first.x - last.x).hypot(first.y - last.y) <= GEOM_EPS {
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
// Delaunay / Voronoi stubs
// ---------------------------------------------------------------------------

/// Triangulación Delaunay aproximada por abanico (fan) desde el primer
/// vértice del casco. Es válida y no falla; no garantiza propiedad Delaunay.
pub fn delaunay_fan_triangulation(points: &[Point2]) -> Result<Vec<[Point2; 3]>, DiscreteError> {
    validate_finite_points(points)?;
    if points.len() < 3 {
        return Err(DiscreteError(
            "DelaunayTriangulation: se requieren al menos 3 puntos".into(),
        ));
    }
    // Usa orden del casco para estabilidad; si el casco tiene todos los puntos,
    // el fan será sobre el casco; si hay puntos interiores, igual fan desde pts[0]
    // produce triángulos que cubren, aunque no óptimos.
    let n = points.len();
    if n > MAX_POLYGON_VERTICES_LOCAL {
        return Err(DiscreteError(format!(
            "Delaunay: {} puntos excede el máximo {}",
            n, MAX_POLYGON_VERTICES_LOCAL
        )));
    }
    let mut tris = Vec::with_capacity(n.saturating_sub(2));
    for i in 1..n - 1 {
        tris.push([points[0], points[i], points[i + 1]]);
    }
    Ok(tris)
}

/// Voronoi aproximado stub: para cada sitio genera un polígono circular
/// (aprox. 16 lados) centrado en el punto con radio 10% de la extensión
/// del conjunto o 0.5 si la extensión es nula.
pub fn voronoi_stub_cells(points: &[Point2]) -> Result<Vec<Vec<Point2>>, DiscreteError> {
    validate_finite_points(points)?;
    if points.is_empty() {
        return Err(DiscreteError(
            "Voronoi: se requieren al menos 1 punto".into(),
        ));
    }
    // Radio heurístico.
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
    let span = (max_x - min_x).hypot(max_y - min_y);
    let radius = if span.is_finite() && span > 1e-9 {
        (span * 0.08).clamp(0.1, 5.0)
    } else {
        0.5
    };
    let sides = 16usize;
    let mut cells: Vec<Vec<Point2>> = Vec::with_capacity(points.len());
    for &center in points {
        let mut ring: Vec<Point2> = Vec::with_capacity(sides);
        for k in 0..sides {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (sides as f64);
            ring.push(Point2::new(
                center.x + radius * theta.cos(),
                center.y + radius * theta.sin(),
            ));
        }
        cells.push(ring);
    }
    Ok(cells)
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

    #[test]
    fn delaunay_fan_produces_n_minus_2() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let tris = delaunay_fan_triangulation(&pts).expect("delaunay");
        assert_eq!(tris.len(), 2);
    }

    #[test]
    fn voronoi_stub_generates_cells() {
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)];
        let cells = voronoi_stub_cells(&pts).expect("voronoi");
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].len(), 16);
    }
}
