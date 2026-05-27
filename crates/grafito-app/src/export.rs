use grafito_core::{Document, GeoObject};
use grafito_geometry::{Point2, Color};
use grafito_geometry::expr::eval_function_with_vars;
use glam::Vec2 as GlamVec2;

pub fn svg_color(c: Color) -> String {
    format!(
        "rgb({},{},{})",
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8
    )
}

pub fn tikz_color(c: Color) -> String {
    format!(
        "rgb,255:red,{};green,{};blue,{}",
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8
    )
}

pub fn export_tikz(document: &Document) -> String {
    let view = document.view();
    let mut out = String::from("% Grafito TikZ Export\n\\begin{tikzpicture}[scale=1]\n");
    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                out.push_str(&format!("  \\fill[black] ({:.2},{:.2}) circle (2pt);\n", s.x, s.y));
                if !p.label.is_empty() { out.push_str(&format!("  \\node[above right] at ({:.2},{:.2}) {{{}}};\n", s.x, s.y, p.label)); }
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start); let b = view.world_to_screen(l.end);
                out.push_str(&format!("  \\draw ({:.2},{:.2}) -- ({:.2},{:.2});\n", a.x, a.y, b.x, b.y));
                if !l.label.is_empty() { out.push_str(&format!("  \\node[above] at ({:.2},{:.2}) {{{}}};\n", (a.x+b.x)/2., (a.y+b.y)/2., l.label)); }
            }
            GeoObject::Circle(c) => {
                let cen = view.world_to_screen(c.center);
                let r = c.radius as f32 * view.scale;
                out.push_str(&format!("  \\draw ({:.2},{:.2}) circle ({:.2});\n", cen.x, cen.y, r));
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let pts: Vec<String> = poly.vertices.iter().map(|v| {
                    let s = view.world_to_screen(*v); format!("({:.2},{:.2})", s.x, s.y)
                }).collect();
                out.push_str(&format!("  \\draw {} -- cycle;\n", pts.join(" -- ")));
            }
            GeoObject::Ellipse(el) => {
                let cen = view.world_to_screen(el.center);
                out.push_str(&format!("  \\draw ({:.2},{:.2}) ellipse ({:.2} and {:.2});\n",
                    cen.x, cen.y, el.rx as f32 * view.scale, el.ry as f32 * view.scale));
            }
            _ => {}
        }
    }
    out.push_str("\\end{tikzpicture}\n");
    out
}

pub fn export_svg(document: &Document) -> String {
    use std::fmt::Write;
    let view = document.view();
    let w = view.screen_size.x as u32;
    let h = view.screen_size.y as u32;
    let mut svg = String::new();
    let _ = writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" style="background:white">"#);

    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(GlamVec2::new(w as f32, h as f32));
    let min_x = world_tl.x.floor() as i32 - 1;
    let max_x = world_br.x.ceil() as i32 + 1;
    let min_y = world_br.y.floor() as i32 - 1;
    let max_y = world_tl.y.ceil() as i32 + 1;

    for x in min_x..=max_x {
        let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
        let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
        let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="rgb(217,217,217)" stroke-width="1"/>"#, a.x, a.y, b.x, b.y);
    }
    for y in min_y..=max_y {
        let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
        let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
        let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="rgb(217,217,217)" stroke-width="1"/>"#, a.x, a.y, b.x, b.y);
    }

    let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
    let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);
    let ax_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
    let ax_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
    let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="2"/>"#, ax_a.x, ax_a.y, ax_b.x, ax_b.y);
    let ay_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
    let ay_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
    let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="2"/>"#, ay_a.x, ay_a.y, ay_b.x, ay_b.y);

    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{}" fill="{}"/>"#, s.x, s.y, p.size, svg_color(p.color));
                if !p.label.is_empty() {
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, s.x + p.size as f32 + 2.0, s.y - p.size as f32 - 2.0, p.label);
                }
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start);
                let b = view.world_to_screen(l.end);
                let _ = writeln!(svg, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{}"/>"#, a.x, a.y, b.x, b.y, svg_color(l.color), l.width);
                if !l.label.is_empty() {
                    let mx = (a.x + b.x) * 0.5;
                    let my = (a.y + b.y) * 0.5;
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, mx, my - 8.0, l.label);
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = (c.radius as f32) * view.scale;
                if let Some(fill) = c.fill_color {
                    let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{}"/>"#, center.x, center.y, r, svg_color(fill));
                }
                let _ = writeln!(svg, r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{}" stroke-width="{}"/>"#, center.x, center.y, r, svg_color(c.color), c.width);
                if !c.label.is_empty() {
                    let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, center.x + r + 2.0, center.y - r - 2.0, c.label);
                }
            }
            GeoObject::Polygon(poly) => {
                if poly.vertices.len() >= 3 {
                    let pts: Vec<String> = poly.vertices.iter()
                        .map(|v| {
                            let s = view.world_to_screen(*v);
                            format!("{:.1},{:.1}", s.x, s.y)
                        })
                        .collect();
                    let pts_str = pts.join(" ");
                    let fill = poly.fill_color.map_or("none".to_string(), |c| svg_color(c));
                    let _ = writeln!(svg, r#"<polygon points="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#, pts_str, fill, svg_color(poly.color), poly.width);
                    if !poly.label.is_empty() {
                        let cx: f32 = poly.vertices.iter().map(|v| view.world_to_screen(*v).x).sum::<f32>() / poly.vertices.len() as f32;
                        let cy: f32 = poly.vertices.iter().map(|v| view.world_to_screen(*v).y).sum::<f32>() / poly.vertices.len() as f32;
                        let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, cx, cy, poly.label);
                    }
                }
            }
            GeoObject::Function(fun) => {
                let min_x = fun.domain_min.unwrap_or(world_tl.x);
                let max_x = fun.domain_max.unwrap_or(world_br.x);
                let steps = 500;
                let step = (max_x - min_x) / steps as f64;
                let mut points = Vec::new();
                for i in 0..=steps {
                    let x = min_x + i as f64 * step;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, x, &document.variables) {
                        if y.is_finite() && y.abs() < 1e6 {
                            let s = view.world_to_screen(Point2::new(x, y));
                            points.push(format!("{:.1},{:.1}", s.x, s.y));
                        } else if !points.is_empty() {
                            let pts_str = points.join(" ");
                            let _ = writeln!(svg, r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}"/>"#, pts_str, svg_color(fun.color), fun.width);
                            points.clear();
                        }
                    }
                }
                if !points.is_empty() {
                    let pts_str = points.join(" ");
                    let _ = writeln!(svg, r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}"/>"#, pts_str, svg_color(fun.color), fun.width);
                }
                if !fun.label.is_empty() {
                    let mid_x = (min_x + max_x) * 0.5;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, mid_x, &document.variables) {
                        let s = view.world_to_screen(Point2::new(mid_x, y));
                        let _ = writeln!(svg, r#"<text x="{:.1}" y="{:.1}" font-family="sans-serif" font-size="12" fill="black">{}</text>"#, s.x, s.y + 14.0, fun.label);
                    }
                }
            }
            _ => {}
        }
    }
    svg.push_str("</svg>\n");
    svg
}

