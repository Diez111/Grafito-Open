use grafito_core::{
    Attractor3DObj, BoxPlotObj, ComplexGridObj, Cone3DObj, Cube3DObj, Cylinder3DObj, Document,
    EllipseObj, Fractal2DObj, FunctionObj, GeoObject, HistogramObj, HyperSurface4DObj,
    HyperbolaObj, ImplicitCurveObj, LineObj, MoebiusStripObj, ObjectId, ParabolaObj,
    PhasePortraitObj, ParametricCurve2DObj, Point3DObj, PointObj, PolarCurveObj, PolygonObj, RegressionLineObj,
    RelationOperator, ScatterPlotObj, Segment3DObj, Sphere3DObj, Surface3DObj, Torus3DObj,
    VectorField2DObj, VectorField3DObj,
};
use grafito_geometry::expr::{eval_function_with_vars, evaluate};
use grafito_geometry::matrices::{taylor_series, Matrix};
use grafito_geometry::statistics;
use grafito_geometry::symbolic;
use grafito_geometry::Color;
use grafito_geometry::Point2;
use grafito_geometry::Point3D;
use std::collections::{HashMap, HashSet};

fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_digit() {
                res.push('*');
            }
            if c1.is_ascii_digit() && c2 == '(' {
                res.push('*');
            }
            if c1 == ')' && c2 == '(' {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2 == '('
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2.is_ascii_alphabetic()
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
        }
    }
    res
}

