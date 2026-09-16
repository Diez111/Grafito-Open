//! Motor faltante de geometría / medida / cónicas / 3D.
//!
//! Funciones puras (sin `GeoObject`: este crate no depende de
//! `grafito-core`) pensadas para que la fase de cableado las invoque desde
//! los comandos. Todo acotado, determinista y con error honesto en `String`.
//!
//! Convenciones:
//! - Los ángulos se expresan en radianes y se normalizan a (-π, π].
//! - La cúbica implícita usa el orden canónico de monomios
//!   `[x³, x²y, xy², y³, x², xy, y², x, y, 1]`.
//! - La cónica general usa `ax² + bxy + cy² + dx + ey + f = 0` con el orden
//!   `[a, b, c, d, e, f]` (igual que [`crate::intersections::Conic`]).
//! - La tolerancia de colinealidad es `max(COLLINEAR_ABS_TOL,
//!   geom_eps(escala) * escala)`: nunca baja del piso absoluto ni se vuelve
//!   sorda ante coordenadas grandes.

use crate::lines::{distance_point_to_line, geom_eps};
use crate::measure::circumcenter;
use crate::{Plane3D, Point2, Point3D};

/// Piso absoluto de tolerancia para colinealidad y coincidencia.
pub const COLLINEAR_ABS_TOL: f64 = 1e-9;
/// Tolerancia relativa para congruencia de longitudes (SSS).
pub const CONGRUENCE_REL_TOL: f64 = 1e-9;
/// Vértices máximos aceptados por el helper de rigidez.
pub const MAX_RIGID_VERTICES: usize = 1024;
/// Puntos exactos que definen una cúbica implícita (9 grados de libertad).
pub const MAX_CUBIC_POINTS: usize = 9;
/// Monomios de la cúbica implícita `[x³, x²y, xy², y³, x², xy, y², x, y, 1]`.
pub const CUBIC_MONOMIALS: usize = 10;
/// Puntos exactos que definen una cónica general (5 grados de libertad).
pub const MAX_CONIC_POINTS: usize = 5;
/// Vértices máximos de la base de un prisma (espeja la cota de medida).
pub const MAX_PRISM_BASE_VERTICES: usize = 8192;
/// Residuo máximo aceptado al ajustar la cúbica (espeja el 1e-6 del ajuste
/// de cónica por 5 puntos del documento).
pub const CUBIC_RESIDUAL_TOL: f64 = 1e-6;
/// Residuo máximo aceptado al ajustar la cónica por 5 puntos.
pub const CONIC_RESIDUAL_TOL: f64 = 1e-6;
/// Pivote mínimo relativo: bajo esto el sistema se declara singular.
pub const SINGULAR_PIVOT_TOL: f64 = 1e-12;
/// Dimensión máxima del eliminador propio (`9×9` cúbica, `5×5` cónica).
pub const GAUSS_MAX_DIM: usize = 9;

// ── utilidades privadas ──────────────────────────────────────────────

/// Escala característica de un conjunto de puntos (magnitud máxima).
fn point_scale(pts: &[Point2]) -> f64 {
    let mut scale = 1.0f64;
    for p in pts {
        scale = scale.max(p.x.abs()).max(p.y.abs());
    }
    if scale.is_finite() {
        scale
    } else {
        1.0
    }
}

/// Tolerancia absoluta de colinealidad para la escala dada.
fn collinear_tol(scale: f64) -> f64 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    COLLINEAR_ABS_TOL.max(geom_eps(scale) * scale)
}

fn check_finite_2d(pts: &[Point2], what: &str) -> Result<(), String> {
    if !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return Err(format!("{what}: puntos deben ser finitos"));
    }
    Ok(())
}

fn check_finite_3d(pts: &[Point3D], what: &str) -> Result<(), String> {
    if !pts.iter().all(|p| p.is_finite()) {
        return Err(format!("{what}: puntos deben ser finitos"));
    }
    Ok(())
}

/// ¿Está `angle` entre `from` y `to` en sentido antihorario?
fn is_angle_between_ccw(from: f64, to: f64, angle: f64) -> bool {
    const TAU: f64 = 2.0 * std::f64::consts::PI;
    let rel_to = (to - from).rem_euclid(TAU);
    let rel_angle = (angle - from).rem_euclid(TAU);
    rel_angle <= rel_to
}

/// Normal de Newell (sin normalizar) de un polígono 3D. `None` si degenerado.
fn newell_normal(base: &[Point3D]) -> Option<(f64, f64, f64)> {
    if base.len() < 3 {
        return None;
    }
    let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..base.len() {
        let c = base[i];
        let n = base[(i + 1) % base.len()];
        nx += (c.y - n.y) * (c.z + n.z);
        ny += (c.z - n.z) * (c.x + n.x);
        nz += (c.x - n.x) * (c.y + n.y);
    }
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if !len.is_finite() || len <= 1e-12 {
        return None;
    }
    Some((nx, ny, nz))
}

fn centroid_3d(pts: &[Point3D]) -> Point3D {
    let n = pts.len().max(1) as f64;
    let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        x += p.x;
        y += p.y;
        z += p.z;
    }
    Point3D::new(x / n, y / n, z / n)
}

/// Eliminación gaussiana propia y acotada con pivoteo parcial.
///
/// Resuelve `A·x = b` con `1 <= n <= GAUSS_MAX_DIM`. Devuelve la solución y
/// el cociente `pivote_max / pivote_min` como estimador de condicionamiento
/// (cercano a 1 = sano; grande = ajuste frágil). `None` si singular o
/// fuera de cota. No usa `nalgebra` a propósito: dimensión tiny, costo
/// `O(n³)` con `n <= 9` y comportamiento bit-determinista documentado.
fn gauss_solve(a_rows: Vec<Vec<f64>>, b: Vec<f64>) -> Option<(Vec<f64>, f64)> {
    let n = b.len();
    if n == 0 || n > GAUSS_MAX_DIM || a_rows.len() != n {
        return None;
    }
    if a_rows.iter().any(|row| row.len() != n) {
        return None;
    }
    if !a_rows
        .iter()
        .flatten()
        .chain(b.iter())
        .all(|v| v.is_finite())
    {
        return None;
    }
    let mut aug: Vec<Vec<f64>> = a_rows;
    for (row, rhs) in aug.iter_mut().zip(b.iter()) {
        row.push(*rhs);
    }
    let mut pivot_min = f64::INFINITY;
    let mut pivot_max = 0.0f64;
    for col in 0..n {
        let mut best = col;
        for row in (col + 1)..n {
            if aug[row][col].abs() > aug[best][col].abs() {
                best = row;
            }
        }
        aug.swap(col, best);
        let pivot = aug[col][col];
        if !pivot.is_finite() || pivot.abs() <= SINGULAR_PIVOT_TOL {
            return None;
        }
        pivot_min = pivot_min.min(pivot.abs());
        pivot_max = pivot_max.max(pivot.abs());
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug[row][col] / pivot;
            if !factor.is_finite() {
                return None;
            }
            let pivot_segment: Vec<f64> = aug[col][col..=n].to_vec();
            for (slot, pivot_val) in aug[row][col..=n].iter_mut().zip(pivot_segment.iter()) {
                let v = *slot - factor * pivot_val;
                if !v.is_finite() {
                    return None;
                }
                *slot = v;
            }
        }
    }
    let mut sol = vec![0.0f64; n];
    for i in 0..n {
        let diag = aug[i][i];
        if !diag.is_finite() || diag.abs() <= SINGULAR_PIVOT_TOL {
            return None;
        }
        let v = aug[i][n] / diag;
        if !v.is_finite() {
            return None;
        }
        sol[i] = v;
    }
    if !pivot_min.is_finite() || pivot_min <= 0.0 {
        return None;
    }
    Some((sol, pivot_max / pivot_min))
}

// ── 1. Razones afines y congruencia ──────────────────────────────────

/// Razón afín de tres puntos colineales: el `t` tal que `C = A + t·(B−A)`.
///
/// Valida colinealidad con la tolerancia de este módulo; si `A == B` o `C`
/// se aparta de la recta, error honesto.
pub fn affine_ratio(a: Point2, b: Point2, c: Point2) -> Result<f64, String> {
    check_finite_2d(&[a, b, c], "AffineRatio")?;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let denom = dx * dx + dy * dy;
    let scale = point_scale(&[a, b, c]).max(denom.sqrt());
    if !denom.is_finite() || denom <= collinear_tol(scale).powi(2) {
        return Err("AffineRatio: A y B coinciden (razón indefinida)".to_string());
    }
    let dist = distance_point_to_line(c, a, b);
    if !dist.is_finite() || dist > collinear_tol(scale) {
        return Err("AffineRatio: los puntos no son colineales".to_string());
    }
    let t = ((c.x - a.x) * dx + (c.y - a.y) * dy) / denom;
    if t.is_finite() {
        Ok(t)
    } else {
        Err("AffineRatio: razón no finita".to_string())
    }
}