pub fn export_png(document: &Document, width: u32, height: u32) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
    let view = document.view();
    let grid_color = [217u8, 217, 217, 255];
    let black = [0u8, 0, 0, 255];

    let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(GlamVec2::new(width as f32, height as f32));
    let min_x = world_tl.x.floor() as i32 - 1;
    let max_x = world_br.x.ceil() as i32 + 1;
    let min_y = world_br.y.floor() as i32 - 1;
    let max_y = world_tl.y.ceil() as i32 + 1;

    for x in min_x..=max_x {
        let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
        let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
        draw_line(&mut img, a.x as i32, a.y as i32, b.x as i32, b.y as i32, grid_color);
    }
    for y in min_y..=max_y {
        let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
        let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
        draw_line(&mut img, a.x as i32, a.y as i32, b.x as i32, b.y as i32, grid_color);
    }

    let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
    let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);
    let ax_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
    let ax_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
    draw_line(&mut img, ax_a.x as i32, ax_a.y as i32, ax_b.x as i32, ax_b.y as i32, black);
    let ay_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
    let ay_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
    draw_line(&mut img, ay_a.x as i32, ay_a.y as i32, ay_b.x as i32, ay_b.y as i32, black);

    for (_, obj) in document.objects_iter() {
        if !obj.is_visible() { continue; }
        match obj {
            GeoObject::Point(p) => {
                let s = view.world_to_screen(p.position);
                let r = p.size.max(1.0) as i32;
                let c = [(p.color.r*255.0) as u8, (p.color.g*255.0) as u8, (p.color.b*255.0) as u8, 255];
                fill_circle(&mut img, s.x as i32, s.y as i32, r, c);
            }
            GeoObject::Line(l) => {
                let a = view.world_to_screen(l.start);
                let b = view.world_to_screen(l.end);
                let c = [(l.color.r*255.0) as u8, (l.color.g*255.0) as u8, (l.color.b*255.0) as u8, 255];
                let w = (l.width/2.0).max(0.5) as i32;
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let len = (dx*dx + dy*dy).sqrt().max(0.001);
                let nx = -dy / len;
                let ny = dx / len;
                for t in -w..=w {
                    let offset_x = (nx * t as f32) as i32;
                    let offset_y = (ny * t as f32) as i32;
                    draw_line(&mut img, a.x as i32 + offset_x , a.y as i32 + offset_y , b.x as i32 + offset_x , b.y as i32 + offset_y , c);
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = (c.radius as f32 * view.scale).max(0.5) as i32;
                if let Some(fill) = c.fill_color {
                    let fc = [(fill.r*255.0) as u8, (fill.g*255.0) as u8, (fill.b*255.0) as u8, 255];
                    fill_circle(&mut img, center.x as i32, center.y as i32, r, fc);
                }
                let cc = [(c.color.r*255.0) as u8, (c.color.g*255.0) as u8, (c.color.b*255.0) as u8, 255];
                let (cx, cy) = (center.x as i32, center.y as i32);
                let (w, h) = (width as i32, height as i32);
                for d in 0.max(r - (c.width/2.0) as i32)..=r + (c.width/2.0) as i32 {
                    let mut x = 0i32;
                    let mut y = d;
                    let mut p_val = 1 - d;
                    while x <= y {
                        for (px, py) in &[(cx+x,cy+y),(cx-x,cy+y),(cx+x,cy-y),(cx-x,cy-y),(cx+y,cy+x),(cx-y,cy+x),(cx+y,cy-x),(cx-y,cy-x)] {
                            if *px >= 0 && *px < w && *py >= 0 && *py < h {
                                img.put_pixel(*px as u32, *py as u32, to_rgba(cc));
                            }
                        }
                        x += 1;
                        if p_val < 0 { p_val += 2*x + 1; }
                        else { y -= 1; p_val += 2*(x - y) + 1; }
                    }
                }
            }
            GeoObject::Polygon(poly) => {
                if poly.vertices.len() >= 3 {
                    let pts: Vec<(i32, i32)> = poly.vertices.iter()
                        .map(|v| {
                            let s = view.world_to_screen(*v);
                            (s.x as i32, s.y as i32)
                        }).collect();
                    if let Some(fill) = poly.fill_color {
                        let fc = [(fill.r*255.0) as u8, (fill.g*255.0) as u8, (fill.b*255.0) as u8, 200];
                        let mut y_pts: Vec<i32> = pts.iter().map(|p| p.1).collect();
                        y_pts.sort();
                        let y_min = y_pts[0].max(0);
                        let y_max = y_pts[y_pts.len()-1].min(img.height() as i32 - 1);
                        for y in y_min..=y_max {
                            let mut xs: Vec<i32> = Vec::new();
                            for i in 0..pts.len() {
                                let (x0, y0) = pts[i];
                                let (x1, y1) = pts[(i+1)%pts.len()];
                                if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                                    let t = (y - y0) as f32 / (y1 - y0) as f32;
                                    xs.push((x0 as f32 + t * (x1 - x0) as f32) as i32);
                                }
                            }
                            xs.sort();
                            for i in (0..xs.len()-1).step_by(2) {
                                let x0 = xs[i].max(0);
                                let x1 = xs[i+1].min(img.width() as i32 - 1);
                                for x in x0..=x1 {
                                    img.put_pixel(x as u32, y as u32, to_rgba(fc));
                                }
                            }
                        }
                    }
                    let sc = [(poly.color.r*255.0) as u8, (poly.color.g*255.0) as u8, (poly.color.b*255.0) as u8, 255];
                    for i in 0..pts.len() {
                        let a = pts[i];
                        let b = pts[(i+1)%pts.len()];
                        draw_line(&mut img, a.0, a.1, b.0, b.1, sc);
                    }
                }
            }
            GeoObject::Function(fun) => {
                let (w, h) = (width as i32, height as i32);
                let min_x = fun.domain_min.unwrap_or(world_tl.x);
                let max_x = fun.domain_max.unwrap_or(world_br.x);
                let steps = 500;
                let step = (max_x - min_x) / steps as f64;
                let fc = [(fun.color.r*255.0) as u8, (fun.color.g*255.0) as u8, (fun.color.b*255.0) as u8, 255];
                let mut prev: Option<(i32, i32)> = None;
                for i in 0..=steps {
                    let x = min_x + i as f64 * step;
                    if let Ok(y) = eval_function_with_vars(&fun.expr, x, &document.variables) {
                        if y.is_finite() && y.abs() < 1e6 {
                            let s = view.world_to_screen(Point2::new(x, y));
                            let curr = (s.x as i32, s.y as i32);
                            if curr.0 >= 0 && curr.0 < w && curr.1 >= 0 && curr.1 < h {
                                if let Some(prev_p) = prev {
                                    draw_line(&mut img, prev_p.0, prev_p.1, curr.0, curr.1, fc);
                                }
                                prev = Some(curr);
                            } else {
                                prev = None;
                            }
                            continue;
                        }
                    }
                    prev = None;
                }
            }
            _ => {}
        }
    }
    img
}

fn to_rgba(c: [u8; 4]) -> image::Rgba<u8> { image::Rgba(c) }

fn draw_line(img: &mut image::RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && x < w && y >= 0 && y < h {
            img.put_pixel(x as u32, y as u32, to_rgba(color));
        }
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

fn fill_circle(img: &mut image::RgbaImage, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let r2 = r * r;
    for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
        let dy = y - cy;
        let dx = ((r2 - dy*dy) as f64).sqrt() as i32;
        let x0 = (cx - dx).max(0);
        let x1 = (cx + dx).min(w - 1);
        for x in x0..=x1 {
            img.put_pixel(x as u32, y as u32, to_rgba(color));
        }
    }
}
