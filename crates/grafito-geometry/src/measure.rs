//! Medidas, centros y proximidad (P2): área/perímetro/longitud, centros de
//! triángulo Kimberling 1–6, baricentro, trilineales, punto-en-polígono,
//! punto más cercano, envolvente de familia de rectas y arclength Bézier.
//!
//! Todo acotado y determinista. La envolvente es numérica (diferencias
//! centrales + 2×2 exacto por muestra) y lo declara.

use crate::Point2;

/// Muestras máximas de una envolvente.
pub const MAX_ENVELOPE_SAMPLES: usize = 200;
/// Vértices máximos para medidas poligonales (techo de Polygon).
pub const MAX_MEASURE_VERTICES: usize = 8192;
/// Intervalos máximos del Simpson adaptativo de arclength.
pub const MAX_ARCLENGTH_INTERVALS: usize = 4096;

fn check_vertices(pts: &[Point2], what: &str) -> Result<(), String> {
    if pts.len() > MAX_MEASURE_VERTICES {
        return Err(format!(
            "{what}: {} vértices exceden el máximo {MAX_MEASURE_VERTICES}",
            pts.len()
        ));
    }
    if !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return Err(format!("{what}: vértices no finitos"));
    }
    Ok(())
}

/// Área con signo (shoelace); positiva en CCW.
pub fn polygon_area_signed(pts: &[Point2]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

/// Área de polígono (valor absoluto).
pub fn polygon_area(pts: &[Point2]) -> Result<f64, String> {
    check_vertices(pts, "área")?;
    if pts.len() < 3 {
        return Err("se necesitan al menos 3 vértices".to_string());
    }
    Ok(polygon_area_signed(pts).abs())
}

/// Perímetro de polígono cerrado.
pub fn polygon_perimeter(pts: &[Point2]) -> Result<f64, String> {
    check_vertices(pts, "perímetro")?;
    if pts.len() < 2 {
        return Err("se necesitan al menos 2 vértices".to_string());
    }
    let mut total = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        total += (b.x - a.x).hypot(b.y - a.y);
    }
    Ok(total)
}

/// Longitud de polilínea abierta.
pub fn polyline_length(pts: &[Point2]) -> Result<f64, String> {
    check_vertices(pts, "longitud")?;
    if pts.len() < 2 {
        return Err("se necesitan al menos 2 puntos".to_string());
    }
    let mut total = 0.0;
    for pair in pts.windows(2) {
        total += (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y);
    }
    Ok(total)
}

/// Área del círculo.
pub fn circle_area(radius: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius < 0.0 {
        return Err("radio debe ser finito no negativo".to_string());
    }
    Ok(std::f64::consts::PI * radius * radius)
}

/// Circunferencia del círculo.
pub fn circle_circumference(radius: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius < 0.0 {
        return Err("radio debe ser finito no negativo".to_string());
    }
    Ok(2.0 * std::f64::consts::PI * radius)
}

/// Longitud de arco circular (ángulos en radianes, cualquier orden).
pub fn arc_length(radius: f64, start: f64, end: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius < 0.0 {
        return Err("radio debe ser finito no negativo".to_string());
    }
    if !start.is_finite() || !end.is_finite() {
        return Err("ángulos deben ser finitos".to_string());
    }
    Ok(radius * (end - start).abs())
}

/// Área de la elipse.
pub fn ellipse_area(rx: f64, ry: f64) -> Result<f64, String> {
    if ![rx, ry].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("semiejes deben ser finitos no negativos".to_string());
    }
    Ok(std::f64::consts::PI * rx * ry)
}

/// Perímetro de la elipse (Ramanujan II, error relativo < 1e-7).
pub fn ellipse_perimeter(rx: f64, ry: f64) -> Result<f64, String> {
    if ![rx, ry].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("semiejes deben ser finitos no negativos".to_string());
    }
    if rx == 0.0 || ry == 0.0 {
        return Ok(4.0 * rx.max(ry));
    }
    let h = ((rx - ry) / (rx + ry)).powi(2);
    Ok(std::f64::consts::PI * (rx + ry) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt())))
}