pub fn process_input(document: &mut Document, input_text: &mut String) -> Option<String> {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() {
        return None;
    }

    // Sanitize mathematical unicode symbols and uppercase variables from virtual keyboard
    let text = raw_text
        .replace("X", "x")
        .replace("Y", "y")
        .replace("F(x)", "f(x)")
        .replace("G(x)", "g(x)")
        .replace("x²", "x^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "3.14159265359")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("≤", "<=")
        .replace("≥", ">=");

    let text = insert_implicit_multiplication(&text);

    let mut result: Option<String> = None;

    if let Some(mut cmd) = parse_cas_command(&text) {
        cmd.args = cmd
            .args
            .iter()
            .map(|a| insert_implicit_multiplication(a))
            .collect();
        match cmd.command.as_str() {
            "Ellipse" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if parts.len() >= 2 {
                    let rx = cmd.args[1].trim().parse().unwrap_or(1.0);
                    let ry = cmd.args[2].trim().parse().unwrap_or(1.0);
                    document.add_object(GeoObject::Ellipse(EllipseObj::new(
                        Point2::new(parts[0], parts[1]),
                        rx,
                        ry,
                    )));
                    input_text.clear();
                    return None;
                }
            }
            "RegularPolygon" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                if parts.len() >= 2 {
                    let n = cmd.args[1]
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(4)
                        .clamp(3, 64);
                    let r = cmd.args[2].trim().parse::<f64>().unwrap_or(1.0);
                    let cx = parts[0];
                    let cy = parts[1];
                    let verts: Vec<Point2> = (0..n)
                        .map(|i| {
                            let a = i as f64 / n as f64 * std::f64::consts::TAU;
                            Point2::new(cx + r * a.cos(), cy + r * a.sin())
                        })
                        .collect();
                    document.add_object(GeoObject::Polygon(PolygonObj::new(verts)));
                    input_text.clear();
                    return None;
                }
            }
            "Translate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok((dx, dy))) = (
                    find_object_by_label(document, &cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                ) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let new_pos = Point2::new(p.position.x + dx, p.position.y + dy);
                                let (_, cons_id) = document.add_constructed_object(
                                    GeoObject::Point(
                                        PointObj::new(new_pos).with_label(format!("{}'", p.label)),
                                    ),
                                    "Translate",
                                    &[id],
                                );
                                document.set_constraint_param(cons_id, "_tr_dx", dx);
                                document.set_constraint_param(cons_id, "_tr_dy", dy);
                            }
                            _ => {
                                result = Some("Translate only supports Points".into());
                            }
                        }
                    }
                } else {
                    result = Some("Usage: Translate[Object, (dx,dy)]".into());
                }
                input_text.clear();
                return result;
            }
            "Rotate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok(angle)) = (
                    find_object_by_label(document, &cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                ) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let c = angle.to_radians();
                                let nx = p.position.x * c.cos() - p.position.y * c.sin();
                                let ny = p.position.x * c.sin() + p.position.y * c.cos();
                                let (_, cons_id) = document.add_constructed_object(
                                    GeoObject::Point(
                                        PointObj::new(Point2::new(nx, ny))
                                            .with_label(format!("{}'", p.label)),
                                    ),
                                    "Rotate",
                                    &[id],
                                );
                                document.set_constraint_param(cons_id, "_rot_a", angle);
                            }
                            _ => {
                                result = Some("Rotate only supports Points".into());
                            }
                        }
                    }
                } else {
                    result = Some("Usage: Rotate[Object, angle_degrees]".into());
                }
                input_text.clear();
                return result;
            }
            "Surface3D" if cmd.args.len() >= 5 => {
                let expr = cmd.args[0].trim();
                if let (Ok(x_min), Ok(x_max), Ok(y_min), Ok(y_max)) = (
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                    cmd.args[3].trim().parse::<f64>(),
                    cmd.args[4].trim().parse::<f64>(),
                ) {
                    let obj = GeoObject::Surface3D(Surface3DObj::new(
                        expr,
                        (x_min, x_max),
                        (y_min, y_max),
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Point3D" if cmd.args.len() == 3 => {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                ) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Segment3D" if cmd.args.len() == 6 => {
                if let (Ok(x1), Ok(y1), Ok(z1), Ok(x2), Ok(y2), Ok(z2)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                    cmd.args[4].trim().parse(),
                    cmd.args[5].trim().parse(),
                ) {
                    let obj = GeoObject::Segment3D(Segment3DObj::new(
                        Point3D::new(x1, y1, z1),
                        Point3D::new(x2, y2, z2),
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Sphere" if cmd.args.len() == 4 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                ) {
                    let obj = GeoObject::Sphere3D(Sphere3DObj::new(Point3D::new(x, y, z), r));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Cube" if cmd.args.len() == 4 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(s)) = (
                    cmd.args[0].trim().parse(),
                    cmd.args[1].trim().parse(),
                    cmd.args[2].trim().parse(),
                    cmd.args[3].trim().parse(),
                ) {
                    let obj = GeoObject::Cube3D(Cube3DObj::new(Point3D::new(x, y, z), s));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Cylinder" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r), Ok(h)) = (
                    cmd.args[0].trim().parse::<f64>(),
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                    cmd.args[3].trim().parse::<f64>(),
                    cmd.args[4].trim().parse::<f64>(),
                ) {
                    let obj = GeoObject::Cylinder3D(Cylinder3DObj::new(
                        Point3D::new(x, y, z),
                        Point3D::new(x, y, z + h),
                        r,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Cone" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(r), Ok(h)) = (
                    cmd.args[0].trim().parse::<f64>(),
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                    cmd.args[3].trim().parse::<f64>(),
                    cmd.args[4].trim().parse::<f64>(),
                ) {
                    let obj = GeoObject::Cone3D(Cone3DObj::new(
                        Point3D::new(x, y, z),
                        Point3D::new(x, y, z + h),
                        r,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Torus" if cmd.args.len() == 5 => {
                if let (Ok(x), Ok(y), Ok(z), Ok(rmaj), Ok(rmin)) = (
                    cmd.args[0].trim().parse::<f64>(),
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                    cmd.args[3].trim().parse::<f64>(),
                    cmd.args[4].trim().parse::<f64>(),
                ) {
                    let obj =
                        GeoObject::Torus3D(Torus3DObj::new(Point3D::new(x, y, z), rmaj, rmin));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Moebius" if cmd.args.len() == 2 => {
                if let (Ok(r), Ok(w)) = (
                    cmd.args[0].trim().parse::<f64>(),
                    cmd.args[1].trim().parse::<f64>(),
                ) {
                    let obj = GeoObject::MoebiusStrip(MoebiusStripObj::new(
                        Point3D::new(0.0, 0.0, 0.0),
                        r,
                        w,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            "Tangent" => {
                if cmd.args.len() >= 3 {
                    if let (Ok((cx, cy)), Ok(r), Ok((px, py))) = (
                        parse_point_str(&cmd.args[0]),
                        cmd.args[1].trim().parse::<f64>(),
                        parse_point_str(&cmd.args[2]),
                    ) {
                        let dx = px - cx;
                        let dy = py - cy;
                        let d = (dx * dx + dy * dy).sqrt();
                        if d > r {
                            let a = r * r / d;
                            let h = (r * r - a * a).sqrt();
                            let pm = Point2::new(cx + a * dx / d, cy + a * dy / d);
                            let perp_x = -h * dy / d;
                            let perp_y = h * dx / d;
                            let t1 = Point2::new(pm.x + perp_x, pm.y + perp_y);
                            let t2 = Point2::new(pm.x - perp_x, pm.y - perp_y);
                            document.add_object(GeoObject::Line(
                                LineObj::new(Point2::new(px, py), t1).with_label("T1"),
                            ));
                            document.add_object(GeoObject::Line(
                                LineObj::new(Point2::new(px, py), t2).with_label("T2"),
                            ));
                        }
                    }
                }
                input_text.clear();
                return None;
            }
            "Intersect" if cmd.args.len() == 2 => {
                let label_a = cmd.args[0].trim();
                let label_b = cmd.args[1].trim();
                let id_a = find_object_by_label(document, label_a);
                let id_b = find_object_by_label(document, label_b);
                if let (Some(id_a), Some(id_b)) = (id_a, id_b) {
                    let obj_a = document.get_object(id_a).cloned();
                    let obj_b = document.get_object(id_b).cloned();
                    if let (Some(obj_a), Some(obj_b)) = (obj_a, obj_b) {
                        let pts = intersect_objects(&obj_a, &obj_b);
                        match pts.len() {
                            0 => result = Some("No intersection".into()),
                            1 => {
                                let p = pts[0];
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p).with_label("I")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                result = Some(format!("Intersection at ({:.4}, {:.4})", p.x, p.y));
                            }
                            2 => {
                                let p1 = pts[0];
                                let p2 = pts[1];
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p1).with_label("I\u{2081}")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                document.add_constructed_object(
                                    GeoObject::Point(PointObj::new(p2).with_label("I\u{2082}")),
                                    "Intersect",
                                    &[id_a, id_b],
                                );
                                result = Some(format!(
                                    "Two intersections: ({:.4}, {:.4}) and ({:.4}, {:.4})",
                                    p1.x, p1.y, p2.x, p2.y
                                ));
                            }
                            _ => result = Some("Infinite intersections (coincident)".into()),
                        }
                    }
                } else {
                    result = Some("Usage: Intersect[obj1, obj2]".into());
                }
                input_text.clear();
                return result;
            }
            "PerpendicularBisector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let mx = (x1 + x2) * 0.5;
                    let my = (y1 + y2) * 0.5;
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let p1 = Point2::new(mx - dy * 5.0, my + dx * 5.0);
                    let p2 = Point2::new(mx + dy * 5.0, my - dx * 5.0);
                    document.add_object(GeoObject::Line(LineObj::new(p1, p2).with_label("B")));
                }
                input_text.clear();
                return None;
            }
            "AngleBisector" if cmd.args.len() == 3 => {
                if let (Ok((x1, y1)), Ok((xv, yv)), Ok((x2, y2))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let d1 = ((xv - x1).powi(2) + (yv - y1).powi(2)).sqrt();
                    let d2 = ((xv - x2).powi(2) + (yv - y2).powi(2)).sqrt();
                    if d1 > 0.0 && d2 > 0.0 {
                        let ux = (x1 - xv) / d1;
                        let uy = (y1 - yv) / d1;
                        let vx = (x2 - xv) / d2;
                        let vy = (y2 - yv) / d2;
                        let bx = ux + vx;
                        let by = uy + vy;
                        let b_len = (bx * bx + by * by).sqrt();
                        if b_len > 0.0 {
                            let p = Point2::new(xv + bx / b_len * 5.0, yv + by / b_len * 5.0);
                            document.add_object(GeoObject::Line(
                                LineObj::new(Point2::new(xv, yv), p).with_label("Ab"),
                            ));
                        }
                    }
                }
                input_text.clear();
                return None;
            }
            "Midpoint" if cmd.args.len() == 2 => {
                let id_a = find_object_by_label(document, cmd.args[0].trim());
                let id_b = find_object_by_label(document, cmd.args[1].trim());
                if let (Some(id_a), Some(id_b)) = (id_a, id_b) {
                    if let (Some(GeoObject::Point(a)), Some(GeoObject::Point(b))) =
                        (document.get_object(id_a), document.get_object(id_b))
                    {
                        let mx = (a.position.x + b.position.x) * 0.5;
                        let my = (a.position.y + b.position.y) * 0.5;
                        document.add_constructed_object(
                            GeoObject::Point(PointObj::new(Point2::new(mx, my)).with_label("M")),
                            "Midpoint",
                            &[id_a, id_b],
                        );
                    }
                } else if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let obj = GeoObject::Point(
                        PointObj::new(Point2::new((x1 + x2) * 0.5, (y1 + y2) * 0.5))
                            .with_label("M"),
                    );
                    document.add_object(obj);
                }
                input_text.clear();
                return None;
            }
            "Vector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let obj = GeoObject::Line(
                        LineObj::new(Point2::new(x1, y1), Point2::new(x2, y2)).with_label("v"),
                    );
                    document.add_object(obj);
                }
                input_text.clear();
                return None;
            }
            "Ray" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) =
                    (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]))
                {
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let len = (dx * dx + dy * dy).sqrt().max(0.01);
                    let far = Point2::new(x1 + dx / len * 100.0, y1 + dy / len * 100.0);
                    document.add_object(GeoObject::Line(
                        LineObj::new(Point2::new(x1, y1), far).with_label("r"),
                    ));
                }
                input_text.clear();
                return None;
            }
            "Parabola" if cmd.args.len() >= 2 => {
                if let (Ok((vx, vy)), Ok(p)) = (
                    parse_point_str(&cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                ) {
                    document.add_object(GeoObject::Parabola(ParabolaObj::new(
                        Point2::new(vx, vy),
                        p,
                    )));
                }
                input_text.clear();
                return None;
            }
            "Hyperbola" if cmd.args.len() >= 3 => {
                if let (Ok((cx, cy)), Ok(a), Ok(b)) = (
                    parse_point_str(&cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                    cmd.args[2].trim().parse::<f64>(),
                ) {
                    document.add_object(GeoObject::Hyperbola(HyperbolaObj::new(
                        Point2::new(cx, cy),
                        a,
                        b,
                    )));
                }
                input_text.clear();
                return None;
            }
            "Dilate" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok(factor), Ok((cx, cy))) = (
                    parse_point_str(&cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let nx = cx + (px - cx) * factor;
                    let ny = cy + (py - cy) * factor;
                    document.add_object(GeoObject::Point(
                        PointObj::new(Point2::new(nx, ny)).with_label("D'"),
                    ));
                }
                input_text.clear();
                return None;
            }
            "Reflect" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok((ax, ay)), Ok((bx, by))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let dx = bx - ax;
                    let dy = by - ay;
                    let len2 = dx * dx + dy * dy;
                    if len2 > 0.0 {
                        let t = ((px - ax) * dx + (py - ay) * dy) / len2;
                        let cx = ax + t * dx;
                        let cy = ay + t * dy;
                        let rx = 2.0 * cx - px;
                        let ry = 2.0 * cy - py;
                        document.add_object(GeoObject::Point(
                            PointObj::new(Point2::new(rx, ry)).with_label("R'"),
                        ));
                    }
                }
                input_text.clear();
                return None;
            }
            "Locus" if cmd.args.len() == 2 => {
                let expr = cmd.args[0].trim();
                if let Ok(range) = cmd.args[1].trim().parse::<f64>() {
                    let steps = 200;
                    let mut vertices = Vec::new();
                    for i in 0..=steps {
                        let x = -range + 2.0 * range * i as f64 / steps as f64;
                        let mut vars = HashMap::new();
                        vars.insert("x".to_string(), x);
                        if let Ok(y) = evaluate(
                            expr,
                            &vars
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect::<Vec<_>>(),
                        ) {
                            if y.is_finite() && y.abs() < 1e6 {
                                vertices.push(Point2::new(x, y));
                            }
                        }
                    }
                    if vertices.len() >= 2 {
                        let mut poly = PolygonObj::new(vertices);
                        poly.label = "L".to_string();
                        document.add_object(GeoObject::Polygon(poly));
                    }
                }
                input_text.clear();
                return None;
            }
            "FunctionInspector" if cmd.args.len() == 1 => {
                let expr = cmd.args[0].trim();
                let v = document.variables.clone();
                let f = |x: f64| {
                    let mut vars: Vec<(String, f64)> =
                        v.iter().map(|(k, val)| (k.clone(), *val)).collect();
                    vars.push(("x".to_string(), x));
                    evaluate(expr, &vars).unwrap_or(f64::NAN)
                };
                let mins = find_extrema(&f, -10.0, 10.0, false);
                let maxs = find_extrema(&f, -10.0, 10.0, true);
                let mut res = String::new();
                if let Some((mx, my)) = root_10(&f) {
                    res.push_str(&format!("Root ≈ ({}: {:.4})", mx, my));
                }
                for (mx, my) in &mins {
                    res.push_str(&format!(" Min@({:.2},{:.2})", mx, my));
                }
                for (mx, my) in &maxs {
                    res.push_str(&format!(" Max@({:.2},{:.2})", mx, my));
                }
                result = Some(if res.is_empty() {
                    "No extrema found in [-10,10]".into()
                } else {
                    res
                });
                input_text.clear();
                return result;
            }
            "Normal" if cmd.args.len() == 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let expr = format!("exp(-(x-{})^2/(2*{}^2))/({}*sqrt(2*pi))", mu, sigma, sigma);
                document.add_object(GeoObject::Function(
                    FunctionObj::new(expr).with_label(format!("N({},{})", mu, sigma)),
                ));
                result = Some(format!("Normal N({},{}) added", mu, sigma));
                input_text.clear();
                return result;
            }
            "Binomial" if cmd.args.len() == 3 => {
                let n: usize = cmd.args[0].trim().parse().unwrap_or(10);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: usize = cmd.args[2].trim().parse().unwrap_or(1);
                let comb = |n: usize, k: usize| -> f64 {
                    if k > n {
                        return 0.0;
                    }
                    let k = k.min(n - k);
                    let mut result = 1.0;
                    for i in 0..k {
                        result = result * (n - i) as f64 / (i + 1) as f64;
                    }
                    result
                };
                let prob = comb(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32);
                result = Some(format!("P(X={}) = {:.6} (Binom({},{}))", k, prob, n, p));
                input_text.clear();
                return result;
            }
            "Poisson" if cmd.args.len() == 2 => {
                let lambda: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: usize = cmd.args[1].trim().parse().unwrap_or(1);
                let mut prob = (-lambda).exp();
                for i in 1..=k {
                    prob *= lambda / i as f64;
                }
                result = Some(format!("P(X={}) = {:.6} (Poisson({}))", k, prob, lambda));
                input_text.clear();
                return result;
            }
            "Curve3D" if cmd.args.len() >= 3 => {
                let exprs = cmd.args[0].trim();
                let t_min: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let t_max: f64 = cmd.args[2].trim().parse().unwrap_or(std::f64::consts::TAU);
                let steps = 200;
                let mut pts = Vec::new();
                for i in 0..=steps {
                    let t = t_min + (t_max - t_min) * i as f64 / steps as f64;
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    let inner = exprs.trim_start_matches('(').trim_end_matches(')');
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() >= 3 {
                        let vals: Vec<f64> = parts
                            .iter()
                            .filter_map(|s| {
                                let expr = s.trim();
                                eval_function_with_vars(expr, t, &vars).ok().or_else(|| {
                                    evaluate(
                                        expr,
                                        &vars
                                            .iter()
                                            .map(|(k, v)| (k.clone(), *v))
                                            .collect::<Vec<_>>(),
                                    )
                                    .ok()
                                })
                            })
                            .collect();
                        if vals.len() >= 3 {
                            pts.push(Point3D::new(vals[0], vals[1], vals[2]));
                        }
                    }
                }
                if pts.len() >= 2 {
                    let mut segs = Vec::new();
                    for i in 1..pts.len() {
                        segs.push((pts[i - 1], pts[i]));
                    }
                    for (a, b) in &segs {
                        document.add_object(GeoObject::Segment3D(
                            Segment3DObj::new(*a, *b).with_label("C3"),
                        ));
                    }
                }
                input_text.clear();
                return None;
            }
            "SetValue" if cmd.args.len() == 2 => {
                if let Some(id) = find_object_by_label(document, &cmd.args[0]) {
                    if let Ok(val) = cmd.args[1].trim().parse::<f64>() {
                        document.set_variable(cmd.args[0].trim().to_string(), val);
                    } else if let Ok((x, y)) = parse_point_str(&cmd.args[1]) {
                        if let Some(GeoObject::Point(p)) = document.get_object_mut(id) {
                            p.position = Point2::new(x, y);
                            let order = document.propagation_order(&[id]);
                            if !order.is_empty() {
                                document.re_evaluate_constraints(&order);
                            }
                        }
                    }
                }
                input_text.clear();
                return None;
            }
            "Extrude" if cmd.args.len() >= 2 => {
                let height: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let id_opt = find_object_by_label(document, &cmd.args[0]);
                let vertices = id_opt.and_then(|id| {
                    document.get_object(id).and_then(|obj| {
                        if let GeoObject::Polygon(poly) = obj {
                            if poly.vertices.len() >= 3 {
                                Some(poly.vertices.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                });
                if let Some(verts) = vertices {
                    let base_y = 0.0;
                    let top_y = height;
                    for i in 0..verts.len() {
                        let v = verts[i];
                        let vn = verts[(i + 1) % verts.len()];
                        let b = Point3D::new(v.x, base_y, v.y);
                        let t = Point3D::new(v.x, top_y, v.y);
                        let bn = Point3D::new(vn.x, base_y, vn.y);
                        let tn = Point3D::new(vn.x, top_y, vn.y);
                        if let Some(poly_id) = id_opt {
                            let (_, c1) = document.add_constructed_object(GeoObject::Segment3D(
                                Segment3DObj::new(b, t).with_label("E"),
                            ), "Extrude", &[poly_id]);
                            document.set_constraint_param(c1, "_ext_h", height);
                            let (_, c2) = document.add_constructed_object(GeoObject::Segment3D(
                                Segment3DObj::new(b, bn).with_label("E"),
                            ), "Extrude", &[poly_id]);
                            document.set_constraint_param(c2, "_ext_h", height);
                            let (_, c3) = document.add_constructed_object(GeoObject::Segment3D(
                                Segment3DObj::new(t, tn).with_label("E"),
                            ), "Extrude", &[poly_id]);
                            document.set_constraint_param(c3, "_ext_h", height);
                        }
                    }
                } else {
                    result = Some("Extrude only supports Polygons with 3+ vertices".into());
                }
                input_text.clear();
                return result;
            }
            "Script" if !cmd.args.is_empty() => {
                let commands: Vec<String> = cmd.args[0]
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut output = String::new();
                for c in &commands {
                    let mut temp = c.clone();
                    if let Some(res) = process_input(document, &mut temp) {
                        output.push_str(&res);
                        output.push('\n');
                    }
                }
                result = if output.is_empty() {
                    Some("Script executed".into())
                } else {
                    Some(output)
                };
                input_text.clear();
                return result;
            }
            "Simplify" if !cmd.args.is_empty() => {
                let expr = cmd.args[0].trim();
                let vars: Vec<(String, f64)> = document
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                match evaluate(expr, &vars) {
                    Ok(val) => result = Some(format!("{} ≈ {}", expr, val)),
                    Err(e) => result = Some(format!("Simplify error: {}", e)),
                }
                input_text.clear();
                return result;
            }
            "Lorenz" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![10.0, 28.0, 8.0 / 3.0]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                if params.is_empty() {
                    input_text.clear();
                    return Some("Error: Invalid parameters for Lorenz. Use: Lorenz[sigma, rho, beta] or Lorenz[sigma=10, rho=28, beta=8/3]".into());
                }
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("lorenz", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Lorenz attractor created".into());
            }
            "Rossler" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.2, 0.2, 5.7]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("rossler", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Rössler attractor created".into());
            }
            "Thomas" | "Butterfly" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.208186]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("thomas", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Thomas butterfly attractor created".into());
            }
            "Aizawa" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![0.95, 0.7, 0.6, 3.5, 0.25, 0.1]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("aizawa", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Aizawa attractor created".into());
            }
            "Chen" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![35.0, 3.0, 28.0]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chen", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Chen attractor created".into());
            }
            "Halvorsen" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![1.89]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("halvorsen", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Halvorsen attractor created".into());
            }
            "Dadras" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![3.0, 2.7, 1.7, 2.0, 9.0]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("dadras", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Dadras attractor created".into());
            }
            "Chua" => {
                let params = if cmd.args.is_empty() || cmd.args[0].trim().is_empty() {
                    vec![15.6, 28.0, -1.143, -0.714]
                } else {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                };
                let obj = GeoObject::Attractor3D(Attractor3DObj::new("chua", params));
                document.add_object(obj);
                input_text.clear();
                return Some("Chua attractor created".into());
            }
            "Mandelbrot" => {
                let max_iter = cmd
                    .args
                    .first()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(256);
                let obj = GeoObject::Fractal2D(Fractal2DObj::mandelbrot().with_max_iter(max_iter));
                document.add_object(obj);
                input_text.clear();
                return Some("Mandelbrot fractal created".into());
            }
            "Julia" if cmd.args.len() >= 2 => {
                let cr: f64 = cmd.args[0].trim().parse().unwrap_or(-0.70176);
                let ci: f64 = cmd.args[1].trim().parse().unwrap_or(-0.3842);
                let max_iter = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(256);
                let obj = GeoObject::Fractal2D(Fractal2DObj::julia(cr, ci).with_max_iter(max_iter));
                document.add_object(obj);
                input_text.clear();
                return Some(format!("Julia set c={cr}+{ci}i created"));
            }
            "BurningShip" => {
                let obj = GeoObject::Fractal2D(Fractal2DObj::burning_ship());
                document.add_object(obj);
                input_text.clear();
                return Some("Burning Ship fractal created".into());
            }
            "Hypercube" => {
                let angles = if cmd.args.len() >= 3 {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                } else {
                    vec![0.3, 0.5, 0.7]
                };
                let obj =
                    GeoObject::HyperSurface4D(HyperSurface4DObj::hypercube().with_rotation(angles));
                document.add_object(obj);
                input_text.clear();
                return Some("Hipercubo 4D creado (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "Hypersphere" => {
                let angles = if cmd.args.len() >= 3 {
                    cmd.args
                        .iter()
                        .filter_map(|s| s.trim().parse().ok())
                        .collect()
                } else {
                    vec![0.3, 0.5, 0.7]
                };
                let obj = GeoObject::HyperSurface4D(
                    HyperSurface4DObj::hypersphere().with_rotation(angles),
                );
                document.add_object(obj);
                input_text.clear();
                return Some("Hiperesfera 4D creada (escala=3.0). Botón derecho para orbitar, scroll para zoom.".into());
            }
            "VectorField3D" if cmd.args.len() >= 3 => {
                let obj = GeoObject::VectorField3D(VectorField3DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                    cmd.args[2].trim(),
                ));
                document.add_object(obj);
                input_text.clear();
                return Some("3D Vector Field created".into());
            }
            "Histogram" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                let bins = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(10);
                if !data.is_empty() {
                    let obj = GeoObject::Histogram(HistogramObj::new(data, bins));
                    document.add_object(obj);
                    input_text.clear();
                    return Some("Histogram created".into());
                }
            }
            "ScatterPlot" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if !xs.is_empty() && xs.len() == ys.len() {
                    let obj = GeoObject::ScatterPlot(ScatterPlotObj::new(xs, ys));
                    document.add_object(obj);
                    input_text.clear();
                    return Some("Scatter plot created".into());
                }
            }
            "BoxPlot" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if !data.is_empty() {
                    let obj = GeoObject::BoxPlot(BoxPlotObj::new(data));
                    document.add_object(obj);
                    input_text.clear();
                    return Some("Box plot created".into());
                }
            }
            "LinearRegression" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if !xs.is_empty() && xs.len() == ys.len() {
                    if let Some((slope, intercept, r2)) = statistics::linear_regression(&xs, &ys) {
                        let obj = GeoObject::RegressionLine(RegressionLineObj::linear(
                            xs, ys, slope, intercept, r2,
                        ));
                        document.add_object(obj);
                        input_text.clear();
                        return Some(format!(
                            "y = {:.4}x + {:.4}, R²={:.4}",
                            slope, intercept, r2
                        ));
                    }
                }
            }
            "Mean" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(m) = statistics::mean(&data) {
                    input_text.clear();
                    return Some(format!("Mean = {:.6}", m));
                }
            }
            "Median" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(m) = statistics::median(&data) {
                    input_text.clear();
                    return Some(format!("Median = {:.6}", m));
                }
            }
            "StdDev" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                if let Some(s) = statistics::std_dev(&data) {
                    input_text.clear();
                    return Some(format!("StdDev = {:.6}", s));
                }
            }
            "Correlation" if cmd.args.len() >= 2 => {
                let xs = parse_brace_list(&cmd.args[0]);
                let ys = parse_brace_list(&cmd.args[1]);
                if let Some(r) = statistics::pearson_correlation(&xs, &ys) {
                    input_text.clear();
                    return Some(format!("r = {:.6}", r));
                }
            }
            "Determinant" if !cmd.args.is_empty() => {
                if let Some(m) = parse_matrix_arg(&cmd.args[0]) {
                    if let Some(det) = m.determinant() {
                        input_text.clear();
                        return Some(format!("det = {:.6}", det));
                    }
                }
            }
            "Inverse" if !cmd.args.is_empty() => {
                if let Some(m) = parse_matrix_arg(&cmd.args[0]) {
                    if let Some(inv) = m.inverse() {
                        input_text.clear();
                        return Some(format!("Inverse:\n{}", inv));
                    }
                }
            }
            "Taylor" if cmd.args.len() >= 2 => {
                let expr = cmd.args[0].trim();
                let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
                let center = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let order = cmd
                    .args
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(5);
                if let Some(series) = taylor_series(expr, var, center, order) {
                    input_text.clear();
                    return Some(format!("Taylor: {}", series));
                }
            }
            "Cardioid" if !cmd.args.is_empty() => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let steps = 200;
                let points = grafito_geometry::special_curves::cardioid(a, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Cardioid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!("Cardioid(a={}) created", a));
            }
            "Rose" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let n: i32 = cmd.args[1].trim().parse().unwrap_or(3);
                let d: i32 = cmd.args[2].trim().parse().unwrap_or(1);
                let steps = 400;
                let points = grafito_geometry::special_curves::rose(a, n, d, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("Rose({}/{})", n, d);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!("Rose(a={}, n={}, d={}) created", a, n, d));
            }
            "ArchimedeanSpiral" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(0.1);
                let max_theta: f64 = cmd.args[2].trim().parse().unwrap_or(20.0);
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::archimedean_spiral(a, b, max_theta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Spiral".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!(
                    "Archimedean Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "LogarithmicSpiral" if cmd.args.len() >= 3 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(0.1);
                let max_theta: f64 = cmd.args[2].trim().parse().unwrap_or(10.0);
                let steps = 300;
                let points =
                    grafito_geometry::special_curves::logarithmic_spiral(a, b, max_theta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "LogSpiral".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!(
                    "Logarithmic Spiral(a={}, b={}, θ={}) created",
                    a, b, max_theta
                ));
            }
            "Lissajous" if cmd.args.len() >= 5 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let freq_x: f64 = cmd.args[2].trim().parse().unwrap_or(3.0);
                let freq_y: f64 = cmd.args[3].trim().parse().unwrap_or(2.0);
                let delta: f64 = cmd.args[4].trim().parse().unwrap_or(0.0);
                let steps = 400;
                let points =
                    grafito_geometry::special_curves::lissajous(a, b, freq_x, freq_y, delta, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Lissajous".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!(
                    "Lissajous(a={}, b={}, fx={}, fy={}, δ={}) created",
                    a, b, freq_x, freq_y, delta
                ));
            }
            "Epicycloid" if cmd.args.len() >= 2 => {
                let r: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: f64 = cmd.args[1].trim().parse().unwrap_or(3.0);
                let steps = 400;
                let points = grafito_geometry::special_curves::epicycloid(r, k, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Epicycloid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!("Epicycloid(r={}, k={}) created", r, k));
            }
            "Hypocycloid" if cmd.args.len() >= 2 => {
                let r: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: f64 = cmd.args[1].trim().parse().unwrap_or(4.0);
                let steps = 400;
                let points = grafito_geometry::special_curves::hypocycloid(r, k, steps);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = "Hypocycloid".to_string();
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!("Hypocycloid(r={}, k={}) created", r, k));
            }
            "ODE" if cmd.args.len() >= 4 => {
                let expr = cmd.args[0].trim();
                let t0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let y0: f64 = cmd.args[2].trim().parse().unwrap_or(1.0);
                let t_end: f64 = cmd.args[3].trim().parse().unwrap_or(10.0);
                let steps: usize = cmd
                    .args
                    .get(4)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let method = cmd
                    .args
                    .get(5)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());

                let f = |t: f64, y: f64| -> f64 {
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    vars.insert("y".to_string(), y);
                    evaluate(
                        expr,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0)
                };

                let solution = if method == "euler" {
                    grafito_geometry::ode::euler(f, t0, y0, t_end, steps)
                } else {
                    grafito_geometry::ode::runge_kutta_4(f, t0, y0, t_end, steps)
                };

                let points = grafito_geometry::ode::solution_to_points(&solution);
                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("ODE({})", method);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!(
                    "ODE solved with {} method ({} steps)",
                    method, steps
                ));
            }
            "ODESystem" if cmd.args.len() >= 5 => {
                let expr1 = cmd.args[0].trim();
                let expr2 = cmd.args[1].trim();
                let t0: f64 = cmd.args[2].trim().parse().unwrap_or(0.0);
                let y0_1: f64 = cmd.args[3].trim().parse().unwrap_or(1.0);
                let y0_2: f64 = cmd.args[4].trim().parse().unwrap_or(0.0);
                let t_end: f64 = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(10.0);
                let steps: usize = cmd
                    .args
                    .get(6)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let method = cmd
                    .args
                    .get(7)
                    .map(|s| s.trim().to_lowercase())
                    .unwrap_or("rk4".to_string());

                let f = |_t: f64, state: &[f64]| -> Vec<f64> {
                    let mut vars = document.variables.clone();
                    vars.insert("y1".to_string(), state[0]);
                    vars.insert("y2".to_string(), state[1]);
                    let dy1 = evaluate(
                        expr1,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0);
                    let dy2 = evaluate(
                        expr2,
                        &vars
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or(0.0);
                    vec![dy1, dy2]
                };

                let solution = if method == "euler" {
                    grafito_geometry::ode::euler_system(f, t0, vec![y0_1, y0_2], t_end, steps)
                } else {
                    grafito_geometry::ode::runge_kutta_4_system(
                        f,
                        t0,
                        vec![y0_1, y0_2],
                        t_end,
                        steps,
                    )
                };

                // Plot y1 vs y2 (phase portrait)
                let points: Vec<Point2> = solution
                    .iter()
                    .map(|(_, state)| Point2::new(state[0], state[1]))
                    .collect();

                if points.len() >= 3 {
                    let mut poly = PolygonObj::new(points);
                    poly.label = format!("Phase({})", method);
                    document.add_object(GeoObject::Polygon(poly));
                }
                input_text.clear();
                return Some(format!(
                    "ODE system solved with {} method ({} steps)",
                    method, steps
                ));
            }
            "Gamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::gamma(x);
                input_text.clear();
                return Some(format!("Γ({}) = {:.6}", x, result));
            }
            "LnGamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::ln_gamma(x);
                input_text.clear();
                return Some(format!("ln(Γ({})) = {:.6}", x, result));
            }
            "Beta" if cmd.args.len() >= 2 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::beta(a, b);
                input_text.clear();
                return Some(format!("B({}, {}) = {:.6}", a, b, result));
            }
            "BesselJ" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_j(n, x);
                input_text.clear();
                return Some(format!("J_{}({}) = {:.6}", n, x, result));
            }
            "BesselY" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_y(n, x);
                input_text.clear();
                return Some(format!("Y_{}({}) = {:.6}", n, x, result));
            }
            "BesselI" if cmd.args.len() >= 2 => {
                let n: i32 = cmd.args[0].trim().parse().unwrap_or(0);
                let x: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::bessel_i(n, x);
                input_text.clear();
                return Some(format!("I_{}({}) = {:.6}", n, x, result));
            }
            "Erf" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let result = grafito_geometry::special_functions::erf(x);
                input_text.clear();
                return Some(format!("erf({}) = {:.6}", x, result));
            }
            "Erfc" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let result = grafito_geometry::special_functions::erfc(x);
                input_text.clear();
                return Some(format!("erfc({}) = {:.6}", x, result));
            }
            "Digamma" if !cmd.args.is_empty() => {
                let x: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let result = grafito_geometry::special_functions::digamma(x);
                input_text.clear();
                return Some(format!("ψ({}) = {:.6}", x, result));
            }
            "Uniform" if cmd.args.len() >= 2 => {
                let a: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.5);
                let pdf = grafito_geometry::statistics::uniform_pdf(x, a, b);
                let cdf = grafito_geometry::statistics::uniform_cdf(x, a, b);
                input_text.clear();
                return Some(format!(
                    "U({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    a, b, x, pdf, x, cdf
                ));
            }
            "GammaDist" if cmd.args.len() >= 2 => {
                let alpha: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let beta: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let pdf = grafito_geometry::statistics::gamma_pdf(x, alpha, beta);
                input_text.clear();
                return Some(format!(
                    "Gamma({},{}): PDF({}) = {:.6}",
                    alpha, beta, x, pdf
                ));
            }
            "BetaDist" if cmd.args.len() >= 2 => {
                let alpha: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let beta: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.5);
                let pdf = grafito_geometry::statistics::beta_pdf(x, alpha, beta);
                input_text.clear();
                return Some(format!("Beta({},{}): PDF({}) = {:.6}", alpha, beta, x, pdf));
            }
            "Cauchy" if cmd.args.len() >= 2 => {
                let x0: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let gamma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let pdf = grafito_geometry::statistics::cauchy_pdf(x, x0, gamma);
                let cdf = grafito_geometry::statistics::cauchy_cdf(x, x0, gamma);
                input_text.clear();
                return Some(format!(
                    "Cauchy({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    x0, gamma, x, pdf, x, cdf
                ));
            }
            "Pareto" if cmd.args.len() >= 2 => {
                let xm: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let alpha: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(2.0);
                let pdf = grafito_geometry::statistics::pareto_pdf(x, xm, alpha);
                let cdf = grafito_geometry::statistics::pareto_cdf(x, xm, alpha);
                input_text.clear();
                return Some(format!(
                    "Pareto({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    xm, alpha, x, pdf, x, cdf
                ));
            }
            "Rayleigh" if !cmd.args.is_empty() => {
                let sigma: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                let pdf = grafito_geometry::statistics::rayleigh_pdf(x, sigma);
                let cdf = grafito_geometry::statistics::rayleigh_cdf(x, sigma);
                input_text.clear();
                return Some(format!(
                    "Rayleigh({}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    sigma, x, pdf, x, cdf
                ));
            }
            "Laplace" if cmd.args.len() >= 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let b: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let x: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.0);
                let pdf = grafito_geometry::statistics::laplace_pdf(x, mu, b);
                let cdf = grafito_geometry::statistics::laplace_cdf(x, mu, b);
                input_text.clear();
                return Some(format!(
                    "Laplace({},{}): PDF({}) = {:.6}, CDF({}) = {:.6}",
                    mu, b, x, pdf, x, cdf
                ));
            }
            "NegBinomial" if cmd.args.len() >= 2 => {
                let r: u32 = cmd.args[0].trim().parse().unwrap_or(1);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: u32 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
                let pmf = grafito_geometry::statistics::negative_binomial_pmf(r, p, k);
                let cdf = grafito_geometry::statistics::negative_binomial_cdf(r, p, k);
                input_text.clear();
                return Some(format!(
                    "NegBin({},{}): PMF({}) = {:.6}, CDF({}) = {:.6}",
                    r, p, k, pmf, k, cdf
                ));
            }
            "TTest" if cmd.args.len() >= 2 => {
                let data = parse_brace_list(&cmd.args[0]);
                let mu0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_one_sample(&data, mu0)
                {
                    input_text.clear();
                    return Some(format!("t-test: t = {:.4}, p = {:.6}", t_stat, p_value));
                }
            }
            "TTest2" if cmd.args.len() >= 2 => {
                let data1 = parse_brace_list(&cmd.args[0]);
                let data2 = parse_brace_list(&cmd.args[1]);
                if let Some((t_stat, p_value)) =
                    grafito_geometry::statistics::t_test_two_sample(&data1, &data2)
                {
                    input_text.clear();
                    return Some(format!(
                        "t-test (2 samples): t = {:.4}, p = {:.6}",
                        t_stat, p_value
                    ));
                }
            }
            "ZTest" if cmd.args.len() >= 3 => {
                let data = parse_brace_list(&cmd.args[0]);
                let mu0: f64 = cmd.args[1].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[2].trim().parse().unwrap_or(1.0);
                if let Some((z_stat, p_value)) =
                    grafito_geometry::statistics::z_test_one_sample(&data, mu0, sigma)
                {
                    input_text.clear();
                    return Some(format!("z-test: z = {:.4}, p = {:.6}", z_stat, p_value));
                }
            }
            "ChiSqTest" if cmd.args.len() >= 2 => {
                let observed = parse_brace_list(&cmd.args[0]);
                let expected = parse_brace_list(&cmd.args[1]);
                if let Some((chi2, p_value)) =
                    grafito_geometry::statistics::chi_squared_test(&observed, &expected)
                {
                    input_text.clear();
                    return Some(format!("χ²-test: χ² = {:.4}, p = {:.6}", chi2, p_value));
                }
            }
            "ANOVA" if cmd.args.len() >= 2 => {
                let mut groups: Vec<Vec<f64>> = Vec::new();
                for arg in &cmd.args {
                    groups.push(parse_brace_list(arg));
                }
                let group_refs: Vec<&[f64]> = groups.iter().map(|g| g.as_slice()).collect();
                if let Some((f_stat, p_value)) =
                    grafito_geometry::statistics::anova_one_way(&group_refs)
                {
                    input_text.clear();
                    return Some(format!("ANOVA: F = {:.4}, p = {:.6}", f_stat, p_value));
                }
            }
            "CIMean" if !cmd.args.is_empty() => {
                let data = parse_brace_list(&cmd.args[0]);
                let confidence: f64 = cmd
                    .args
                    .get(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.95);
                if let Some((lower, mean, upper)) =
                    grafito_geometry::statistics::confidence_interval_mean(&data, confidence)
                {
                    input_text.clear();
                    return Some(format!(
                        "CI ({:.0}%): [{:.4}, {:.4}, {:.4}]",
                        confidence * 100.0,
                        lower,
                        mean,
                        upper
                    ));
                }
            }
            "CIProportion" if cmd.args.len() >= 2 => {
                let successes: u32 = cmd.args[0].trim().parse().unwrap_or(0);
                let n: u32 = cmd.args[1].trim().parse().unwrap_or(1);
                let confidence: f64 = cmd
                    .args
                    .get(2)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0.95);
                if let Some((lower, p_hat, upper)) =
                    grafito_geometry::statistics::confidence_interval_proportion(
                        successes, n, confidence,
                    )
                {
                    input_text.clear();
                    return Some(format!(
                        "CI ({:.0}%): [{:.4}, {:.4}, {:.4}]",
                        confidence * 100.0,
                        lower,
                        p_hat,
                        upper
                    ));
                }
            }
            "ComplexGrid" if cmd.args.len() >= 5 => {
                let x_min = cmd.args[1].trim().parse().unwrap_or(-5.0);
                let x_max = cmd.args[2].trim().parse().unwrap_or(5.0);
                let y_min = cmd.args[3].trim().parse().unwrap_or(-5.0);
                let y_max = cmd.args[4].trim().parse().unwrap_or(5.0);
                let density: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(10);
                let mut cg = ComplexGridObj::new(
                    &format!("{}->z", cmd.args[0].trim()),
                    x_min,
                    x_max,
                    y_min,
                    y_max,
                );
                cg.density = density;
                // Support expr = "f(z)" syntax: strip "f(z)=" prefix
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                cg.expr = expr.to_string();
                document.add_object(GeoObject::ComplexGrid(cg));
                input_text.clear();
                return Some("Complex grid created — scroll/zoom to explore".into());
            }
            "DomainColoring" if cmd.args.len() >= 5 => {
                let x_min = cmd.args[1].trim().parse().unwrap_or(-3.0);
                let x_max = cmd.args[2].trim().parse().unwrap_or(3.0);
                let y_min = cmd.args[3].trim().parse().unwrap_or(-3.0);
                let y_max = cmd.args[4].trim().parse().unwrap_or(3.0);
                let res: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(200);
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(z)=").unwrap_or(expr);
                let expr = expr.strip_prefix("w=").unwrap_or(expr);
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_domain_coloring();
                let mut cg2 = cg;
                cg2.density = res;
                document.add_object(GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return Some(format!("Domain coloring ({}x{}) created", res, res));
            }
            "HeatMap" if cmd.args.len() >= 5 => {
                let x_min = cmd.args[1].trim().parse().unwrap_or(-5.0);
                let x_max = cmd.args[2].trim().parse().unwrap_or(5.0);
                let y_min = cmd.args[3].trim().parse().unwrap_or(-5.0);
                let y_max = cmd.args[4].trim().parse().unwrap_or(5.0);
                let res: usize = cmd
                    .args
                    .get(5)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(150);
                let expr = cmd.args[0].trim();
                let expr = expr.strip_prefix("f(x,y)=").unwrap_or(expr);
                let expr = expr.strip_prefix("z=").unwrap_or(expr);
                let cg = ComplexGridObj::new(expr, x_min, x_max, y_min, y_max).as_heat_map();
                let mut cg2 = cg;
                cg2.density = res;
                document.add_object(GeoObject::ComplexGrid(cg2));
                input_text.clear();
                return Some(format!("Heat map ({}x{}) created", res, res));
            }
            "PolarCurve" if cmd.args.len() >= 3 => {
                let expr = cmd.args[0].trim();
                let t_min = cmd.args[1].trim().parse().unwrap_or(0.0);
                let t_max = cmd.args[2].trim().parse().unwrap_or(std::f64::consts::TAU);
                let obj = GeoObject::PolarCurve(PolarCurveObj::new(expr, t_min, t_max));
                document.add_object(obj);
                input_text.clear();
                return Some(format!("Polar curve r = {} [{}..{}]", expr, t_min, t_max));
            }
            "ParametricCurve2D" if cmd.args.len() >= 4 => {
                let expr_x = cmd.args[0].trim();
                let expr_y = cmd.args[1].trim();
                let t_min = cmd.args[2].trim().parse().unwrap_or(0.0);
                let t_max = cmd.args[3].trim().parse().unwrap_or(std::f64::consts::TAU);
                let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                    expr_x, expr_y, t_min, t_max,
                ));
                document.add_object(obj);
                input_text.clear();
                return Some("Parametric curve created".into());
            }
            "Piecewise" if cmd.args.len() >= 3 => {
                let mut expr = format!("piecewise({}", cmd.args[0].trim());
                for a in &cmd.args[1..] {
                    expr.push_str(", ");
                    expr.push_str(a.trim());
                }
                expr.push(')');
                let label = next_function_label(document);
                document.add_object(GeoObject::Function(
                    FunctionObj::new(&expr).with_label(&label),
                ));
                input_text.clear();
                return Some(format!("Piecewise function → {}", label));
            }
            "VectorField2D" if cmd.args.len() >= 2 => {
                let obj = GeoObject::VectorField2D(VectorField2DObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                ));
                document.add_object(obj);
                input_text.clear();
                return Some("Vector field 2D created — streamlines auto-rendered".into());
            }
            "PhasePortrait" if cmd.args.len() >= 2 => {
                let mut pp = PhasePortraitObj::new(
                    cmd.args[0].trim(),
                    cmd.args[1].trim(),
                    -10.0, 10.0, -10.0, 10.0,
                );
                pp.density = 25;
                pp.color = Color::new(0.2, 0.2, 0.8, 1.0);
                document.add_object(GeoObject::PhasePortrait(pp));
                input_text.clear();
                return Some("Phase portrait created".into());
            }
            "Contour" if cmd.args.len() >= 6 => {
                let expr = cmd.args[0].trim();
                let _x_min = cmd.args[1].trim().parse().unwrap_or(-5.0);
                let _x_max = cmd.args[2].trim().parse().unwrap_or(5.0);
                let _y_min = cmd.args[3].trim().parse().unwrap_or(-5.0);
                let _y_max = cmd.args[4].trim().parse().unwrap_or(5.0);
                let levels: Vec<f64> = cmd.args[5..]
                    .iter()
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if levels.is_empty() {
                    return None;
                }
                // Split LHS/RHS on '='
                let (lhs, rhs) = if let Some(pos) = expr.find('=') {
                    (
                        expr[..pos].trim().to_string(),
                        expr[pos + 1..].trim().to_string(),
                    )
                } else {
                    (expr.to_string(), "0".to_string())
                };
                let mut obj = ImplicitCurveObj::new(&lhs, &rhs, RelationOperator::Eq);
                obj.contour_levels = Some(levels);
                document.add_object(GeoObject::ImplicitCurve(obj));
                input_text.clear();
                return Some("Contour curves created".into());
            }
            _ => {}
        }
        result = execute_cas_command(document, &cmd);
        input_text.clear();
        return result;
    }

    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();

    if let Some((name, rest)) = text.split_once('=') {
        let name = name.trim();
        let rest = rest.trim();
        if name.chars().all(|c| c.is_alphabetic()) && name.len() == 1 {
            if let Ok(val) = rest.parse::<f64>() {
                document.set_variable(name.to_string(), val);
                input_text.clear();
                return None;
            }
        }
        if is_function_lhs(name)
            && (rest.contains('x')
                || rest
                    .chars()
                    .all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c)))
        {
            if let Some(id) = find_object_by_label(document, name) {
                document.remove_object(id);
            }
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(name));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new(x, y)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                    parts[2].parse::<f64>(),
                ) {
                    let obj =
                        GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }

        if name == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(&label));
            document.add_object(obj);
            input_text.clear();
            return None;
        }

        // Polar curve: r = f(theta) or r(theta) = f(theta)
        if name == "r" || name == "r(θ)" || name == "r(t)" || name == "r(theta)" {
            let t_min = 0.0;
            let t_max = 2.0 * std::f64::consts::PI;
            let obj = GeoObject::PolarCurve(PolarCurveObj::new(rest, t_min, t_max));
            document.add_object(obj);
            input_text.clear();
            return None;
        }

        // Parametric 2D: (x(t), y(t)) = (f(t), g(t))
        if let Some(inner) = name.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let name_parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if name_parts.len() == 2
                && name_parts[0].ends_with("(t)")
                && name_parts[1].ends_with("(t)")
            {
                let rest_clean = rest.trim_matches(|c| c == '(' || c == ')');
                let rest_parts: Vec<&str> = rest_clean.split(',').map(|s| s.trim()).collect();
                if rest_parts.len() == 2 {
                    let obj = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                        rest_parts[0],
                        rest_parts[1],
                        0.0,
                        std::f64::consts::TAU,
                    ));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }

        if rest == "y" {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(name).with_label(&label));
            document.add_object(obj);
            input_text.clear();
            return None;
        }

        // Contour: f(x,y) = [c1, c2, c3] → multi-level implicit
        if rest.starts_with('[') && rest.ends_with(']') {
            if let Ok(levels) = rest[1..rest.len() - 1]
                .split(',')
                .map(|s| s.trim().parse::<f64>())
                .collect::<Result<Vec<f64>, _>>()
            {
                if levels.len() >= 2 {
                    let mut obj = ImplicitCurveObj::new(name, "0", RelationOperator::Eq);
                    obj.contour_levels = Some(levels);
                    document.add_object(GeoObject::ImplicitCurve(obj));
                    input_text.clear();
                    return None;
                }
            }
        }

        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(name, rest, RelationOperator::Eq));
        document.add_object(obj);
        input_text.clear();
        return None;
    } else if let Some((lhs, rhs)) = text.split_once("<=") {
        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            lhs.trim(),
            rhs.trim(),
            RelationOperator::LessEq,
        ));
        document.add_object(obj);
        input_text.clear();
        return None;
    } else if let Some((lhs, rhs)) = text.split_once(">=") {
        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            lhs.trim(),
            rhs.trim(),
            RelationOperator::GreaterEq,
        ));
        document.add_object(obj);
        input_text.clear();
        return None;
    } else if let Some((lhs, rhs)) = text.split_once('<') {
        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            lhs.trim(),
            rhs.trim(),
            RelationOperator::Less,
        ));
        document.add_object(obj);
        input_text.clear();
        return None;
    } else if let Some((lhs, rhs)) = text.split_once('>') {
        let obj = GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            lhs.trim(),
            rhs.trim(),
            RelationOperator::Greater,
        ));
        document.add_object(obj);
        input_text.clear();
        return None;
    } else {
        if contains_var(text, 'x') {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(text).with_label(label));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[0].parse::<f64>(),
                    parts[1].parse::<f64>(),
                    parts[2].parse::<f64>(),
                ) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new(x, y)));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
    }
    input_text.clear();
    result
}