/// Razón doble de cuatro puntos colineales `(CA/CB) / (DA/DB)` con distancias
/// con signo sobre la recta. Valida colinealidad y denominadores no nulos.
pub fn cross_ratio(a: Point2, b: Point2, c: Point2, d: Point2) -> Result<f64, String> {
    check_finite_2d(&[a, b, c, d], "CrossRatio")?;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    let scale = point_scale(&[a, b, c, d]).max(len_sq.sqrt());
    let tol = collinear_tol(scale);
    if !len_sq.is_finite() || len_sq <= tol.powi(2) {
        return Err("CrossRatio: A y B coinciden (recta indefinida)".to_string());
    }
    for (name, p) in [("C", c), ("D", d)] {
        let dist = distance_point_to_line(p, a, b);
        if !dist.is_finite() || dist > tol {
            return Err(format!("CrossRatio: el punto {name} no es colineal"));
        }
    }
    // Coordenadas con signo sobre la recta (origen A, unidad |B−A|).
    let len = len_sq.sqrt();
    let ux = dx / len;
    let uy = dy / len;
    let coord = |p: Point2| (p.x - a.x) * ux + (p.y - a.y) * uy;
    let (sa, sb, sc, sd) = (0.0, len, coord(c), coord(d));
    let (n1, d1) = (sc - sa, sc - sb);
    let (n2, d2) = (sd - sa, sd - sb);
    if d1.abs() <= tol || d2.abs() <= tol {
        return Err("CrossRatio: denominador nulo (puntos coincidentes)".to_string());
    }
    let ratio = (n1 / d1) / (n2 / d2);
    if ratio.is_finite() {
        Ok(ratio)
    } else {
        Err("CrossRatio: razón no finita".to_string())
    }
}

/// ¿Son congruentes dos segmentos? Compara longitudes con tolerancia relativa
/// `CONGRUENCE_REL_TOL`. El cableado mapea el resto de tipos a error honesto.
pub fn segments_congruent(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> Result<bool, String> {
    check_finite_2d(&[a0, a1, b0, b1], "AreCongruent")?;
    let la = (a1.x - a0.x).hypot(a1.y - a0.y);
    let lb = (b1.x - b0.x).hypot(b1.y - b0.y);
    if !la.is_finite() || !lb.is_finite() {
        return Err("AreCongruent: longitudes no finitas".to_string());
    }
    Ok((la - lb).abs() <= CONGRUENCE_REL_TOL * la.max(lb).max(1.0))
}

/// Lados del anillo cerrado, ordenados de menor a mayor (determinista).
pub fn congruence_side_lengths(vertices: &[Point2]) -> Result<Vec<f64>, String> {
    if vertices.len() < 3 {
        return Err("AreCongruent: se necesitan al menos 3 vértices".to_string());
    }
    if vertices.len() > crate::measure::MAX_MEASURE_VERTICES {
        return Err(format!(
            "AreCongruent: {} vértices exceden la cota {}",
            vertices.len(),
            crate::measure::MAX_MEASURE_VERTICES
        ));
    }
    check_finite_2d(vertices, "AreCongruent")?;
    let mut sides: Vec<f64> = (0..vertices.len())
        .map(|i| {
            let a = vertices[i];
            let b = vertices[(i + 1) % vertices.len()];
            (b.x - a.x).hypot(b.y - a.y)
        })
        .collect();
    if !sides.iter().all(|s| s.is_finite()) {
        return Err("AreCongruent: lados no finitos".to_string());
    }
    sides.sort_by(|x, y| x.total_cmp(y));
    Ok(sides)
}

/// ¿Son congruentes dos polígonos? Criterio SSS sobre el multiconjunto de
/// lados ordenados. Distinto número de vértices → `Ok(false)` (no error).
/// El cableado devuelve error honesto para pares de otro tipo.
pub fn polygons_congruent_sss(a: &[Point2], b: &[Point2]) -> Result<bool, String> {
    if a.len() != b.len() {
        if a.len() < 3 || b.len() < 3 {
            return Err("AreCongruent: se necesitan al menos 3 vértices".to_string());
        }
        return Ok(false);
    }
    let sa = congruence_side_lengths(a)?;
    let sb = congruence_side_lengths(b)?;
    let tol = CONGRUENCE_REL_TOL;
    Ok(sa
        .iter()
        .zip(sb.iter())
        .all(|(x, y)| (x - y).abs() <= tol * x.max(*y).max(1.0)))
}

// ── 2. Arcos y sectores circulares ───────────────────────────────────

/// Normaliza un ángulo a (-π, π].
pub fn normalize_angle(angle: f64) -> f64 {
    const TAU: f64 = 2.0 * std::f64::consts::PI;
    let mut a = angle.rem_euclid(TAU);
    if a > std::f64::consts::PI {
        a -= TAU;
    }
    if a <= -std::f64::consts::PI {
        a += TAU;
    }
    a
}

/// Datos de un arco circular (listos para construir el `Arc` del documento).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircularArcData {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

/// Datos de un sector circular (listos para construir el `Sector`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircularSectorData {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

fn check_arc_inputs(
    center: Point2,
    radius: f64,
    a0: f64,
    a1: f64,
    what: &str,
) -> Result<(), String> {
    if !center.x.is_finite() || !center.y.is_finite() {
        return Err(format!("{what}: centro debe ser finito"));
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err(format!("{what}: radio debe ser finito y positivo"));
    }
    if !a0.is_finite() || !a1.is_finite() {
        return Err(format!("{what}: ángulos deben ser finitos"));
    }
    Ok(())
}

/// Arco circular por centro, radio y ángulos (se normalizan a (-π, π]).
pub fn circular_arc(
    center: Point2,
    radius: f64,
    a0: f64,
    a1: f64,
) -> Result<CircularArcData, String> {
    check_arc_inputs(center, radius, a0, a1, "CircularArc")?;
    Ok(CircularArcData {
        center,
        radius,
        start_angle: normalize_angle(a0),
        end_angle: normalize_angle(a1),
    })
}

/// Sector circular por centro, radio y ángulos (se normalizan a (-π, π]).
pub fn circular_sector(
    center: Point2,
    radius: f64,
    a0: f64,
    a1: f64,
) -> Result<CircularSectorData, String> {
    check_arc_inputs(center, radius, a0, a1, "CircularSector")?;
    Ok(CircularSectorData {
        center,
        radius,
        start_angle: normalize_angle(a0),
        end_angle: normalize_angle(a1),
    })
}

/// Ángulos ordenados del arco que va de `p1` a `p3` pasando por `p2`.
fn ordered_arc_angles(center: Point2, p1: Point2, p2: Point2, p3: Point2) -> (f64, f64) {
    let a1 = (p1.y - center.y).atan2(p1.x - center.x);
    let a2 = (p2.y - center.y).atan2(p2.x - center.x);
    let a3 = (p3.y - center.y).atan2(p3.x - center.x);
    let ccw = is_angle_between_ccw(a1, a3, a2);
    let (mut start, mut end) = if ccw { (a1, a3) } else { (a3, a1) };
    let contains = if ccw {
        is_angle_between_ccw(start, end, a2)
    } else {
        is_angle_between_ccw(end, start, a2)
    };
    if !contains {
        std::mem::swap(&mut start, &mut end);
    }
    (start, end)
}

/// Arco circunscrito a tres puntos no colineales (reusa `circumcenter` de
/// `measure`). El arco va del primero al tercero pasando por el segundo.
pub fn circumcircular_arc(a: Point2, b: Point2, c: Point2) -> Result<CircularArcData, String> {
    check_finite_2d(&[a, b, c], "CircumcircularArc")?;
    let center = circumcenter(a, b, c)
        .ok_or_else(|| "CircumcircularArc: puntos colineales (sin circuncentro)".to_string())?;
    if !center.x.is_finite() || !center.y.is_finite() {
        return Err("CircumcircularArc: circuncentro no finito".to_string());
    }
    let radius = center.distance(&a);
    if !radius.is_finite() || radius <= COLLINEAR_ABS_TOL {
        return Err("CircumcircularArc: radio degenerado".to_string());
    }
    let (start_angle, end_angle) = ordered_arc_angles(center, a, b, c);
    Ok(CircularArcData {
        center,
        radius,
        start_angle,
        end_angle,
    })
}

/// Sector circunscrito a tres puntos no colineales (mismo arco que
/// [`circumcircular_arc`], con centro incluido).
pub fn circumcircular_sector(
    a: Point2,
    b: Point2,
    c: Point2,
) -> Result<CircularSectorData, String> {
    let arc = circumcircular_arc(a, b, c)
        .map_err(|e| e.replace("CircumcircularArc", "CircumcircularSector"))?;
    Ok(CircularSectorData {
        center: arc.center,
        radius: arc.radius,
        start_angle: arc.start_angle,
        end_angle: arc.end_angle,
    })
}

// ── 3. Cúbica por 9 puntos ───────────────────────────────────────────

/// Ajuste de cúbica implícita con diagnóstico de condicionamiento.
///
/// `pivot_ratio` estima el condicionamiento (`max_pivote / min_pivote` del
/// eliminador sobre filas normalizadas): cercano a 1 es sano; mayor a 1e9
/// indica puntos casi degenerados (el ajuste sigue siendo el mejor intento
/// con residuo bajo, pero pequeños movimientos de los puntos lo cambian
/// mucho). Los puntos casi coincidentes o configuraciones singulares
/// (p. ej. 5+ puntos colineales) dan error honesto en vez de coeficientes
/// basura.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicFit {
    /// Coeficientes en orden `[x³, x²y, xy², y³, x², xy, y², x, y, 1]`,
    /// normalizados (coeficiente mayor = ±1).
    pub coeffs: [f64; CUBIC_MONOMIALS],
    /// Cociente de pivotes del sistema que fijó el ajuste.
    pub pivot_ratio: f64,
}