/// Ejes ordenados (mayor, menor) de elipse con semiejes rx, ry.
pub fn ellipse_axes(rx: f64, ry: f64) -> Result<(f64, f64), String> {
    if ![rx, ry].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("semiejes deben ser finitos no negativos".to_string());
    }
    Ok((2.0 * rx.max(ry), 2.0 * rx.min(ry)))
}

/// Excentricidad lineal de elipse `sqrt(|rx²−ry²|)`.
pub fn ellipse_linear_eccentricity(rx: f64, ry: f64) -> Result<f64, String> {
    if ![rx, ry].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err("semiejes deben ser finitos no negativos".to_string());
    }
    Ok((rx * rx - ry * ry).abs().sqrt())
}

/// Excentricidad lineal de hipérbola `sqrt(a²+b²)`.
pub fn hyperbola_linear_eccentricity(a: f64, b: f64) -> Result<f64, String> {
    if ![a, b].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err("semiejes de hipérbola deben ser finitos positivos".to_string());
    }
    Ok((a * a + b * b).sqrt())
}

/// Longitud de arco de curva Bézier de grado arbitrario (Simpson adaptativo
/// sobre la norma de la derivada de Bernstein; tolerancia 1e-9).
pub fn bezier_arclength(points: &[Point2]) -> Result<f64, String> {
    check_vertices(points, "arclength")?;
    if points.len() < 2 {
        return Err("se necesitan al menos 2 puntos de control".to_string());
    }
    let n = points.len() - 1;
    // Coeficientes binomiales con checked (grado ≤ 8191 cabe en f64 de sobra,
    // pero el binomio intermedio puede desbordar u64: usa f64 directo).
    let binom = |n: usize, i: usize| -> f64 {
        let mut acc = 1.0;
        for k in 0..i {
            acc *= (n - k) as f64 / (k + 1) as f64;
        }
        acc
    };
    let point_at = |t: f64| -> Point2 {
        let (mut x, mut y) = (0.0, 0.0);
        for (i, p) in points.iter().enumerate() {
            let w = binom(n, i) * (1.0 - t).powi((n - i) as i32) * t.powi(i as i32);
            x += w * p.x;
            y += w * p.y;
        }
        Point2::new(x, y)
    };
    let speed = |t: f64| -> f64 {
        let h = 1e-7f64;
        let a = point_at((t - h).max(0.0));
        let b = point_at((t + h).min(1.0));
        let dt = (t + h).min(1.0) - (t - h).max(0.0);
        if dt == 0.0 {
            return 0.0;
        }
        ((b.x - a.x).hypot(b.y - a.y)) / dt
    };
    // Simpson adaptativo iterativo con pila acotada.
    let simpson = |a: f64, b: f64| -> f64 {
        let m = (a + b) * 0.5;
        (b - a) / 6.0 * (speed(a) + 4.0 * speed(m) + speed(b))
    };
    let mut total = 0.0;
    let mut stack = vec![(0.0f64, 1.0f64, simpson(0.0, 1.0), 0usize)];
    let mut intervals = 0usize;
    while let Some((a, b, whole, depth)) = stack.pop() {
        intervals += 1;
        if intervals > MAX_ARCLENGTH_INTERVALS {
            return Err(format!(
                "arclength excede {MAX_ARCLENGTH_INTERVALS} intervalos; curva degenerada"
            ));
        }
        let m = (a + b) * 0.5;
        let left = simpson(a, m);
        let right = simpson(m, b);
        if depth >= 20 || (left + right - whole).abs() <= 1e-9 * (1.0 + whole.abs()) {
            total += left + right + (left + right - whole) / 15.0;
        } else {
            stack.push((m, b, right, depth + 1));
            stack.push((a, m, left, depth + 1));
        }
    }
    if !total.is_finite() {
        return Err("arclength no finito".to_string());
    }
    Ok(total)
}