fn intersect_objects(obj_a: &GeoObject, obj_b: &GeoObject) -> Vec<Point2> {
    use grafito_geometry::intersections::{self, IntersectionResult};

    match (obj_a, obj_b) {
        (GeoObject::Line(a), GeoObject::Line(b)) => {
            match intersections::line_line(a.start, a.end, b.start, b.end) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                _ => vec![],
            }
        }
        (GeoObject::Line(l), GeoObject::Circle(c))
        | (GeoObject::Circle(c), GeoObject::Line(l)) => {
            match intersections::line_circle(l.start, l.end, c.center, c.radius) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                _ => vec![],
            }
        }
        (GeoObject::Circle(c1), GeoObject::Circle(c2)) => {
            match intersections::circle_circle(c1.center, c1.radius, c2.center, c2.radius) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                IntersectionResult::Infinite => vec![],
                IntersectionResult::None => vec![],
            }
        }
        (GeoObject::Function(f), GeoObject::Line(l)) => {
            let slope = if (l.end.x - l.start.x).abs() < 1e-12 {
                0.0
            } else {
                (l.end.y - l.start.y) / (l.end.x - l.start.x)
            };
            let intercept = l.start.y - slope * l.start.x;
            let x_min = f.domain_min.unwrap_or(-10.0);
            let x_max = f.domain_max.unwrap_or(10.0);
            intersections::function_line(&f.expr, slope, intercept, x_min, x_max)
        }
        (GeoObject::Line(l), GeoObject::Function(f)) => {
            let slope = if (l.end.x - l.start.x).abs() < 1e-12 {
                0.0
            } else {
                (l.end.y - l.start.y) / (l.end.x - l.start.x)
            };
            let intercept = l.start.y - slope * l.start.x;
            let x_min = f.domain_min.unwrap_or(-10.0);
            let x_max = f.domain_max.unwrap_or(10.0);
            intersections::function_line(&f.expr, slope, intercept, x_min, x_max)
        }
        (GeoObject::Function(f1), GeoObject::Function(f2)) => {
            let x_min = f1
                .domain_min
                .unwrap_or(-10.0)
                .max(f2.domain_min.unwrap_or(-10.0));
            let x_max = f1
                .domain_max
                .unwrap_or(10.0)
                .min(f2.domain_max.unwrap_or(10.0));
            intersections::function_function(&f1.expr, &f2.expr, x_min, x_max)
        }
        (GeoObject::Segment3D(a), GeoObject::Segment3D(b)) => {
            match intersections::segment_segment(
                Point2::new(a.a.x, a.a.y),
                Point2::new(a.b.x, a.b.y),
                Point2::new(b.a.x, b.a.y),
                Point2::new(b.b.x, b.b.y),
            ) {
                IntersectionResult::One(p) => vec![p],
                IntersectionResult::Two(p1, p2) => vec![p1, p2],
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

#[derive(Debug)]
pub struct CasCmd {
    pub command: String,
    pub args: Vec<String>,
}

pub fn parse_cas_command(text: &str) -> Option<CasCmd> {
    let text = text.trim();
    if let Some(open) = text.find('[') {
        let close = text.rfind(']')?;
        let command = text[..open].trim().to_string();
        let inside = &text[open + 1..close];
        let args: Vec<String> = split_args(inside)
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        if command.is_empty() {
            return None;
        }
        let normalized = match command.to_lowercase().as_str() {
            "derivative" | "derivada" | "deriv" | "diff" => "Derivative",
            "integral" | "integrar" | "int" => "Integral",
            "solve" | "nsolve" | "resolver" => "Solve",
            "limit" | "limite" | "lim" => "Limit",
            "factor" | "factorizar" => "Factor",
            "expand" | "expandir" => "Expand",
            "simplify" | "simplificar" => "Simplify",
            "lorenz" => "Lorenz",
            "rossler" | "rössler" => "Rossler",
            "thomas" | "butterfly" => "Thomas",
            "aizawa" => "Aizawa",
            "chen" => "Chen",
            "halvorsen" => "Halvorsen",
            "dadras" => "Dadras",
            "chua" => "Chua",
            "mandelbrot" => "Mandelbrot",
            "julia" => "Julia",
            "burningship" | "burning_ship" => "BurningShip",
            "hypercube" | "tesseract" => "Hypercube",
            "hypersphere" => "Hypersphere",
            "vectorfield3d" | "vectorfield" => "VectorField3D",
            "histogram" | "histograma" => "Histogram",
            "scatterplot" | "scatter" => "ScatterPlot",
            "boxplot" => "BoxPlot",
            "linearregression" | "regression" | "regresion" => "LinearRegression",
            "mean" | "media" => "Mean",
            "median" | "mediana" => "Median",
            "stddev" | "desviacion" => "StdDev",
            "correlation" | "correlacion" => "Correlation",
            "determinant" | "det" => "Determinant",
            "inverse" | "inversa" => "Inverse",
            "taylor" => "Taylor",
            "complexgrid" | "complex_grid" | "cgrid" => "ComplexGrid",
            "domaincoloring" | "domain_coloring" | "dcolor" => "DomainColoring",
            "heatmap" | "heat_map" | "hmap" => "HeatMap",
            "polarcurve" | "polar_curve" | "polar" => "PolarCurve",
            "parametriccurve2d" | "parametric_curve_2d" | "param2d" => "ParametricCurve2D",
            "vectorfield2d" | "vector_field_2d" | "vf2d" => "VectorField2D",
            "phaseportrait" | "phase_portrait" | "phase" => "PhasePortrait",
            "contour" | "contourlines" | "contour_lines" => "Contour",
            "piecewise" | "pw" => "Piecewise",
            _ => {
                if args.is_empty() {
                    return None;
                }
                return Some(CasCmd { command, args });
            }
        };
        Some(CasCmd {
            command: normalized.to_string(),
            args,
        })
    } else {
        let cmd_lower = text.to_lowercase();
        let bare_commands = [
            "lorenz",
            "rossler",
            "thomas",
            "butterfly",
            "aizawa",
            "chen",
            "halvorsen",
            "dadras",
            "chua",
            "mandelbrot",
            "burningship",
            "hypercube",
            "hypersphere",
        ];
        for &cmd in &bare_commands {
            if cmd_lower == cmd {
                let normalized = match cmd {
                    "burningship" => "BurningShip".to_string(),
                    "butterfly" => "Thomas".to_string(),
                    "lorenz" => "Lorenz".to_string(),
                    "rossler" => "Rossler".to_string(),
                    "thomas" => "Thomas".to_string(),
                    "aizawa" => "Aizawa".to_string(),
                    "chen" => "Chen".to_string(),
                    "halvorsen" => "Halvorsen".to_string(),
                    "dadras" => "Dadras".to_string(),
                    "chua" => "Chua".to_string(),
                    "mandelbrot" => "Mandelbrot".to_string(),
                    "hypercube" => "Hypercube".to_string(),
                    "hypersphere" => "Hypersphere".to_string(),
                    _ => {
                        let mut c = cmd.to_string();
                        c[..1].make_ascii_uppercase();
                        c
                    }
                };
                return Some(CasCmd {
                    command: normalized,
                    args: vec![],
                });
            }
        }
        None
    }
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].to_string());
    args
}