fn cubic_row(p: Point2) -> [f64; CUBIC_MONOMIALS] {
    let (x, y) = (p.x, p.y);
    [
        x * x * x,
        x * x * y,
        x * y * y,
        y * y * y,
        x * x,
        x * y,
        y * y,
        x,
        y,
        1.0,
    ]
}

/// Residuo absoluto de la cúbica en un punto (sin normalizar).
pub fn cubic_residual(coeffs: &[f64; CUBIC_MONOMIALS], p: Point2) -> f64 {
    let row = cubic_row(p);
    row.iter()
        .zip(coeffs.iter())
        .map(|(a, b)| a * b)
        .sum::<f64>()
        .abs()
}

/// Ajuste detallado de la cúbica implícita por 9 puntos.
///
/// Fija cada coeficiente a 1 por turno (mismo patrón que el ajuste de cónica
/// por 5 puntos) y resuelve el `9×9` restante con el eliminador propio.
/// Acepta el primer ajuste con residuo relativo `< CUBIC_RESIDUAL_TOL`.
pub fn cubic_through_nine_detailed(points: &[Point2]) -> Result<CubicFit, String> {
    if points.len() != MAX_CUBIC_POINTS {
        return Err(format!(
            "Cubic: se requieren exactamente {MAX_CUBIC_POINTS} puntos (llegaron {})",
            points.len()
        ));
    }
    check_finite_2d(points, "Cubic")?;
    // Filas normalizadas por su coeficiente mayor: estabiliza el pivoteo
    // ante coordenadas grandes sin cambiar el núcleo.
    let mut rows: Vec<[f64; CUBIC_MONOMIALS]> = points.iter().map(|p| cubic_row(*p)).collect();
    let mut row_norms = vec![0.0f64; rows.len()];
    for (row, norm) in rows.iter().zip(row_norms.iter_mut()) {
        let m = row.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if !m.is_finite() || m <= 0.0 {
            return Err("Cubic: puntos no finitos o nulos".to_string());
        }
        *norm = m;
    }
    for (row, norm) in rows.iter_mut().zip(row_norms.iter()) {
        for v in row.iter_mut() {
            *v /= norm;
        }
    }
    for fixed in 0..CUBIC_MONOMIALS {
        let mut a_rows: Vec<Vec<f64>> = Vec::with_capacity(MAX_CUBIC_POINTS);
        let mut b: Vec<f64> = Vec::with_capacity(MAX_CUBIC_POINTS);
        for row in &rows {
            let mut a_row: Vec<f64> = Vec::with_capacity(CUBIC_MONOMIALS - 1);
            for (i, v) in row.iter().enumerate() {
                if i != fixed {
                    a_row.push(*v);
                }
            }
            a_rows.push(a_row);
            b.push(-row[fixed]);
        }
        let Some((sol, pivot_ratio)) = gauss_solve(a_rows, b) else {
            continue;
        };
        let mut coeffs = [0.0f64; CUBIC_MONOMIALS];
        let mut idx = 0;
        for (i, c) in coeffs.iter_mut().enumerate() {
            if i == fixed {
                *c = 1.0;
            } else {
                *c = sol[idx];
                idx += 1;
            }
        }
        // Normaliza (mayor = ±1) y verifica residuo relativo por fila.
        let m = coeffs.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if !m.is_finite() || m <= 0.0 {
            continue;
        }
        for c in coeffs.iter_mut() {
            *c /= m;
        }
        let mut worst = 0.0f64;
        for (row, norm) in rows.iter().zip(row_norms.iter()) {
            let raw: f64 = row.iter().zip(coeffs.iter()).map(|(a, b)| a * b).sum();
            // Deshace la normalización de fila para medir en escala real.
            let scaled = (raw * norm).abs() / norm.max(1.0);
            worst = worst.max(scaled);
        }
        if worst.is_finite() && worst < CUBIC_RESIDUAL_TOL && pivot_ratio.is_finite() {
            return Ok(CubicFit {
                coeffs,
                pivot_ratio,
            });
        }
    }
    Err("Cubic: puntos singulares (no definen una cúbica única)".to_string())
}

/// Ajuste de la cúbica implícita por 9 puntos (solo coeficientes).
pub fn cubic_through_nine(points: &[Point2]) -> Result<[f64; CUBIC_MONOMIALS], String> {
    cubic_through_nine_detailed(points).map(|fit| fit.coeffs)
}

// ── 4. Dirección y perpendicular ─────────────────────────────────────

/// Vector director normalizado de la recta por dos puntos (tupla unitaria).
pub fn line_direction_2d(a: Point2, b: Point2) -> Result<(f64, f64), String> {
    check_finite_2d(&[a, b], "Direction")?;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Direction: la recta necesita dos puntos distintos".to_string());
    }
    Ok((dx / len, dy / len))
}

/// Vector director normalizado de la recta 3D por dos puntos.
pub fn line_direction_3d(a: Point3D, b: Point3D) -> Result<(f64, f64, f64), String> {
    check_finite_3d(&[a, b], "Direction")?;
    let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Direction: la recta 3D necesita dos puntos distintos".to_string());
    }
    Ok((dx / len, dy / len, dz / len))
}

/// Dirección asociada a un plano `ax + by + cz + d = 0`: su normal unitaria.
pub fn plane_normal_direction(a: f64, b: f64, c: f64, d: f64) -> Result<(f64, f64, f64), String> {
    if ![a, b, c, d].iter().all(|v| v.is_finite()) {
        return Err("Direction: coeficientes del plano deben ser finitos".to_string());
    }
    let len = (a * a + b * b + c * c).sqrt();
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Direction: plano degenerado (normal nula)".to_string());
    }
    Ok((a / len, b / len, c / len))
}

/// Recta perpendicular a la dada que pasa por `p`: devuelve el punto de paso
/// y su vector director unitario (el cableado construye la recta con eso).
pub fn perpendicular_line_through_point(
    p: Point2,
    a: Point2,
    b: Point2,
) -> Result<(Point2, (f64, f64)), String> {
    check_finite_2d(&[p, a, b], "PerpendicularLine")?;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("PerpendicularLine: la recta base necesita dos puntos distintos".to_string());
    }
    Ok((p, (-dy / len, dx / len)))
}

// ── 5. Polígono rígido ───────────────────────────────────────────────
//
// Estado del wiring (verificado en el código, no en docs):
// - SÍ existe y es usable programáticamente el mantenimiento de distancias:
//   `Document::try_add_distance_constraint(a, b, distance)` (core
//   `document.rs`, `try_add_numeric_constraint("Distance", …)`) con ecuación
//   `DistanceEq` entre DOS PUNTOS y validación fail-closed (dos `Point`,
//   distancia finita no negativa).
// - NO es limpio atar un polígono existente: `PolygonObj.vertices` es un
//   `Vec<Point2>` inline (core `object.rs`), no objetos `Point` del
//   documento, así que no hay IDs a los que colgar las `Distance`.
// El cableado debe crear N puntos libres + el polígono + las ligas (o usar
// este helper + validación y crear el comando como constructor normal).
// Los pares de abajo son la triangulación en abanico: n aristas + (n−3)
// diagonales = 2n−3 barras (conteo de Laman necesario, no suficiente).

/// Pares de vértices (índices) a ligar con restricciones de distancia:
/// aristas del anillo + diagonales en abanico desde el vértice 0.
pub fn rigid_bars_for_polygon(n: usize) -> Result<Vec<(usize, usize)>, String> {
    if n < 3 {
        return Err("RigidPolygon: se necesitan al menos 3 vértices".to_string());
    }
    if n > MAX_RIGID_VERTICES {
        return Err(format!(
            "RigidPolygon: {n} vértices exceden la cota {MAX_RIGID_VERTICES}"
        ));
    }
    let mut bars: Vec<(usize, usize)> = Vec::with_capacity(2 * n - 3);
    for i in 0..n {
        bars.push((i, (i + 1) % n));
    }
    for i in 2..n - 1 {
        bars.push((0, i));
    }
    Ok(bars)
}

/// ¿El conteo de barras es el rígido `2n−3`? (Laman: necesario, no suficiente.)
pub fn is_rigid_bar_count(n_vertices: usize, n_bars: usize) -> bool {
    if n_vertices < 3 {
        return false;
    }
    n_vertices
        .checked_mul(2)
        .and_then(|v| v.checked_sub(3))
        .is_some_and(|expected| expected == n_bars)
}