/// Centroide (promedio de vértices).
pub fn centroid3(a: Point2, b: Point2, c: Point2) -> Point2 {
    Point2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
}

/// Longitudes de lados opuestos a cada vértice.
fn side_lengths(a: Point2, b: Point2, c: Point2) -> (f64, f64, f64) {
    let dist = |p: Point2, q: Point2| (q.x - p.x).hypot(q.y - p.y);
    (dist(b, c), dist(c, a), dist(a, b))
}

/// Incentro (ponderado por lados).
pub fn incenter(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let (la, lb, lc) = side_lengths(a, b, c);
    let per = la + lb + lc;
    if per == 0.0 || !per.is_finite() {
        return None;
    }
    Some(Point2::new(
        (la * a.x + lb * b.x + lc * c.x) / per,
        (la * a.y + lb * b.y + lc * c.y) / per,
    ))
}

/// Circuncentro (intersección de mediatrices); `None` si degenerado.
pub fn circumcenter(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-15 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    Some(Point2::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    ))
}

/// Ortocentro; `None` si degenerado.
pub fn orthocenter(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    // H = A + B + C − 2·O (O circuncentro).
    let o = circumcenter(a, b, c)?;
    Some(Point2::new(
        a.x + b.x + c.x - 2.0 * o.x,
        a.y + b.y + c.y - 2.0 * o.y,
    ))
}

/// Punto de nueve puntos (punto medio entre ortocentro y circuncentro).
pub fn nine_point_center(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let o = circumcenter(a, b, c)?;
    let h = orthocenter(a, b, c)?;
    Some(Point2::new((o.x + h.x) * 0.5, (o.y + h.y) * 0.5))
}

/// Punto simmediano (barycéntrico a²:b²:c²).
pub fn symmedian_point(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let (la, lb, lc) = side_lengths(a, b, c);
    let (wa, wb, wc) = (la * la, lb * lb, lc * lc);
    let sum = wa + wb + wc;
    if sum == 0.0 || !sum.is_finite() {
        return None;
    }
    Some(Point2::new(
        (wa * a.x + wb * b.x + wc * c.x) / sum,
        (wa * a.y + wb * b.y + wc * c.y) / sum,
    ))
}

/// Centro de Kimberling 1–6: centroide, circuncentro, incentro, ortocentro,
/// nueve puntos, simmediano. El resto → error honesto con la tabla.
pub fn triangle_center(n: i64, a: Point2, b: Point2, c: Point2) -> Result<Point2, String> {
    for p in [a, b, c] {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err("vértices deben ser finitos".to_string());
        }
    }
    match n {
        1 => Ok(centroid3(a, b, c)),
        2 => circumcenter(a, b, c).ok_or_else(|| "triángulo degenerado".to_string()),
        3 => incenter(a, b, c).ok_or_else(|| "triángulo degenerado".to_string()),
        4 => orthocenter(a, b, c).ok_or_else(|| "triángulo degenerado".to_string()),
        5 => nine_point_center(a, b, c).ok_or_else(|| "triángulo degenerado".to_string()),
        6 => symmedian_point(a, b, c).ok_or_else(|| "triángulo degenerado".to_string()),
        _ => Err("centro n soportado: 1 centroide, 2 circuncentro, 3 incentro, 4 ortocentro, 5 nueve puntos, 6 simmediano".to_string()),
    }
}

/// Baricentro (promedio) de ≥1 puntos.
pub fn barycenter(points: &[Point2]) -> Result<Point2, String> {
    if points.is_empty() || points.len() > MAX_MEASURE_VERTICES {
        return Err("se requiere 1+ puntos dentro de la cota".to_string());
    }
    let (mut x, mut y) = (0.0, 0.0);
    for p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err("puntos deben ser finitos".to_string());
        }
        x += p.x;
        y += p.y;
    }
    Ok(Point2::new(
        x / points.len() as f64,
        y / points.len() as f64,
    ))
}