pub fn execute_cas_command(document: &mut Document, cmd: &CasCmd) -> Option<String> {
    match cmd.command.as_str() {
        "Derivative" => {
            let expr = cmd.args.first()?;
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            match symbolic::derivative(expr, var) {
                Ok(d_expr) => {
                    // Also graph the derivative if it contains the variable
                    if d_expr.contains(var) || d_expr.parse::<f64>().is_ok() {
                        let label = next_function_label(document);
                        document.add_object(GeoObject::Function(
                            FunctionObj::new(&d_expr).with_label(&label),
                        ));
                        Some(format!(
                            "d/d{var}({expr}) = {d_expr}  →  Graficado como {label}"
                        ))
                    } else {
                        Some(format!("d/d{var}({expr}) = {d_expr}"))
                    }
                }
                Err(e) => Some(format!("Error calculando derivada: {}", e)),
            }
        }
        "Integral" => {
            let expr = cmd.args.first()?;
            let mut var = "x".to_string();
            let mut a_str = None;
            let mut b_str = None;

            if cmd.args.len() == 4 {
                var = cmd.args.get(1).unwrap().trim().to_string();
                a_str = cmd.args.get(2);
                b_str = cmd.args.get(3);
            } else if cmd.args.len() == 3 {
                a_str = cmd.args.get(1);
                b_str = cmd.args.get(2);
            } else if cmd.args.len() == 2 {
                var = cmd.args.get(1).unwrap().trim().to_string();
            }

            // Check if upper limit is a variable (e.g. Integral[expr, t, 0, x])
            // → graph as f(x) = ∫ₐˣ expr dt
            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let b_trim = b_s.trim();
                if b_trim.len() == 1 && b_trim.chars().all(|c| c.is_alphabetic()) {
                    let lower: f64 = a_s.trim().parse().unwrap_or(0.0);
                    let label = next_function_label(document);
                    let obj = FunctionObj::new(expr)
                        .with_label(&label)
                        .as_integral(&var, lower);
                    document.add_object(GeoObject::Function(obj));
                    return Some(format!(
                        "F({}) = ∫₍{}₎ˣ {} d{} → {}",
                        b_trim, lower, expr, var, label
                    ));
                }
            }

            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(expr).with_label(&label),
            ));

            if let (Some(a_s), Some(b_s)) = (a_str, b_str) {
                let a: f64 = a_s.trim().parse().unwrap_or(0.0);
                let b: f64 = b_s.trim().parse().unwrap_or(1.0);
                match symbolic::integrate_definite(expr, &var, a, b) {
                    Ok(result) => Some(format!("{} → Graficado como {}", result, label)),
                    Err(e) => Some(format!("Error calculando integral: {}", e)),
                }
            } else {
                match symbolic::integrate(expr, &var) {
                    Ok(result) => Some(format!("{} → Graficado original como {}", result, label)),
                    Err(e) => Some(format!("Error calculando integral: {}", e)),
                }
            }
        }
        "Solve" => {
            let expr = cmd.args.first()?;
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            let a: f64 = cmd
                .args
                .get(2)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(-20.0);
            let b: f64 = cmd
                .args
                .get(3)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(20.0);

            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(expr).with_label(&label),
            ));

            let expr_c1 = expr.clone();
            let expr_c2 = expr.clone();
            let vars1 = document.variables.clone();
            let vars2 = document.variables.clone();
            let f = move |x: f64| eval_function_with_vars(&expr_c1, x, &vars1).unwrap_or(f64::NAN);
            let mut roots = Vec::new();
            let steps = 4000;
            let step = (b - a) / steps as f64;
            let mut prev = f(a);
            for i in 1..=steps {
                let x = a + i as f64 * step;
                let curr = f(x);
                if curr.abs() < 1e-12 {
                    let duplicate = roots.iter().any(|&r: &f64| (r - x).abs() < 1e-6);
                    if !duplicate {
                        roots.push(x);
                    }
                } else if prev.is_finite() && curr.is_finite() && prev * curr <= 0.0 {
                    let mut left = x - step;
                    let mut right = x;
                    let mut f_left = prev;
                    let mut root = left;
                    for _ in 0..50 {
                        let mid = (left + right) * 0.5;
                        let f_mid =
                            eval_function_with_vars(&expr_c2, mid, &vars2).unwrap_or(f64::NAN);
                        if f_mid.abs() < 1e-9 {
                            root = mid;
                            break;
                        }
                        if f_mid.is_finite() {
                            if f_left * f_mid < 0.0 {
                                right = mid;
                            } else {
                                left = mid;
                                f_left = f_mid;
                            }
                        } else {
                            break;
                        }
                        root = mid;
                    }
                    let duplicate = roots.iter().any(|&r: &f64| (r - root).abs() < 1e-6);
                    if !duplicate {
                        roots.push(root);
                    }
                }
                prev = curr;
            }
            if roots.is_empty() {
                Some(format!(
                    "Sin raíces para {expr} en [{a:.1}, {b:.1}] → Graficado como {label}"
                ))
            } else {
                let mut strs = Vec::new();
                for r in &roots {
                    strs.push(format!("{var} ≈ {:.6}", r));
                    document.add_object(GeoObject::Point(
                        PointObj::new(Point2::new(*r, 0.0)).with_label("Raíz"),
                    ));
                }
                Some(format!("{} → Graficado como {}", strs.join(", "), label))
            }
        }

        "Limit" => {
            let expr = cmd.args.first()?;
            let var = cmd.args.get(1).map(|s| s.trim()).unwrap_or("x");
            let at_str = cmd.args.get(2).map(|s| s.trim()).unwrap_or("0");
            let at: f64 = match at_str {
                "inf" | "Inf" | "∞" => f64::INFINITY,
                "-inf" | "-Inf" | "-∞" => f64::NEG_INFINITY,
                s => s.parse().unwrap_or(0.0),
            };

            let label = next_function_label(document);
            document.add_object(GeoObject::Function(
                FunctionObj::new(expr).with_label(&label),
            ));

            match symbolic::limit(expr, var, at) {
                Ok(result) => {
                    if let Some(val_str) = result.split("=").last() {
                        if let Ok(val) = val_str.trim().parse::<f64>() {
                            if at.is_finite() {
                                document.add_object(GeoObject::Point(
                                    PointObj::new(Point2::new(at, val)).with_label("Límite"),
                                ));
                            }
                        }
                    }
                    Some(format!("{} → Graficado como {}", result, label))
                }
                Err(e) => Some(format!("Error: {}", e)),
            }
        }
        "Factor" => {
            let expr = cmd.args.first()?;
            match symbolic::factor(expr) {
                Ok(factors) => Some(format!("{} = {}", expr, factors)),
                Err(e) => Some(format!("Factor error: {}", e)),
            }
        }
        "Expand" => {
            let expr = cmd.args.first()?;
            match symbolic::expand(expr) {
                Ok(expanded) => Some(format!("{} = {}", expr, expanded)),
                Err(e) => Some(format!("Expand error: {}", e)),
            }
        }
        "Simplify" => {
            let expr = cmd.args.first()?;
            match symbolic::simplify(expr) {
                Ok(simplified) => Some(format!("{} = {}", expr, simplified)),
                Err(e) => Some(format!("Simplify error: {}", e)),
            }
        }
        _ => None,
    }
}