/// Valida una lista de barras: índices en rango, sin duplicados ni lazos, y
/// conteo `2n−3`. Devuelve `Ok(true)` si es rígida en conteo.
pub fn validate_rigidity_bars(n_vertices: usize, bars: &[(usize, usize)]) -> Result<bool, String> {
    if !(3..=MAX_RIGID_VERTICES).contains(&n_vertices) {
        return Err(format!(
            "RigidPolygon: vértices fuera de rango 3..={MAX_RIGID_VERTICES}"
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, j) in bars {
        if *i >= n_vertices || *j >= n_vertices {
            return Err(format!(
                "RigidPolygon: barra ({i},{j}) fuera de rango para {n_vertices} vértices"
            ));
        }
        if i == j {
            return Err(format!("RigidPolygon: lazo en el vértice {i}"));
        }
        let key = ((*i).min(*j), (*i).max(*j));
        if !seen.insert(key) {
            return Err(format!("RigidPolygon: barra duplicada ({i},{j})"));
        }
    }
    Ok(is_rigid_bar_count(n_vertices, seen.len()))
}

// ── 6. Cónicas: alias, parámetro, recorrido, tipo, vértices, ángulos ──

/// Ajuste puro de cónica general por 5 puntos.
///
/// Devuelve `[a, b, c, d, e, f]` normalizados (mayor = ±1). Si ya existe un
/// ajuste equivalente en el documento (`ConicByFivePoints`), el cableado
/// puede reusarlo; este motor no toca el documento.
pub fn conic_through_five(points: &[Point2]) -> Result<[f64; 6], String> {
    if points.len() != MAX_CONIC_POINTS {
        return Err(format!(
            "Conic: se requieren exactamente {MAX_CONIC_POINTS} puntos (llegaron {})",
            points.len()
        ));
    }
    check_finite_2d(points, "Conic")?;
    let mut rows: Vec<[f64; 6]> = points
        .iter()
        .map(|p| [p.x * p.x, p.x * p.y, p.y * p.y, p.x, p.y, 1.0])
        .collect();
    let mut norms = vec![0.0f64; rows.len()];
    for (row, norm) in rows.iter().zip(norms.iter_mut()) {
        let m = row.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if !m.is_finite() || m <= 0.0 {
            return Err("Conic: puntos no finitos o nulos".to_string());
        }
        *norm = m;
    }
    for (row, norm) in rows.iter_mut().zip(norms.iter()) {
        for v in row.iter_mut() {
            *v /= norm;
        }
    }
    for fixed in 0..6 {
        let mut a_rows: Vec<Vec<f64>> = Vec::with_capacity(MAX_CONIC_POINTS);
        let mut b: Vec<f64> = Vec::with_capacity(MAX_CONIC_POINTS);
        for row in &rows {
            let mut a_row: Vec<f64> = Vec::with_capacity(5);
            for (i, v) in row.iter().enumerate() {
                if i != fixed {
                    a_row.push(*v);
                }
            }
            a_rows.push(a_row);
            b.push(-row[fixed]);
        }
        let Some((sol, _)) = gauss_solve(a_rows, b) else {
            continue;
        };
        let mut coeffs = [0.0f64; 6];
        let mut idx = 0;
        for (i, c) in coeffs.iter_mut().enumerate() {
            if i == fixed {
                *c = 1.0;
            } else {
                *c = sol[idx];
                idx += 1;
            }
        }
        let m = coeffs.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if !m.is_finite() || m <= 0.0 {
            continue;
        }
        for c in coeffs.iter_mut() {
            *c /= m;
        }
        let mut worst = 0.0f64;
        for row in &rows {
            let raw: f64 = row.iter().zip(coeffs.iter()).map(|(a, b)| a * b).sum();
            worst = worst.max(raw.abs());
        }
        if worst.is_finite() && worst < CONIC_RESIDUAL_TOL {
            return Ok(coeffs);
        }
    }
    Err("Conic: puntos singulares (no definen una cónica única)".to_string())
}

/// Clase de una cónica general (discriminante + invariante cúbico).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConicKind {
    Ellipse,
    Hyperbola,
    Parabola,
    Degenerate,
}

/// Clasifica `[a, b, c, d, e, f]`: primero el invariante cúbico (degenerada),
/// luego el discriminante `b² − 4ac` con tolerancia relativa a la escala.
pub fn classify_conic(coeffs: &[f64; 6]) -> Result<ConicKind, String> {
    if !coeffs.iter().all(|v| v.is_finite()) {
        return Err("Type: coeficientes deben ser finitos".to_string());
    }
    let [a, b, c, d, e, f] = *coeffs;
    let scale = a
        .abs()
        .max(b.abs())
        .max(c.abs())
        .max(d.abs())
        .max(e.abs())
        .max(f.abs())
        .max(1.0);
    // Invariante cúbico det[[a,b/2,d/2],[b/2,c,e/2],[d/2,e/2,f]].
    let det = a * (c * f - e * e / 4.0) - b / 2.0 * (b / 2.0 * f - e * d / 4.0)
        + d / 2.0 * (b / 2.0 * e / 2.0 - c * d / 2.0);
    if !det.is_finite() {
        return Err("Type: invariante no finito".to_string());
    }
    if det.abs() <= SINGULAR_PIVOT_TOL * scale * scale * scale {
        return Ok(ConicKind::Degenerate);
    }
    let disc = b * b - 4.0 * a * c;
    let tol = SINGULAR_PIVOT_TOL * scale * scale;
    if disc < -tol {
        Ok(ConicKind::Ellipse)
    } else if disc > tol {
        Ok(ConicKind::Hyperbola)
    } else {
        Ok(ConicKind::Parabola)
    }
}

/// Parámetro focal de la parábola: el campo `p` del objeto (distancia
/// vértice–foco). Devuelve `|p|`; error si no es finito o es cero.
pub fn parabola_focal_parameter(p: f64) -> Result<f64, String> {
    if !p.is_finite() || p == 0.0 {
        return Err("Parameter: la parábola necesita p finito no nulo".to_string());
    }
    Ok(p.abs())
}

/// Parámetro focal de la elipse `p = b²/a` con `a` = semieje mayor.
pub fn ellipse_focal_parameter(rx: f64, ry: f64) -> Result<f64, String> {
    if ![rx, ry].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Parameter: semiejes de elipse deben ser finitos positivos".to_string());
    }
    let (a, b) = (rx.max(ry), rx.min(ry));
    let p = b * b / a;
    if p.is_finite() {
        Ok(p)
    } else {
        Err("Parameter: parámetro no finito".to_string())
    }
}

/// Parámetro focal de la hipérbola `p = b²/a`.
pub fn hyperbola_focal_parameter(a: f64, b: f64) -> Result<f64, String> {
    if ![a, b].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Parameter: semiejes de hipérbola deben ser finitos positivos".to_string());
    }
    let p = b * b / a;
    if p.is_finite() {
        Ok(p)
    } else {
        Err("Parameter: parámetro no finito".to_string())
    }
}

/// Parámetro de recorrido sobre una polilínea: `t` global (índice de
/// segmento + `t` local) del punto de la polilínea más cercano a `p`.
///
/// Los puntos del documento no guardan parámetro de recorrido (la polilínea
/// solo guarda vértices), así que el cableado debe usar este cómputo.
pub fn path_parameter_polyline(p: Point2, vertices: &[Point2]) -> Result<f64, String> {
    if vertices.len() < 2 {
        return Err("PathParameter: se necesitan al menos 2 vértices".to_string());
    }
    if vertices.len() > crate::measure::MAX_MEASURE_VERTICES {
        return Err(format!(
            "PathParameter: {} vértices exceden la cota {}",
            vertices.len(),
            crate::measure::MAX_MEASURE_VERTICES
        ));
    }
    check_finite_2d(vertices, "PathParameter")?;
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err("PathParameter: punto debe ser finito".to_string());
    }
    let mut best_t = 0.0f64;
    let mut best_d = f64::INFINITY;
    for i in 0..vertices.len() - 1 {
        let a = vertices[i];
        let b = vertices[i + 1];
        let abx = b.x - a.x;
        let aby = b.y - a.y;
        let denom = abx * abx + aby * aby;
        if !denom.is_finite() || denom <= 0.0 {
            continue;
        }
        let t = (((p.x - a.x) * abx + (p.y - a.y) * aby) / denom).clamp(0.0, 1.0);
        let qx = a.x + t * abx;
        let qy = a.y + t * aby;
        let d = (p.x - qx).hypot(p.y - qy);
        if d < best_d {
            best_d = d;
            best_t = i as f64 + t;
        }
    }
    if best_d.is_finite() {
        Ok(best_t)
    } else {
        Err("PathParameter: polilínea degenerada".to_string())
    }
}

// NOTA Type: ya cubierto — `GeoObject::name()` (core `object.rs`) devuelve el
// nombre estable del tipo ("Point", "Ellipse", …). El cableado debe usarlo;
// no se duplica acá para no bifurcar la tabla.

/// Vértices de un polígono como lista validada (cota + finitud).
pub fn polygon_vertex_list(vertices: &[Point2]) -> Result<Vec<Point2>, String> {
    if vertices.len() < 3 {
        return Err("Vertex: se necesitan al menos 3 vértices".to_string());
    }
    if vertices.len() > crate::measure::MAX_MEASURE_VERTICES {
        return Err(format!(
            "Vertex: {} vértices exceden la cota {}",
            vertices.len(),
            crate::measure::MAX_MEASURE_VERTICES
        ));
    }
    check_finite_2d(vertices, "Vertex")?;
    Ok(vertices.to_vec())
}

