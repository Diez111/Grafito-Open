use grafito_core::{
    Document, GeoObject, ObjectId,
    PointObj, LineObj, FunctionObj, EllipseObj,
    PolygonObj, ParabolaObj, HyperbolaObj,
    Point3DObj, Segment3DObj, Surface3DObj,
};
use grafito_geometry::Point2;
use grafito_geometry::Point3D;
use grafito_geometry::expr::{eval_function_with_vars, evaluate};
use grafito_geometry::symbolic;
use std::collections::{HashMap, HashSet};

fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() { res.push('*'); }
            if c1 == ')' && c2.is_ascii_alphabetic() { res.push('*'); }
            if c1 == ')' && c2.is_ascii_digit() { res.push('*'); }
            if c1.is_ascii_digit() && c2 == '(' { res.push('*'); }
            if c1 == ')' && c2 == '(' { res.push('*'); }
            if (c1 == 'x' || c1 == 'y') && c2 == '(' { res.push('*'); }
            if (c1 == 'x' || c1 == 'y') && c2.is_ascii_alphabetic() { res.push('*'); }
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
        
    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();
    let mut result: Option<String> = None;

    if let Some(cmd) = parse_cas_command(text) {
        match cmd.command.as_str() {
            "Ellipse" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if parts.len() >= 2 {
                    let rx = cmd.args[1].trim().parse().unwrap_or(1.0);
                    let ry = cmd.args[2].trim().parse().unwrap_or(1.0);
                    document.add_object(GeoObject::Ellipse(EllipseObj::new(Point2::new(parts[0], parts[1]), rx, ry)));
                    input_text.clear();
                    return None;
                }
            }
            "RegularPolygon" if cmd.args.len() >= 3 => {
                let center_str = cmd.args[0].trim();
                let rest = center_str.trim_start_matches('(').trim_end_matches(')');
                let parts: Vec<f64> = rest.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if parts.len() >= 2 {
                    let n = cmd.args[1].trim().parse::<usize>().unwrap_or(4).max(3).min(64);
                    let r = cmd.args[2].trim().parse::<f64>().unwrap_or(1.0);
                    let cx = parts[0]; let cy = parts[1];
                    let verts: Vec<Point2> = (0..n).map(|i| {
                        let a = i as f64 / n as f64 * std::f64::consts::TAU;
                        Point2::new(cx + r * a.cos(), cy + r * a.sin())
                    }).collect();
                    document.add_object(GeoObject::Polygon(PolygonObj::new(verts)));
                    input_text.clear();
                    return None;
                }
            }
            "Translate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok((dx, dy))) = (find_object_by_label(document, &cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let new_pos = Point2::new(p.position.x + dx, p.position.y + dy);
                                document.add_object(GeoObject::Point(PointObj::new(new_pos).with_label(format!("{}'", p.label))));
                            }
                            _ => { result = Some("Translate only supports Points".into()); }
                        }
                    }
                } else { result = Some("Usage: Translate[Object, (dx,dy)]".into()); }
                input_text.clear();
                return result;
            }
            "Rotate" if cmd.args.len() == 2 => {
                if let (Some(id), Ok(angle)) = (find_object_by_label(document, &cmd.args[0]), cmd.args[1].trim().parse::<f64>()) {
                    if let Some(obj) = document.get_object(id) {
                        match obj {
                            GeoObject::Point(p) => {
                                let c = angle.to_radians();
                                let nx = p.position.x * c.cos() - p.position.y * c.sin();
                                let ny = p.position.x * c.sin() + p.position.y * c.cos();
                                document.add_object(GeoObject::Point(PointObj::new(Point2::new(nx, ny)).with_label(format!("{}'", p.label))));
                            }
                            _ => { result = Some("Rotate only supports Points".into()); }
                        }
                    }
                } else { result = Some("Usage: Rotate[Object, angle_degrees]".into()); }
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
                    let obj = GeoObject::Surface3D(Surface3DObj::new(expr, (x_min, x_max), (y_min, y_max)));
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
                        let dx = px - cx; let dy = py - cy;
                        let d = (dx*dx+dy*dy).sqrt();
                        if d > r {
                            let a = r*r/d;
                            let h = (r*r - a*a).sqrt();
                            let pm = Point2::new(cx + a*dx/d, cy + a*dy/d);
                            let perp_x = -h*dy/d; let perp_y = h*dx/d;
                            let t1 = Point2::new(pm.x + perp_x, pm.y + perp_y);
                            let t2 = Point2::new(pm.x - perp_x, pm.y - perp_y);
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(px, py), t1).with_label("T1")));
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(px, py), t2).with_label("T2")));
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "PerpendicularBisector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let mx = (x1 + x2) * 0.5; let my = (y1 + y2) * 0.5;
                    let dx = x2 - x1; let dy = y2 - y1;
                    let p1 = Point2::new(mx - dy * 5.0, my + dx * 5.0);
                    let p2 = Point2::new(mx + dy * 5.0, my - dx * 5.0);
                    document.add_object(GeoObject::Line(LineObj::new(p1, p2).with_label("B")));
                }
                input_text.clear(); return None;
            }
            "AngleBisector" if cmd.args.len() == 3 => {
                if let (Ok((x1, y1)), Ok((xv, yv)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1]), parse_point_str(&cmd.args[2])) {
                    let d1 = ((xv-x1).powi(2) + (yv-y1).powi(2)).sqrt();
                    let d2 = ((xv-x2).powi(2) + (yv-y2).powi(2)).sqrt();
                    if d1 > 0.0 && d2 > 0.0 {
                        let ux = (x1 - xv) / d1; let uy = (y1 - yv) / d1;
                        let vx = (x2 - xv) / d2; let vy = (y2 - yv) / d2;
                        let bx = ux + vx; let by = uy + vy;
                        let b_len = (bx*bx + by*by).sqrt();
                        if b_len > 0.0 {
                            let p = Point2::new(xv + bx / b_len * 5.0, yv + by / b_len * 5.0);
                            document.add_object(GeoObject::Line(LineObj::new(Point2::new(xv, yv), p).with_label("Ab")));
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "Midpoint" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let obj = GeoObject::Point(PointObj::new(Point2::new((x1+x2)*0.5, (y1+y2)*0.5)).with_label("M"));
                    document.add_object(obj);
                }
                input_text.clear(); return None;
            }
            "Vector" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let obj = GeoObject::Line(LineObj::new(Point2::new(x1, y1), Point2::new(x2, y2)).with_label("v"));
                    document.add_object(obj);
                }
                input_text.clear(); return None;
            }
            "Ray" if cmd.args.len() == 2 => {
                if let (Ok((x1, y1)), Ok((x2, y2))) = (parse_point_str(&cmd.args[0]), parse_point_str(&cmd.args[1])) {
                    let dx = x2 - x1; let dy = y2 - y1;
                    let len = (dx*dx+dy*dy).sqrt().max(0.01);
                    let far = Point2::new(x1 + dx/len * 100.0, y1 + dy/len * 100.0);
                    document.add_object(GeoObject::Line(LineObj::new(Point2::new(x1, y1), far).with_label("r")));
                }
                input_text.clear(); return None;
            }
            "Parabola" if cmd.args.len() >= 2 => {
                if let (Ok((vx, vy)), Ok(p)) = (parse_point_str(&cmd.args[0]), cmd.args[1].trim().parse::<f64>()) {
                    document.add_object(GeoObject::Parabola(ParabolaObj::new(Point2::new(vx, vy), p)));
                }
                input_text.clear(); return None;
            }
            "Hyperbola" if cmd.args.len() >= 3 => {
                if let (Ok((cx, cy)), Ok(a), Ok(b)) = (parse_point_str(&cmd.args[0]), cmd.args[1].trim().parse::<f64>(), cmd.args[2].trim().parse::<f64>()) {
                    document.add_object(GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(cx, cy), a, b)));
                }
                input_text.clear(); return None;
            }
            "Dilate" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok(factor), Ok((cx, cy))) = (
                    parse_point_str(&cmd.args[0]),
                    cmd.args[1].trim().parse::<f64>(),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let nx = cx + (px - cx) * factor;
                    let ny = cy + (py - cy) * factor;
                    document.add_object(GeoObject::Point(PointObj::new(Point2::new(nx, ny)).with_label("D'")));
                }
                input_text.clear(); return None;
            }
            "Reflect" if cmd.args.len() == 3 => {
                if let (Ok((px, py)), Ok((ax, ay)), Ok((bx, by))) = (
                    parse_point_str(&cmd.args[0]),
                    parse_point_str(&cmd.args[1]),
                    parse_point_str(&cmd.args[2]),
                ) {
                    let dx = bx - ax; let dy = by - ay;
                    let len2 = dx*dx + dy*dy;
                    if len2 > 0.0 {
                        let t = ((px-ax)*dx + (py-ay)*dy) / len2;
                        let cx = ax + t * dx;
                        let cy = ay + t * dy;
                        let rx = 2.0 * cx - px;
                        let ry = 2.0 * cy - py;
                        document.add_object(GeoObject::Point(PointObj::new(Point2::new(rx, ry)).with_label("R'")));
                    }
                }
                input_text.clear(); return None;
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
                        if let Ok(y) = evaluate(expr, &vars.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()) {
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
                input_text.clear(); return None;
            }
            "FunctionInspector" if cmd.args.len() == 1 => {
                let expr = cmd.args[0].trim();
                let v = document.variables.clone();
                let f = |x: f64| {
                    let mut vars: Vec<(String, f64)> = v.iter().map(|(k,val)| (k.clone(), *val)).collect();
                    vars.push(("x".to_string(), x));
                    evaluate(expr, &vars).unwrap_or(f64::NAN)
                };
                let mins = find_extrema(&f, -10.0, 10.0, false);
                let maxs = find_extrema(&f, -10.0, 10.0, true);
                let mut res = String::new();
                if let Some((mx, my)) = root_10(&f) {
                    res.push_str(&format!("Root ≈ ({}: {:.4})", mx, my));
                }
                for (mx, my) in &mins { res.push_str(&format!(" Min@({:.2},{:.2})", mx, my)); }
                for (mx, my) in &maxs { res.push_str(&format!(" Max@({:.2},{:.2})", mx, my)); }
                result = Some(if res.is_empty() { "No extrema found in [-10,10]".into() } else { res });
                input_text.clear(); return result;
            }
            "Normal" if cmd.args.len() == 2 => {
                let mu: f64 = cmd.args[0].trim().parse().unwrap_or(0.0);
                let sigma: f64 = cmd.args[1].trim().parse().unwrap_or(1.0);
                let expr = format!("exp(-(x-{})^2/(2*{}^2))/({}*sqrt(2*pi))", mu, sigma, sigma);
                document.add_object(GeoObject::Function(FunctionObj::new(expr).with_label(format!("N({},{})", mu, sigma))));
                result = Some(format!("Normal N({},{}) added", mu, sigma));
                input_text.clear(); return result;
            }
            "Binomial" if cmd.args.len() == 3 => {
                let n: usize = cmd.args[0].trim().parse().unwrap_or(10);
                let p: f64 = cmd.args[1].trim().parse().unwrap_or(0.5);
                let k: usize = cmd.args[2].trim().parse().unwrap_or(1);
                let comb = |n: usize, k: usize| -> f64 {
                    if k > n { return 0.0; }
                    let k = k.min(n - k);
                    let mut result = 1.0;
                    for i in 0..k { result = result * (n - i) as f64 / (i + 1) as f64; }
                    result
                };
                let prob = comb(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32);
                result = Some(format!("P(X={}) = {:.6} (Binom({},{}))", k, prob, n, p));
                input_text.clear(); return result;
            }
            "Poisson" if cmd.args.len() == 2 => {
                let lambda: f64 = cmd.args[0].trim().parse().unwrap_or(1.0);
                let k: usize = cmd.args[1].trim().parse().unwrap_or(1);
                let mut prob = (-lambda).exp();
                for i in 1..=k { prob *= lambda / i as f64; }
                result = Some(format!("P(X={}) = {:.6} (Poisson({}))", k, prob, lambda));
                input_text.clear(); return result;
            }
            "Curve3D" if cmd.args.len() >= 4 => {
                let exprs = cmd.args[0].trim();
                let t_min: f64 = cmd.args[2].trim().parse().unwrap_or(0.0);
                let t_max: f64 = cmd.args[3].trim().parse().unwrap_or(6.28);
                let steps = 200;
                let mut pts = Vec::new();
                for i in 0..=steps {
                    let t = t_min + (t_max - t_min) * i as f64 / steps as f64;
                    let mut vars = document.variables.clone();
                    vars.insert("t".to_string(), t);
                    let inner = exprs.trim_start_matches('(').trim_end_matches(')');
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() >= 3 {
                        let vals: Vec<f64> = parts.iter().filter_map(|s| {
                            let expr = s.trim();
                            eval_function_with_vars(expr, t, &vars).ok().or_else(|| {
                                evaluate(expr, &vars.iter().map(|(k,v)| (k.clone(), *v)).collect::<Vec<_>>()).ok()
                            })
                        }).collect();
                        if vals.len() >= 3 {
                            pts.push(Point3D::new(vals[0], vals[1], vals[2]));
                        }
                    }
                }
                if pts.len() >= 2 {
                    let mut segs = Vec::new();
                    for i in 1..pts.len() {
                        segs.push((pts[i-1], pts[i]));
                    }
                    for (a, b) in &segs {
                        document.add_object(GeoObject::Segment3D(
                            Segment3DObj::new(*a, *b).with_label("C3")
                        ));
                    }
                }
                input_text.clear(); return None;
            }
            "SetValue" if cmd.args.len() == 2 => {
                if let Some(id) = find_object_by_label(document, &cmd.args[0]) {
                    if let Ok(val) = cmd.args[1].trim().parse::<f64>() {
                        document.set_variable(cmd.args[0].trim().to_string(), val);
                    } else if let Ok((x, y)) = parse_point_str(&cmd.args[1]) {
                        if let Some(obj) = document.get_object_mut(id) {
                            if let GeoObject::Point(p) = obj { p.position = Point2::new(x, y); }
                        }
                    }
                }
                input_text.clear(); return None;
            }
            "Extrude" if cmd.args.len() >= 2 => {
                let height: f64 = cmd.args.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(1.0);
                let id_opt = find_object_by_label(document, &cmd.args[0]);
                let vertices = id_opt.and_then(|id| document.get_object(id).and_then(|obj| {
                    if let GeoObject::Polygon(poly) = obj {
                        if poly.vertices.len() >= 3 { Some(poly.vertices.clone()) }
                        else { None }
                    } else { None }
                }));
                if let Some(verts) = vertices {
                    let base_y = 0.0; let top_y = height;
                    for i in 0..verts.len() {
                        let v = verts[i];
                        let vn = verts[(i+1) % verts.len()];
                        let b = Point3D::new(v.x, base_y, v.y);
                        let t = Point3D::new(v.x, top_y, v.y);
                        let bn = Point3D::new(vn.x, base_y, vn.y);
                        let tn = Point3D::new(vn.x, top_y, vn.y);
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(b, t).with_label("E")));
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(b, bn).with_label("E")));
                        document.add_object(GeoObject::Segment3D(Segment3DObj::new(t, tn).with_label("E")));
                    }
                } else {
                    result = Some("Extrude only supports Polygons with 3+ vertices".into());
                }
                input_text.clear(); return result;
            }
            "Script" if cmd.args.len() >= 1 => {
                let commands: Vec<String> = cmd.args[0].split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let mut output = String::new();
                for c in &commands {
                    let mut temp = c.clone();
                    if let Some(res) = process_input(document, &mut temp) { output.push_str(&res); output.push('\n'); }
                }
                result = if output.is_empty() { Some("Script executed".into()) } else { Some(output) };
                input_text.clear(); return result;
            }
            "Simplify" if cmd.args.len() >= 1 => {
                let expr = cmd.args[0].trim();
                let vars: Vec<(String, f64)> = document.variables.iter().map(|(k,v)| (k.clone(), *v)).collect();
                match evaluate(expr, &vars) {
                    Ok(val) => result = Some(format!("{} ≈ {}", expr, val)),
                    Err(e) => result = Some(format!("Simplify error: {}", e)),
                }
                input_text.clear(); return result;
            }
            _ => {}
        }
        result = execute_cas_command(document, &cmd);
        input_text.clear();
        return result;
    }

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
        if is_function_lhs(name) && (rest.contains('x') || rest.chars().all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c))) {
            if let Some(id) = find_object_by_label(document, name) {
                document.remove_object(id);
            }
            let obj = GeoObject::Function(FunctionObj::new(rest).with_label(name));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len()-1];
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
            let inner = &rest[1..rest.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    let obj = GeoObject::Point3D(Point3DObj::new(Point3D::new(x, y, z)).with_label(name));
                    document.add_object(obj);
                    input_text.clear();
                    return None;
                }
            }
        }
    } else {
        if contains_var(text, 'x') {
            let label = next_function_label(document);
            let obj = GeoObject::Function(FunctionObj::new(text).with_label(label));
            document.add_object(obj);
            input_text.clear();
            return None;
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(x), Ok(y), Ok(z)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
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
        let inside = &text[open+1..close];
        let args: Vec<String> = split_args(inside).into_iter().map(|s| s.trim().to_string()).collect();
        if command.is_empty() || args.is_empty() { return None; }
        match command.as_str() {
            "Derivative" | "Integral" | "Solve" | "Limit" | "NSolve" | "Factor" | "Expand" | "Simplify" => {}
            _ => return None,
        }
        Some(CasCmd { command, args })
    } else {
        None
    }
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
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

pub fn execute_cas_command(document: &Document, cmd: &CasCmd) -> Option<String> {
    match cmd.command.as_str() {
        "Derivative" => {
            let expr = cmd.args.get(0)?;
            Some(format!("Derivative[{}]: approx (f(x+h)-f(x))/h with f(x)={}", expr, expr))
        }
        "Integral" => {
            let expr = cmd.args.get(0)?;
            let a: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let b: f64 = cmd.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
            let f = move |x: f64| {
                eval_function_with_vars(expr, x, &document.variables).unwrap_or(0.0)
            };
            let result = grafito_geometry::cas::integral_auto(f, a, b);
            Some(format!("∫[{}..{}] {} dx = {:.6}", a, b, expr, result))
        }
        "Solve" | "NSolve" => {
            let expr = cmd.args.get(0)?;
            let a: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-10.0);
            let b: f64 = cmd.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10.0);
            let f = move |x: f64| {
                eval_function_with_vars(expr, x, &document.variables).unwrap_or(f64::NAN)
            };
            match grafito_geometry::cas::find_root(f, (a, b)) {
                Some(root) => Some(format!("Root of {} in [{:.1}, {:.1}] ≈ {:.6}", expr, a, b, root)),
                None => Some(format!("No root found for {} in [{}, {}]", expr, a, b)),
            }
        }
        "Limit" => {
            let expr = cmd.args.get(0)?;
            let x0: f64 = cmd.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let f = move |x: f64| {
                eval_function_with_vars(expr, x, &document.variables).unwrap_or(f64::NAN)
            };
            let result = grafito_geometry::cas::limit(f, x0);
            Some(format!("lim[x→{:.1}] {} ≈ {:.6}", x0, expr, result))
        }
        "Factor" => {
            let expr = cmd.args.get(0)?;
            match symbolic::factor(expr) {
                Ok(factors) => Some(format!("{} = {}", expr, factors)),
                Err(e) => Some(format!("Factor error: {}", e)),
            }
        }
        "Expand" => {
            let expr = cmd.args.get(0)?;
            match symbolic::expand(expr) {
                Ok(expanded) => Some(format!("{} = {}", expr, expanded)),
                Err(e) => Some(format!("Expand error: {}", e)),
            }
        }
        "Simplify" => {
            let expr = cmd.args.get(0)?;
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
            let prev = if i > 0 { chars[i-1] } else { ' ' };
            let next = if i + 1 < chars.len() { chars[i+1] } else { ' ' };
            if !prev.is_alphabetic() && !next.is_alphabetic() {
                return true;
            }
        }
    }
    false
}