/// Punto trilinear α:β:γ sobre el triángulo (→ baricéntrico aα:bβ:cγ).
pub fn trilinear_point(
    alpha: f64,
    beta: f64,
    gamma: f64,
    a: Point2,
    b: Point2,
    c: Point2,
) -> Result<Point2, String> {
    if ![alpha, beta, gamma].iter().all(|v| v.is_finite()) {
        return Err("coordenadas trilineales deben ser finitas".to_string());
    }
    let (la, lb, lc) = side_lengths(a, b, c);
    let (wa, wb, wc) = (la * alpha, lb * beta, lc * gamma);
    let sum = wa + wb + wc;
    if sum == 0.0 || !sum.is_finite() {
        return Err("trilineales degeneradas (suma nula)".to_string());
    }
    Ok(Point2::new(
        (wa * a.x + wb * b.x + wc * c.x) / sum,
        (wa * a.y + wb * b.y + wc * c.y) / sum,
    ))
}

/// Centroide de área de un polígono (fórmula shoelace).
///
/// Para triángulos coincide con el promedio de vértices; para polígonos
/// generales pondera por área. `None` si el área es nula.
pub fn polygon_centroid(pts: &[Point2]) -> Option<Point2> {
    if pts.len() < 3 {
        return None;
    }
    let (mut cx, mut cy, mut twice) = (0.0, 0.0, 0.0);
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let cross = a.x * b.y - b.x * a.y;
        cx += (a.x + b.x) * cross;
        cy += (a.y + b.y) * cross;
        twice += cross;
    }
    if twice.abs() < 1e-15 {
        return None;
    }
    Some(Point2::new(cx / (3.0 * twice), cy / (3.0 * twice)))
}