/// Los 4 vértices de la elipse `[mayor+, mayor−, menor+, menor−]`,
/// con rotación `angle` (radianes) del eje mayor.
pub fn ellipse_vertices(
    center: Point2,
    rx: f64,
    ry: f64,
    angle: f64,
) -> Result<[Point2; 4], String> {
    if !center.x.is_finite() || !center.y.is_finite() || !angle.is_finite() {
        return Err("Vertex: centro y ángulo deben ser finitos".to_string());
    }
    if ![rx, ry].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Vertex: semiejes de elipse deben ser finitos positivos".to_string());
    }
    let (major_len, minor_len, major_angle) = if rx >= ry {
        (rx, ry, angle)
    } else {
        (ry, rx, angle + std::f64::consts::FRAC_PI_2)
    };
    let (ux, uy) = (major_angle.cos(), major_angle.sin());
    let (vx, vy) = (-uy, ux);
    Ok([
        Point2::new(center.x + major_len * ux, center.y + major_len * uy),
        Point2::new(center.x - major_len * ux, center.y - major_len * uy),
        Point2::new(center.x + minor_len * vx, center.y + minor_len * vy),
        Point2::new(center.x - minor_len * vx, center.y - minor_len * vy),
    ])
}

/// Los 2 vértices de la hipérbola (centro ± `a` sobre el eje transverso
/// rotado `angle` radianes).
pub fn hyperbola_vertices(center: Point2, a: f64, angle: f64) -> Result<[Point2; 2], String> {
    if !center.x.is_finite() || !center.y.is_finite() || !angle.is_finite() {
        return Err("Vertex: centro y ángulo deben ser finitos".to_string());
    }
    if !a.is_finite() || a <= 0.0 {
        return Err("Vertex: semieje de hipérbola debe ser finito positivo".to_string());
    }
    let (ux, uy) = (angle.cos(), angle.sin());
    Ok([
        Point2::new(center.x + a * ux, center.y + a * uy),
        Point2::new(center.x - a * ux, center.y - a * uy),
    ])
}

/// Vértice de la parábola (identidad validada: el objeto ya lo guarda;
/// existe para cableado uniforme de `Vertex`).
pub fn parabola_vertex(vertex: Point2) -> Result<Point2, String> {
    if !vertex.x.is_finite() || !vertex.y.is_finite() {
        return Err("Vertex: vértice debe ser finito".to_string());
    }
    Ok(vertex)
}

/// Ángulos interiores de un polígono en radianes, con orientación
/// (signo del área: CCW positivo). Rango `(0, 2π)`.
pub fn interior_angles(vertices: &[Point2]) -> Result<Vec<f64>, String> {
    if vertices.len() < 3 {
        return Err("InteriorAngles: se necesitan al menos 3 vértices".to_string());
    }
    if vertices.len() > crate::measure::MAX_MEASURE_VERTICES {
        return Err(format!(
            "InteriorAngles: {} vértices exceden la cota {}",
            vertices.len(),
            crate::measure::MAX_MEASURE_VERTICES
        ));
    }
    check_finite_2d(vertices, "InteriorAngles")?;
    let signed = crate::measure::polygon_area_signed(vertices);
    if !signed.is_finite() || signed.abs() <= COLLINEAR_ABS_TOL {
        return Err("InteriorAngles: polígono degenerado (área nula)".to_string());
    }
    let orient = signed.signum();
    let n = vertices.len();
    let mut out: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let prev = vertices[(i + n - 1) % n];
        let cur = vertices[i];
        let next = vertices[(i + 1) % n];
        let (ux, uy) = (cur.x - prev.x, cur.y - prev.y);
        let (vx, vy) = (next.x - cur.x, next.y - cur.y);
        let lu = ux.hypot(uy);
        let lv = vx.hypot(vy);
        if !lu.is_finite() || !lv.is_finite() || lu <= COLLINEAR_ABS_TOL || lv <= COLLINEAR_ABS_TOL
        {
            return Err(format!(
                "InteriorAngles: arista degenerada en el vértice {i}"
            ));
        }
        let cross = ux * vy - uy * vx;
        let dot = ux * vx + uy * vy;
        let turn = cross.atan2(dot);
        let interior = std::f64::consts::PI - orient * turn;
        if !interior.is_finite() {
            return Err("InteriorAngles: ángulo no finito".to_string());
        }
        out.push(interior);
    }
    Ok(out)
}

// ── 7. Sólidos 3D: cotas, tapas, lateral e intersección cónica ────────
//
// Los sólidos paramétricos exponen sus cotas en sus campos; lo que no tiene
// forma cerrada (cuádrica general, superficies, curvas) queda como error
// honesto en el cableado.

/// Extensión en Z de un cubo eje-alineado de centro y arista dados.
pub fn cube_z_extent(center: Point3D, size: f64) -> Result<(f64, f64), String> {
    if !center.is_finite() {
        return Err("Bottom/Top: centro debe ser finito".to_string());
    }
    if !size.is_finite() || size <= 0.0 {
        return Err("Bottom/Top: arista debe ser finita positiva".to_string());
    }
    let h = 0.5 * size;
    Ok((center.z - h, center.z + h))
}

/// Extensión en Z de una esfera.
pub fn sphere_z_extent(center: Point3D, radius: f64) -> Result<(f64, f64), String> {
    if !center.is_finite() {
        return Err("Bottom/Top: centro debe ser finito".to_string());
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Bottom/Top: radio debe ser finito positivo".to_string());
    }
    Ok((center.z - radius, center.z + radius))
}

/// Extensión en Z de un cilindro de eje arbitrario (el radio aporta
/// `r·√(1−az²)` con `az` = componente Z del eje unitario).
pub fn cylinder_z_extent(base: Point3D, top: Point3D, radius: f64) -> Result<(f64, f64), String> {
    check_finite_3d(&[base, top], "Bottom/Top")?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Bottom/Top: radio debe ser finito positivo".to_string());
    }
    let (dx, dy, dz) = (top.x - base.x, top.y - base.y, top.z - base.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Bottom/Top: base y tapa coinciden".to_string());
    }
    let az = (dz / len).clamp(-1.0, 1.0);
    let ext = radius * (1.0 - az * az).max(0.0).sqrt();
    Ok((base.z.min(top.z) - ext, base.z.max(top.z) + ext))
}

/// Extensión en Z de un cono (el radio solo ensancha la base).
pub fn cone_z_extent(
    base_center: Point3D,
    apex: Point3D,
    radius: f64,
) -> Result<(f64, f64), String> {
    check_finite_3d(&[base_center, apex], "Bottom/Top")?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Bottom/Top: radio debe ser finito positivo".to_string());
    }
    let (dx, dy, dz) = (
        apex.x - base_center.x,
        apex.y - base_center.y,
        apex.z - base_center.z,
    );
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Bottom/Top: base y ápice coinciden".to_string());
    }
    let az = (dz / len).clamp(-1.0, 1.0);
    let ext = radius * (1.0 - az * az).max(0.0).sqrt();
    Ok(if base_center.z - ext <= apex.z {
        (base_center.z - ext, apex.z.max(base_center.z + ext))
    } else {
        (apex.z, base_center.z + ext)
    })
}

/// Extensión en Z de un prisma (mínimo/máximo entre base y tapa).
pub fn prism_z_extent(base: &[Point3D], direction: Point3D) -> Result<(f64, f64), String> {
    if base.len() < 3 {
        return Err("Bottom/Top: la base necesita al menos 3 vértices".to_string());
    }
    if base.len() > MAX_PRISM_BASE_VERTICES {
        return Err(format!(
            "Bottom/Top: {} vértices exceden la cota {MAX_PRISM_BASE_VERTICES}",
            base.len()
        ));
    }
    check_finite_3d(base, "Bottom/Top")?;
    if !direction.is_finite() {
        return Err("Bottom/Top: dirección debe ser finita".to_string());
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in base {
        for z in [p.z, p.z + direction.z] {
            if !z.is_finite() {
                return Err("Bottom/Top: cota no finita".to_string());
            }
            lo = lo.min(z);
            hi = hi.max(z);
        }
    }
    Ok((lo, hi))
}

/// Extensión en Z de una pirámide de base cuadrada horizontal (misma
/// convención que el render: base en el plano `y = base_center.y`, medio
/// lado `base_size / 2` en X y Z).
pub fn pyramid_z_extent(
    base_center: Point3D,
    apex: Point3D,
    base_size: f64,
) -> Result<(f64, f64), String> {
    check_finite_3d(&[base_center, apex], "Bottom/Top")?;
    if !base_size.is_finite() || base_size <= 0.0 {
        return Err("Bottom/Top: base debe ser finita positiva".to_string());
    }
    let h = 0.5 * base_size;
    Ok((
        (base_center.z - h).min(apex.z),
        (base_center.z + h).max(apex.z),
    ))
}

/// Extensión en Z de un toro en el plano XY (tubo ±`r_minor` en Z).
pub fn torus_z_extent(center: Point3D, r_major: f64, r_minor: f64) -> Result<(f64, f64), String> {
    if !center.is_finite() {
        return Err("Bottom/Top: centro debe ser finito".to_string());
    }
    if ![r_major, r_minor].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Bottom/Top: radios del toro deben ser finitos positivos".to_string());
    }
    Ok((center.z - r_minor, center.z + r_minor))
}

/// Plano horizontal `z = cota` (`0·x + 0·y + 1·z − cota = 0`).
/// El cableado lo llama con `z_min` (Bottom) o `z_max` (Top).
pub fn z_plane(z: f64) -> Result<Plane3D, String> {
    if !z.is_finite() {
        return Err("Bottom/Top: cota debe ser finita".to_string());
    }
    Ok(Plane3D::from_equation(0.0, 0.0, 1.0, -z))
}