pub fn find_object_by_label(document: &Document, label: &str) -> Option<ObjectId> {
    document.objects_iter().find(|(_, obj)| obj.label() == label.trim()).map(|(id, _)| *id)
}

pub fn parse_point_str(s: &str) -> Result<(f64, f64), String> {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() == 2 {
        Ok((parts[0].parse().map_err(|_| "bad x")?, parts[1].parse().map_err(|_| "bad y")?))
    } else {
        Err("expected (x, y)".into())
    }
}

pub fn next_function_label(document: &Document) -> String {
    let used: HashSet<String> = document.objects_iter()
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
            if r >= -10.0 && r <= 10.0 {
                let fx = f(r);
                if fx.abs() < 0.1 { return Some((r, fx)); }
            }
        }
    }
    None
}

pub fn parse_preview(input_text: &str) -> Option<GeoObject> {
    let raw_text = input_text.trim().to_string();
    if raw_text.is_empty() { return None; }
    let text = raw_text
        .replace("x²", "x^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "3.14159265359")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("≤", "<=")
        .replace("≥", ">=");
    let text_with_implicit = insert_implicit_multiplication(&text);
    let text = text_with_implicit.as_str();

    if parse_cas_command(text).is_some() { return None; }

    if let Some((name, rest)) = text.split_once('=') {
        let name = name.trim();
        let rest = rest.trim();
        if is_function_lhs(name) && (rest.contains('x') || rest.chars().all(|c| c.is_numeric() || "+-*/().^x sincostanlognatqerfabs ".contains(c))) {
            return Some(GeoObject::Function(FunctionObj::new(rest).with_label(name)));
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = &rest[1..rest.len()-1];
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                    return Some(GeoObject::Point(PointObj::new(Point2::new(x, y)).with_label(name)));
                }
            }
        }
    } else {
        if text.contains('x') {
            return Some(GeoObject::Function(FunctionObj::new(text).with_label("preview")));
        }
        if text.starts_with('(') && text.ends_with(')') {
            let inner = &text[1..text.len()-1];
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