/// Punto en polígono (ray casting; el borde cuenta como dentro).
pub fn point_in_polygon(p: Point2, poly: &[Point2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        // Punto sobre el segmento → dentro.
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let dot = (p.x - a.x) * (p.x - b.x) + (p.y - a.y) * (p.y - b.y);
        if cross.abs() <= 1e-12 * (1.0 + (b.x - a.x).hypot(b.y - a.y)) && dot <= 0.0 {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) {
            let at_x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < at_x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Punto más cercano sobre un segmento.
pub fn closest_on_segment(p: Point2, a: Point2, b: Point2) -> Point2 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let denom = abx * abx + aby * aby;
    if denom == 0.0 {
        return a;
    }
    let t = (((p.x - a.x) * abx + (p.y - a.y) * aby) / denom).clamp(0.0, 1.0);
    Point2::new(a.x + t * abx, a.y + t * aby)
}

/// Punto más cercano sobre polilínea/polígono (`closed` cierra el anillo).
pub fn closest_on_polyline(p: Point2, pts: &[Point2], closed: bool) -> Option<Point2> {
    if pts.is_empty() {
        return None;
    }
    if pts.len() == 1 {
        return Some(pts[0]);
    }
    let segs = if closed { pts.len() } else { pts.len() - 1 };
    let mut best: Option<Point2> = None;
    let mut best_d = f64::INFINITY;
    for i in 0..segs {
        let q = closest_on_segment(p, pts[i], pts[(i + 1) % pts.len()]);
        let d = (q.x - p.x).hypot(q.y - p.y);
        if d < best_d {
            best_d = d;
            best = Some(q);
        }
    }
    best
}

/// Punto más cercano sobre un círculo.
pub fn closest_on_circle(p: Point2, center: Point2, radius: f64) -> Option<Point2> {
    if !radius.is_finite() || radius < 0.0 {
        return None;
    }
    let dx = p.x - center.x;
    let dy = p.y - center.y;
    let d = dx.hypot(dy);
    if d == 0.0 {
        return Some(Point2::new(center.x + radius, center.y));
    }
    Some(Point2::new(
        center.x + dx / d * radius,
        center.y + dy / d * radius,
    ))
}

/// Envolvente de familia de rectas `a(t)·x + b(t)·y + c(t) = 0`.
///
/// Muestrea `n` valores en `[t0, t1]` (2..=`MAX_ENVELOPE_SAMPLES`) y resuelve
/// el sistema con la derivada numérica (diferencias centrales): cada punto
/// satisface la recta y su tangente en `t`. Tramos singulares se saltean;
/// con <2 puntos, error honesto.
pub fn envelope_lines(
    a_expr: &str,
    b_expr: &str,
    c_expr: &str,
    t0: f64,
    t1: f64,
    n: usize,
) -> Result<Vec<Point2>, String> {
    use crate::ast::parse_ast;
    if ![t0, t1].iter().all(|v| v.is_finite()) || t0 >= t1 {
        return Err("se requiere t0 < t1 finitos".to_string());
    }
    if !(2..=MAX_ENVELOPE_SAMPLES).contains(&n) {
        return Err(format!("n debe estar entre 2 y {MAX_ENVELOPE_SAMPLES}"));
    }
    let parse = |s: &str| {
        parse_ast(&s.replace(' ', "")).map_err(|e| format!("no se pudo parsear '{s}': {e}"))
    };
    let (aa, bb, cc) = (parse(a_expr)?, parse(b_expr)?, parse(c_expr)?);
    let mut points = Vec::new();
    for i in 0..n {
        let t = t0 + (t1 - t0) * (i as f64) / ((n - 1) as f64);
        let h = 1e-6 * (1.0 + t.abs());
        let at = |ast: &crate::ast::Expr, tt: f64| -> Result<f64, String> {
            let v = ast.eval_at("t", tt);
            v.is_finite()
                .then_some(v)
                .ok_or_else(|| format!("coeficiente no evaluable en t={tt}"))
        };
        let (a, b, c) = (at(&aa, t)?, at(&bb, t)?, at(&cc, t)?);
        let (ap, bp, cp) = (
            (at(&aa, t + h)? - at(&aa, t - h)?) / (2.0 * h),
            (at(&bb, t + h)? - at(&bb, t - h)?) / (2.0 * h),
            (at(&cc, t + h)? - at(&cc, t - h)?) / (2.0 * h),
        );
        // [a b; a' b'] [x y]ᵀ = [−c, −c']ᵀ.
        let det = a * bp - ap * b;
        if det.abs() < 1e-12 * (1.0 + a.abs() + b.abs() + ap.abs() + bp.abs()) {
            continue;
        }
        let x = (-c * bp + b * cp) / det;
        let y = (-a * cp + ap * c) / det;
        if x.is_finite() && y.is_finite() {
            points.push(Point2::new(x, y));
        }
    }
    if points.len() < 2 {
        return Err("la envolvente no produjo puntos (familia singular?)".to_string());
    }
    Ok(points)
}

/// Formas lineales baricéntricas `A(x,y), B(x,y), C(x,y)` de un triángulo.
///
/// Devuelve los tres trinomios como strings listos para sustituir los
/// identificadores `A`, `B`, `C` de una ecuación de TriangleCurve.
pub fn barycentric_forms(p1: Point2, p2: Point2, p3: Point2) -> Result<[String; 3], String> {
    for p in [p1, p2, p3] {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err("vértices deben ser finitos".to_string());
        }
    }
    // A(P) = área(P,P2,P3)/área(P1,P2,P3), etc. Cada forma es αx+βy+γ.
    let area2 =
        |a: Point2, b: Point2, c: Point2| (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
    let total = area2(p1, p2, p3);
    if total.abs() < 1e-15 {
        return Err("triángulo degenerado para baricéntricas".to_string());
    }
    // A(x,y) = ((y2−y3)x + (x3−x2)y + (x2y3−x3y2)) / total.
    let form = |a: Point2, b: Point2| -> String {
        let cx = b.y - a.y;
        let cy = a.x - b.x;
        let cc = a.y * b.x - a.x * b.y;
        format!("(({cx}*x+({cy})*y+({cc}))/({total})")
    };
    Ok([form(p2, p3), form(p3, p1), form(p1, p2)])
}

/// Sustituye identificadores sueltos `A`, `B`, `C` por sus formas.
///
/// Solo reemplaza palabras completas (nunca subcadenas): tokeniza por
/// caracteres no alfanuméricos ni `_`.
pub fn substitute_barycentric(equation: &str, forms: &[String; 3]) -> String {
    let mut out = String::with_capacity(equation.len() + 64);
    let mut word = String::new();
    let flush = |out: &mut String, word: &mut String| {
        match word.as_str() {
            "A" => out.push_str(&format!("({})", forms[0])),
            "B" => out.push_str(&format!("({})", forms[1])),
            "C" => out.push_str(&format!("({})", forms[2])),
            _ => out.push_str(word),
        }
        word.clear();
    };
    for ch in equation.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            flush(&mut out, &mut word);
            out.push(ch);
        }
    }
    flush(&mut out, &mut word);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_area_perimeter() {
        let sq = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        assert_eq!(polygon_area(&sq).unwrap(), 4.0);
        assert_eq!(polygon_perimeter(&sq).unwrap(), 8.0);
    }

    #[test]
    fn ellipse_ramanujan_circle_limit() {
        // Círculo r=1: Ramanujan ≈ 2π con error < 1e-7 relativo.
        let p = ellipse_perimeter(1.0, 1.0).unwrap();
        assert!((p - 2.0 * std::f64::consts::PI).abs() < 1e-9, "got {p}");
        assert_eq!(ellipse_area(2.0, 3.0).unwrap(), 6.0 * std::f64::consts::PI);
    }

    #[test]
    fn centers_of_345_triangle() {
        let (a, b, c) = (
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(0.0, 3.0),
        );
        let g = triangle_center(1, a, b, c).unwrap();
        assert!(((g.x - 4.0 / 3.0).abs() < 1e-9) && ((g.y - 1.0).abs() < 1e-9));
        // Incentro del 3-4-5 en (1,1).
        let i = triangle_center(3, a, b, c).unwrap();
        assert!((i.x - 1.0).abs() < 1e-9 && (i.y - 1.0).abs() < 1e-9);
        // Circuncentro en el punto medio de la hipotenusa (2,1.5).
        let o = triangle_center(2, a, b, c).unwrap();
        assert!((o.x - 2.0).abs() < 1e-9 && (o.y - 1.5).abs() < 1e-9);
        assert!(triangle_center(7, a, b, c).is_err());
    }

    #[test]
    fn point_in_polygon_boundary_counts_inside() {
        let sq = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        assert!(point_in_polygon(Point2::new(1.0, 1.0), &sq));
        assert!(point_in_polygon(Point2::new(0.0, 1.0), &sq));
        assert!(!point_in_polygon(Point2::new(3.0, 1.0), &sq));
    }

    #[test]
    fn envelope_of_tangent_family_is_parabola() {
        // Rectas 2tx − y + t² = 0 (tangentes a y = −x²): envolvente = −x².
        let pts = envelope_lines("2*t", "-1", "t^2", -2.0, 2.0, 41).expect("envolvente");
        assert!(pts.len() >= 30);
        for p in &pts {
            assert!(
                (p.y + p.x * p.x).abs() < 1e-6,
                "fuera de la parábola: {p:?}"
            );
        }
    }

    #[test]
    fn bezier_line_arclength() {
        // Bézier lineal (0,0)-(3,4): longitud 5.
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(3.0, 4.0)];
        assert!((bezier_arclength(&pts).unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn barycentric_substitution_avoids_substrings() {
        let forms = ["(FA)".to_string(), "(FB)".to_string(), "(FC)".to_string()];
        // `ABC` no debe tocarse; solo palabras exactas A/B/C.
        let out = substitute_barycentric("A*B+ABC=C", &forms);
        assert!(out.contains("((FA))*((FB))+ABC=((FC))"), "got {out}");
    }
}