/// Tapas de un cilindro: par de planos perpendiculares al eje por la base
/// y la tapa. Error si el eje es degenerado.
pub fn cylinder_end_planes(
    base: Point3D,
    top: Point3D,
    radius: f64,
) -> Result<(Plane3D, Plane3D), String> {
    check_finite_3d(&[base, top], "Ends")?;
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Ends: radio debe ser finito positivo".to_string());
    }
    let (dx, dy, dz) = (top.x - base.x, top.y - base.y, top.z - base.z);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if !len.is_finite() || len <= COLLINEAR_ABS_TOL {
        return Err("Ends: base y tapa coinciden".to_string());
    }
    let (nx, ny, nz) = (dx / len, dy / len, dz / len);
    Ok((
        Plane3D::from_equation(nx, ny, nz, -(nx * base.x + ny * base.y + nz * base.z)),
        Plane3D::from_equation(nx, ny, nz, -(nx * top.x + ny * top.y + nz * top.z)),
    ))
}

/// Tapas de un prisma: planos por el centroide de la base y el de la tapa,
/// con normal de Newell de la base. Error si la base es degenerada.
pub fn prism_end_planes(
    base: &[Point3D],
    direction: Point3D,
) -> Result<(Plane3D, Plane3D), String> {
    if base.len() < 3 {
        return Err("Ends: la base necesita al menos 3 vértices".to_string());
    }
    if base.len() > MAX_PRISM_BASE_VERTICES {
        return Err(format!(
            "Ends: {} vértices exceden la cota {MAX_PRISM_BASE_VERTICES}",
            base.len()
        ));
    }
    check_finite_3d(base, "Ends")?;
    if !direction.is_finite() {
        return Err("Ends: dirección debe ser finita".to_string());
    }
    let (nx, ny, nz) = newell_normal(base).ok_or_else(|| "Ends: base degenerada".to_string())?;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    let (nx, ny, nz) = (nx / len, ny / len, nz / len);
    let c0 = centroid_3d(base);
    let c1 = Point3D::new(c0.x + direction.x, c0.y + direction.y, c0.z + direction.z);
    if !c1.is_finite() {
        return Err("Ends: tapa no finita".to_string());
    }
    Ok((
        Plane3D::from_equation(nx, ny, nz, -(nx * c0.x + ny * c0.y + nz * c0.z)),
        Plane3D::from_equation(nx, ny, nz, -(nx * c1.x + ny * c1.y + nz * c1.z)),
    ))
}

/// Área lateral de un cilindro `2·π·r·h`.
pub fn cylinder_lateral_area(radius: f64, height: f64) -> Result<f64, String> {
    if ![radius, height].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Side: radio y altura deben ser finitos positivos".to_string());
    }
    let area = 2.0 * std::f64::consts::PI * radius * height;
    if area.is_finite() {
        Ok(area)
    } else {
        Err("Side: área no finita".to_string())
    }
}

/// Área lateral de un cono `π·r·g` con generatriz `g = √(r²+h²)`.
pub fn cone_lateral_area(radius: f64, height: f64) -> Result<f64, String> {
    if ![radius, height].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("Side: radio y altura deben ser finitos positivos".to_string());
    }
    let area = std::f64::consts::PI * radius * (radius * radius + height * height).sqrt();
    if area.is_finite() {
        Ok(area)
    } else {
        Err("Side: área no finita".to_string())
    }
}

/// Área lateral de un prisma: perímetro de la base × |dirección`.
pub fn prism_lateral_area(base: &[Point3D], direction: Point3D) -> Result<f64, String> {
    if base.len() < 3 {
        return Err("Side: la base necesita al menos 3 vértices".to_string());
    }
    if base.len() > MAX_PRISM_BASE_VERTICES {
        return Err(format!(
            "Side: {} vértices exceden la cota {MAX_PRISM_BASE_VERTICES}",
            base.len()
        ));
    }
    check_finite_3d(base, "Side")?;
    if !direction.is_finite() {
        return Err("Side: dirección debe ser finita".to_string());
    }
    let ext =
        (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
    if !ext.is_finite() || ext <= COLLINEAR_ABS_TOL {
        return Err("Side: dirección degenerada".to_string());
    }
    let mut perimeter = 0.0f64;
    for i in 0..base.len() {
        let edge = base[i].distance(&base[(i + 1) % base.len()]);
        if !edge.is_finite() {
            return Err("Side: arista no finita".to_string());
        }
        perimeter += edge;
    }
    let area = perimeter * ext;
    if area.is_finite() {
        Ok(area)
    } else {
        Err("Side: área no finita".to_string())
    }
}

/// Círculo intersección esfera×plano (centro + radio del círculo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpherePlaneCircle {
    pub center: Point3D,
    pub radius: f64,
}

/// Intersección esfera×plano → círculo. Sin intersección o tangencia en un
/// punto → error honesto (no hay círculo). El resto de pares de sólidos va
/// por [`intersect_conic_unsupported`].
pub fn sphere_plane_circle(
    center: Point3D,
    radius: f64,
    plane: Plane3D,
) -> Result<SpherePlaneCircle, String> {
    if !center.is_finite() {
        return Err("IntersectConic: centro debe ser finito".to_string());
    }
    if !radius.is_finite() || radius <= 0.0 {
        return Err("IntersectConic: radio debe ser finito positivo".to_string());
    }
    if ![plane.a, plane.b, plane.c, plane.d]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err("IntersectConic: plano debe ser finito".to_string());
    }
    let norm = (plane.a * plane.a + plane.b * plane.b + plane.c * plane.c).sqrt();
    if !norm.is_finite() || norm <= COLLINEAR_ABS_TOL {
        return Err("IntersectConic: plano degenerado".to_string());
    }
    let signed = (plane.a * center.x + plane.b * center.y + plane.c * center.z + plane.d) / norm;
    if !signed.is_finite() {
        return Err("IntersectConic: distancia no finita".to_string());
    }
    let tol = COLLINEAR_ABS_TOL.max(geom_eps(radius) * radius);
    if signed.abs() > radius + tol {
        return Err("IntersectConic: el plano no corta a la esfera".to_string());
    }
    let rho_sq = (radius * radius - signed * signed).max(0.0);
    let rho = rho_sq.sqrt();
    if !rho.is_finite() || rho <= tol {
        return Err("IntersectConic: tangencia en un punto (no define círculo)".to_string());
    }
    let (nx, ny, nz) = (plane.a / norm, plane.b / norm, plane.c / norm);
    let foot = Point3D::new(
        center.x - nx * signed,
        center.y - ny * signed,
        center.z - nz * signed,
    );
    if !foot.is_finite() {
        return Err("IntersectConic: centro no finito".to_string());
    }
    Ok(SpherePlaneCircle {
        center: foot,
        radius: rho,
    })
}