pub fn is_function_lhs(name: &str) -> bool {
    if let Some((id, args)) = name.split_once('(') {
        let id = id.trim();
        let args = args.trim_end_matches(')').trim();
        id.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit())
            && !id.is_empty()
            && !id.chars().next().unwrap().is_ascii_digit()
            && args.len() == 1
            && args.chars().all(|c| c.is_alphabetic())
    } else {
        false
    }
}

pub fn contains_var(text: &str, var: char) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == var {
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = if i + 1 < chars.len() {
                chars[i + 1]
            } else {
                ' '
            };
            if !prev.is_alphabetic() && !next.is_alphabetic() {
                return true;
            }
        }
    }
    false
}

pub fn find_object_by_label(document: &Document, label: &str) -> Option<ObjectId> {
    document
        .objects_iter()
        .find(|(_, obj)| obj.label() == label.trim())
        .map(|(id, _)| *id)
}

pub fn parse_point_str(s: &str) -> Result<(f64, f64), String> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() == 2 {
        Ok((
            parts[0].parse().map_err(|_| "bad x")?,
            parts[1].parse().map_err(|_| "bad y")?,
        ))
    } else {
        Err("expected (x, y)".into())
    }
}

pub fn next_function_label(document: &Document) -> String {
    let used: HashSet<String> = document
        .objects_iter()
        .filter_map(|(_, obj)| {
            if let GeoObject::Function(f) = obj {
                Some(f.label.clone())
            } else {
                None
            }
        })
        .collect();
    for c in 'f'..='z' {
        let label = format!("{}(x)", c);
        if !used.contains(&label) {
            return label;
        }
    }
    format!("f{}(x)", document.object_count())
}