/// Mensaje honesto para pares sin cónica cerrada (todo salvo esfera×plano).
pub fn intersect_conic_unsupported(kind_a: &str, kind_b: &str) -> String {
    format!(
        "IntersectConic: el par {kind_a}×{kind_b} no produce una cónica cerrada con forma conocida; \
         solo esfera×plano da un círculo"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn p2(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn p3(x: f64, y: f64, z: f64) -> Point3D {
        Point3D::new(x, y, z)
    }

    #[test]
    fn affine_ratio_midpoint_is_half() {
        let t = affine_ratio(p2(0.0, 0.0), p2(4.0, 0.0), p2(2.0, 0.0)).unwrap();
        assert!((t - 0.5).abs() < EPS, "got {t}");
    }

    #[test]
    fn affine_ratio_rejects_non_collinear() {
        assert!(affine_ratio(p2(0.0, 0.0), p2(4.0, 0.0), p2(2.0, 1.0)).is_err());
    }

    #[test]
    fn affine_ratio_rejects_coincident_base() {
        assert!(affine_ratio(p2(1.0, 1.0), p2(1.0, 1.0), p2(2.0, 2.0)).is_err());
    }

    #[test]
    fn cross_ratio_of_unit_marks() {
        // (CA/CB)/(DA/DB) con s = 0,1,2,4: (2/1)/(4/3) = 1.5.
        let r = cross_ratio(p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.0), p2(4.0, 0.0)).unwrap();
        assert!((r - 1.5).abs() < EPS, "got {r}");
    }

    #[test]
    fn cross_ratio_rejects_off_line_point() {
        assert!(cross_ratio(p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.0), p2(4.0, 1.0)).is_err());
    }

    #[test]
    fn cross_ratio_rejects_null_denominator() {
        // D == B anula el denominador.
        assert!(cross_ratio(p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.0), p2(1.0, 0.0)).is_err());
    }

    #[test]
    fn segments_congruent_compares_lengths() {
        assert!(
            segments_congruent(p2(0.0, 0.0), p2(3.0, 4.0), p2(1.0, 1.0), p2(1.0, 6.0)).unwrap()
        );
        assert!(
            !segments_congruent(p2(0.0, 0.0), p2(1.0, 0.0), p2(0.0, 0.0), p2(2.0, 0.0)).unwrap()
        );
    }

    #[test]
    fn congruence_side_lengths_sorts_closed_ring() {
        let tri = [p2(0.0, 0.0), p2(4.0, 0.0), p2(0.0, 3.0)];
        assert_eq!(congruence_side_lengths(&tri).unwrap(), vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn polygons_congruent_sss_detects_permuted_triangle() {
        let a = [p2(0.0, 0.0), p2(4.0, 0.0), p2(0.0, 3.0)];
        let b = [p2(0.0, 3.0), p2(0.0, 0.0), p2(4.0, 0.0)];
        assert!(polygons_congruent_sss(&a, &b).unwrap());
        let c = [p2(0.0, 0.0), p2(5.0, 0.0), p2(0.0, 3.0)];
        assert!(!polygons_congruent_sss(&a, &c).unwrap());
    }

    #[test]
    fn polygons_congruent_different_vertex_count_is_false() {
        let a = [p2(0.0, 0.0), p2(4.0, 0.0), p2(0.0, 3.0)];
        let b = [p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 1.0), p2(0.0, 1.0)];
        assert!(!polygons_congruent_sss(&a, &b).unwrap());
    }

    #[test]
    fn normalize_angle_wraps_to_pi_range() {
        assert!((normalize_angle(3.0 * std::f64::consts::PI) - std::f64::consts::PI).abs() < EPS);
        assert!((normalize_angle(-3.0 * std::f64::consts::PI) - std::f64::consts::PI).abs() < EPS);
        assert!((normalize_angle(0.5) - 0.5).abs() < EPS);
    }

    #[test]
    fn circular_arc_builds_normalized_data() {
        let arc = circular_arc(p2(1.0, 2.0), 3.0, 0.0, std::f64::consts::PI).unwrap();
        assert_eq!(arc.center, p2(1.0, 2.0));
        assert!((arc.radius - 3.0).abs() < EPS);
        assert!((arc.end_angle - std::f64::consts::PI).abs() < EPS);
    }

    #[test]
    fn circular_arc_rejects_nonpositive_radius() {
        assert!(circular_arc(p2(0.0, 0.0), 0.0, 0.0, 1.0).is_err());
        assert!(circular_arc(p2(0.0, 0.0), -2.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn circular_sector_builds_data() {
        let s = circular_sector(p2(0.0, 0.0), 2.0, 0.0, std::f64::consts::FRAC_PI_2).unwrap();
        assert!((s.radius - 2.0).abs() < EPS);
        assert!((s.end_angle - std::f64::consts::FRAC_PI_2).abs() < EPS);
    }

    #[test]
    fn circumcircular_arc_passes_through_three_points() {
        let (a, b, c) = (p2(1.0, 0.0), p2(0.0, 1.0), p2(-1.0, 0.0));
        let arc = circumcircular_arc(a, b, c).unwrap();
        assert!(arc.center.distance(&p2(0.0, 0.0)) < EPS);
        assert!((arc.radius - 1.0).abs() < EPS);
        for q in [a, b, c] {
            assert!((arc.center.distance(&q) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn circumcircular_arc_rejects_collinear() {
        assert!(circumcircular_arc(p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.0)).is_err());
    }

    #[test]
    fn circumcircular_sector_shares_arc_geometry() {
        let (a, b, c) = (p2(1.0, 0.0), p2(0.0, 1.0), p2(-1.0, 0.0));
        let arc = circumcircular_arc(a, b, c).unwrap();
        let sec = circumcircular_sector(a, b, c).unwrap();
        assert_eq!(sec.center, arc.center);
        assert!((sec.radius - arc.radius).abs() < EPS);
    }

    #[test]
    fn cubic_through_nine_recovers_parabola_times_line() {
        // (y − x²)·(x − 5) = 0 pasa por estos 9 puntos.
        let pts = [
            p2(0.0, 0.0),
            p2(1.0, 1.0),
            p2(-1.0, 1.0),
            p2(2.0, 4.0),
            p2(-2.0, 4.0),
            p2(5.0, 0.0),
            p2(5.0, 25.0),
            p2(5.0, -3.0),
            p2(3.0, 9.0),
        ];
        let fit = cubic_through_nine_detailed(&pts).unwrap();
        for q in pts {
            assert!(
                cubic_residual(&fit.coeffs, q) < 1e-6,
                "residuo alto en {q:?}"
            );
        }
        assert!(fit.pivot_ratio.is_finite() && fit.pivot_ratio >= 1.0);
    }

    #[test]
    fn cubic_through_nine_rejects_wrong_count() {
        let pts = [p2(0.0, 0.0); 8];
        assert!(cubic_through_nine(&pts).is_err());
    }

    #[test]
    fn cubic_through_nine_rejects_degenerate_points() {
        // 9 puntos colineales: sistema singular honesto.
        let pts: Vec<Point2> = (0..9).map(|i| p2(i as f64, 2.0 * i as f64)).collect();
        assert!(cubic_through_nine(&pts).is_err());
    }

    #[test]
    fn cubic_residual_is_zero_on_fit_point() {
        let pts = [
            p2(0.0, 0.0),
            p2(1.0, 1.0),
            p2(-1.0, 1.0),
            p2(2.0, 4.0),
            p2(-2.0, 4.0),
            p2(5.0, 0.0),
            p2(5.0, 25.0),
            p2(5.0, -3.0),
            p2(3.0, 9.0),
        ];
        let coeffs = cubic_through_nine(&pts).unwrap();
        assert!(cubic_residual(&coeffs, pts[0]) < 1e-6);
    }

    #[test]
    fn line_direction_2d_is_unit() {
        let (ux, uy) = line_direction_2d(p2(0.0, 0.0), p2(3.0, 4.0)).unwrap();
        assert!((ux - 0.6).abs() < EPS && (uy - 0.8).abs() < EPS);
    }

    #[test]
    fn line_direction_2d_rejects_coincident() {
        assert!(line_direction_2d(p2(1.0, 1.0), p2(1.0, 1.0)).is_err());
    }

    #[test]
    fn line_direction_3d_is_unit() {
        let (ux, uy, uz) = line_direction_3d(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 5.0)).unwrap();
        assert!((ux.abs() + uy.abs() + (uz - 1.0).abs()) < EPS);
    }

    #[test]
    fn plane_normal_direction_returns_unit_normal() {
        let n = plane_normal_direction(0.0, 0.0, 2.0, -4.0).unwrap();
        assert!((n.0.abs() + n.1.abs() + (n.2 - 1.0).abs()) < EPS);
    }

    #[test]
    fn plane_normal_direction_rejects_degenerate() {
        assert!(plane_normal_direction(0.0, 0.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn perpendicular_line_through_point_is_orthogonal() {
        let (through, dir) =
            perpendicular_line_through_point(p2(0.0, 1.0), p2(0.0, 0.0), p2(4.0, 0.0)).unwrap();
        assert_eq!(through, p2(0.0, 1.0));
        assert!(dir.0.abs() < EPS && (dir.1 - 1.0).abs() < EPS);
    }

    #[test]
    fn rigid_bars_for_triangle_is_three_edges() {
        assert_eq!(
            rigid_bars_for_polygon(3).unwrap(),
            vec![(0, 1), (1, 2), (2, 0)]
        );
    }

    #[test]
    fn rigid_bars_for_quad_is_five() {
        // 4 aristas + 1 diagonal = 2·4−3.
        let bars = rigid_bars_for_polygon(4).unwrap();
        assert_eq!(bars.len(), 5);
        assert!(bars.contains(&(0, 2)));
    }

    #[test]
    fn is_rigid_bar_count_checks_two_n_minus_three() {
        assert!(is_rigid_bar_count(3, 3));
        assert!(is_rigid_bar_count(4, 5));
        assert!(!is_rigid_bar_count(4, 4));
        assert!(!is_rigid_bar_count(2, 1));
    }

    #[test]
    fn validate_rigidity_bars_accepts_fan_and_rejects_dup() {
        let bars = rigid_bars_for_polygon(5).unwrap();
        assert!(validate_rigidity_bars(5, &bars).unwrap());
        let mut dup = bars.clone();
        dup.push((0, 1));
        assert!(validate_rigidity_bars(5, &dup).is_err());
        assert!(!validate_rigidity_bars(5, &bars[..6]).unwrap());
    }

    #[test]
    fn conic_through_five_recovers_unit_circle() {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let pts = [
            p2(1.0, 0.0),
            p2(0.0, 1.0),
            p2(-1.0, 0.0),
            p2(0.0, -1.0),
            p2(s, s),
        ];
        let k = conic_through_five(&pts).unwrap();
        assert_eq!(classify_conic(&k).unwrap(), ConicKind::Ellipse);
        // Círculo unitario: a≈c, b≈0, centro en el origen.
        assert!((k[0] - k[2]).abs() < 1e-6, "a≈c: {k:?}");
        assert!(k[1].abs() < 1e-6, "b≈0: {k:?}");
    }

    #[test]
    fn conic_through_five_rejects_collinear() {
        let pts = [
            p2(0.0, 0.0),
            p2(1.0, 0.0),
            p2(2.0, 0.0),
            p2(3.0, 0.0),
            p2(4.0, 0.0),
        ];
        assert!(conic_through_five(&pts).is_err());
    }

    #[test]
    fn classify_conic_sorts_by_discriminant() {
        // Hipérbola xy = 1 → b=1 resto cuadrático 0.
        assert_eq!(
            classify_conic(&[0.0, 1.0, 0.0, 0.0, 0.0, -1.0]).unwrap(),
            ConicKind::Hyperbola
        );
        // Parábola y = x² → a=1, e=−1.
        assert_eq!(
            classify_conic(&[1.0, 0.0, 0.0, 0.0, -1.0, 0.0]).unwrap(),
            ConicKind::Parabola
        );
        // Par de rectas x²−y²=0 → degenerada.
        assert_eq!(
            classify_conic(&[1.0, 0.0, -1.0, 0.0, 0.0, 0.0]).unwrap(),
            ConicKind::Degenerate
        );
    }

    #[test]
    fn parabola_focal_parameter_returns_abs() {
        assert_eq!(parabola_focal_parameter(2.0).unwrap(), 2.0);
        assert_eq!(parabola_focal_parameter(-2.0).unwrap(), 2.0);
        assert!(parabola_focal_parameter(0.0).is_err());
    }

    #[test]
    fn ellipse_focal_parameter_is_b_squared_over_a() {
        assert!((ellipse_focal_parameter(3.0, 2.0).unwrap() - 4.0 / 3.0).abs() < EPS);
        assert!((ellipse_focal_parameter(2.0, 3.0).unwrap() - 4.0 / 3.0).abs() < EPS);
    }

    #[test]
    fn hyperbola_focal_parameter_is_b_squared_over_a() {
        assert!((hyperbola_focal_parameter(3.0, 4.0).unwrap() - 16.0 / 3.0).abs() < EPS);
    }

    #[test]
    fn path_parameter_polyline_marks_half_segment() {
        let verts = [p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0)];
        assert!((path_parameter_polyline(p2(2.0, 0.0), &verts).unwrap() - 0.5).abs() < EPS);
        assert!((path_parameter_polyline(p2(4.0, 2.0), &verts).unwrap() - 1.5).abs() < EPS);
    }

    #[test]
    fn polygon_vertex_list_validates() {
        let v = [p2(0.0, 0.0), p2(1.0, 0.0), p2(0.0, 1.0)];
        assert_eq!(polygon_vertex_list(&v).unwrap(), v);
        assert!(polygon_vertex_list(&v[..2]).is_err());
    }

    #[test]
    fn ellipse_vertices_span_both_axes() {
        let v = ellipse_vertices(p2(0.0, 0.0), 3.0, 2.0, 0.0).unwrap();
        assert!((v[0].x - 3.0).abs() < EPS && v[0].y.abs() < EPS);
        assert!((v[1].x + 3.0).abs() < EPS);
        assert!(v[2].x.abs() < EPS && (v[2].y - 2.0).abs() < EPS);
        assert!(v[3].x.abs() < EPS && (v[3].y + 2.0).abs() < EPS);
    }

    #[test]
    fn hyperbola_vertices_oppose_on_transverse_axis() {
        let v = hyperbola_vertices(p2(1.0, 1.0), 2.0, 0.0).unwrap();
        assert!((v[0].x - 3.0).abs() < EPS && (v[0].y - 1.0).abs() < EPS);
        assert!((v[1].x + 1.0).abs() < EPS && (v[1].y - 1.0).abs() < EPS);
    }

    #[test]
    fn parabola_vertex_is_validated_identity() {
        assert_eq!(parabola_vertex(p2(2.0, 3.0)).unwrap(), p2(2.0, 3.0));
    }

    #[test]
    fn interior_angles_of_square_are_right() {
        let sq = [p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 1.0), p2(0.0, 1.0)];
        let angs = interior_angles(&sq).unwrap();
        assert_eq!(angs.len(), 4);
        for a in angs {
            assert!((a - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "got {a}");
        }
    }

    #[test]
    fn interior_angles_sum_to_two_pi_for_triangle() {
        let tri = [p2(0.0, 0.0), p2(4.0, 0.0), p2(0.0, 3.0)];
        let sum: f64 = interior_angles(&tri).unwrap().iter().sum();
        assert!((sum - std::f64::consts::PI).abs() < 1e-9, "got {sum}");
    }

    #[test]
    fn cube_z_extent_is_centered() {
        assert_eq!(cube_z_extent(p3(0.0, 0.0, 5.0), 2.0).unwrap(), (4.0, 6.0));
    }

    #[test]
    fn sphere_z_extent_is_radius() {
        assert_eq!(sphere_z_extent(p3(1.0, 2.0, 3.0), 2.0).unwrap(), (1.0, 5.0));
    }

    #[test]
    fn cylinder_z_extent_vertical_ignores_radius() {
        let (lo, hi) = cylinder_z_extent(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 4.0), 1.0).unwrap();
        assert!((lo - 0.0).abs() < EPS && (hi - 4.0).abs() < EPS);
    }

    #[test]
    fn cylinder_z_extent_tilted_adds_radius() {
        // Eje X: el radio ensancha ±1 en Z.
        let (lo, hi) = cylinder_z_extent(p3(0.0, 0.0, 0.0), p3(4.0, 0.0, 0.0), 1.0).unwrap();
        assert!((lo + 1.0).abs() < EPS && (hi - 1.0).abs() < EPS);
    }

    #[test]
    fn cone_z_extent_covers_base_disc_and_apex() {
        let (lo, hi) = cone_z_extent(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 3.0), 1.0).unwrap();
        assert!((lo - 0.0).abs() < EPS && (hi - 3.0).abs() < EPS);
    }

    #[test]
    fn prism_z_extent_covers_both_caps() {
        let base = [p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), p3(0.0, 1.0, 1.0)];
        assert_eq!(
            prism_z_extent(&base, p3(0.0, 0.0, 2.0)).unwrap(),
            (1.0, 3.0)
        );
    }

    #[test]
    fn pyramid_z_extent_covers_base_and_apex() {
        let (lo, hi) = pyramid_z_extent(p3(0.0, 0.0, 0.0), p3(0.0, 5.0, 4.0), 2.0).unwrap();
        assert!((lo + 1.0).abs() < EPS && (hi - 4.0).abs() < EPS);
    }

    #[test]
    fn torus_z_extent_is_tube() {
        assert_eq!(
            torus_z_extent(p3(0.0, 0.0, 7.0), 5.0, 1.0).unwrap(),
            (6.0, 8.0)
        );
    }

    #[test]
    fn z_plane_is_horizontal_at_level() {
        let pl = z_plane(3.0).unwrap();
        assert!((pl.a.abs() + pl.b.abs() + (pl.c - 1.0).abs() + (pl.d + 3.0).abs()) < EPS);
    }

    #[test]
    fn cylinder_end_planes_are_orthogonal_to_axis() {
        let (p0, p1) = cylinder_end_planes(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 4.0), 1.0).unwrap();
        assert!((p0.c - 1.0).abs() < EPS && p0.d.abs() < EPS);
        assert!((p1.c - 1.0).abs() < EPS && (p1.d + 4.0).abs() < EPS);
    }

    #[test]
    fn prism_end_planes_contain_both_centroids() {
        let base = [p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0)];
        let (p0, p1) = prism_end_planes(&base, p3(0.0, 0.0, 2.0)).unwrap();
        // Base en z=0 con normal +Z: planos z=0 y z=2.
        assert!((p0.c - 1.0).abs() < EPS && p0.d.abs() < EPS);
        assert!((p1.c - 1.0).abs() < EPS && (p1.d + 2.0).abs() < EPS);
    }

    #[test]
    fn cylinder_lateral_area_matches_formula() {
        let a = cylinder_lateral_area(2.0, 3.0).unwrap();
        assert!((a - 12.0 * std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn cone_lateral_area_matches_slant() {
        let a = cone_lateral_area(3.0, 4.0).unwrap();
        assert!((a - 15.0 * std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn prism_lateral_area_is_perimeter_times_extrusion() {
        let base = [p3(0.0, 0.0, 0.0), p3(3.0, 0.0, 0.0), p3(0.0, 4.0, 0.0)];
        assert!((prism_lateral_area(&base, p3(0.0, 0.0, 2.0)).unwrap() - 24.0).abs() < EPS);
    }

    #[test]
    fn sphere_plane_circle_through_center_is_great() {
        let c = sphere_plane_circle(
            p3(0.0, 0.0, 0.0),
            2.0,
            Plane3D::from_equation(0.0, 0.0, 1.0, 0.0),
        )
        .unwrap();
        assert!(c.center.distance(&p3(0.0, 0.0, 0.0)) < EPS);
        assert!((c.radius - 2.0).abs() < EPS);
    }

    #[test]
    fn sphere_plane_circle_offset_shrinks() {
        let c = sphere_plane_circle(
            p3(0.0, 0.0, 0.0),
            2.0,
            Plane3D::from_equation(0.0, 0.0, 1.0, -1.0),
        )
        .unwrap();
        assert!((c.radius - 3.0f64.sqrt()).abs() < 1e-9);
        assert!((c.center.z - 1.0).abs() < EPS);
    }

    #[test]
    fn sphere_plane_circle_miss_and_tangent_are_honest() {
        let far = Plane3D::from_equation(0.0, 0.0, 1.0, -5.0);
        assert!(sphere_plane_circle(p3(0.0, 0.0, 0.0), 2.0, far).is_err());
        let tangent = Plane3D::from_equation(0.0, 0.0, 1.0, -2.0);
        assert!(sphere_plane_circle(p3(0.0, 0.0, 0.0), 2.0, tangent).is_err());
    }

    #[test]
    fn intersect_conic_unsupported_names_the_pair() {
        let msg = intersect_conic_unsupported("Cubo", "Plano");
        assert!(msg.contains("Cubo") && msg.contains("Plano"));
    }
}