pub fn find_extrema<F: Fn(f64) -> f64>(f: &F, a: f64, b: f64, find_max: bool) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let steps = 200;
    let step = (b - a) / steps as f64;
    let mut prev = f(a);
    for i in 1..steps {
        let x = a + i as f64 * step;
        let curr = f(x);
        let next = f(x + step);
        if find_max {
            if curr > prev && curr > next && curr.is_finite() {
                pts.push((x, curr));
            }
        } else {
            if curr < prev && curr < next && curr.is_finite() {
                pts.push((x, curr));
            }
        }
        prev = curr;
    }
    pts
}

pub fn root_10<F: Fn(f64) -> f64>(f: &F) -> Option<(f64, f64)> {
    for x0 in -10..=10 {
        if let Ok(r) = grafito_geometry::cas::newton_root_auto(f, x0 as f64) {
            if (-10.0..=10.0).contains(&r) {
                let fx = f(r);
                if fx.abs() < 0.1 {
                    return Some((r, fx));
                }
            }
        }
    }
    None
}

pub fn parse_preview(input_text: &str) -> Option<GeoObject> {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() {
        return None;
    }
    let text = raw_text
        .replace("x²", "x^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "3.14159265359")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("≤", "<=")
        .replace("≥", ">=");
    if parse_cas_command(&text).is_some() {
        return None;
    }

    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();

    if let Some((name, rest)) = text.split_once('=') {
        let name = name.trim();
        let rest = rest.trim();
        if is_function_lhs(name)
            && (rest.contains('x')
                || rest
                    .chars()
                    .all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c)))
        {
            return Some(GeoObject::Function(FunctionObj::new(rest).with_label(name)));
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return Some(GeoObject::Point(
                        PointObj::new(Point2::new(x, y)).with_label(name),
                    ));
                }
            }
        }
    } else {
        if text.contains('x') {
            return Some(GeoObject::Function(
                FunctionObj::new(text).with_label("preview"),
            ));
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return Some(GeoObject::Point(PointObj::new(Point2::new(x, y))));
                }
            }
        }
    }
    None
}

fn parse_brace_list(s: &str) -> Vec<f64> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    s.split(',')
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .collect()
}

fn parse_matrix_arg(s: &str) -> Option<Matrix> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut rows = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let row_str = &inner[start..=i];
                    let row: Vec<f64> = row_str
                        .trim_matches(|c| c == '[' || c == ']')
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect();
                    rows.push(row);
                    start = i + 1;
                }
            }
            ',' if depth == 0 => {
                start = i + 1;
            }
            _ => {}
        }
    }
    Matrix::from_rows(rows)
}
