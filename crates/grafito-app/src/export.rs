//! File I/O: save/load documents and export images.

use anyhow::{Context, Result};
use grafito_core::{Document, GeoObject, LineKind};
use grafito_geometry::Point2;
use image::{ImageBuffer, Rgba, RgbaImage};

fn escape_xml(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Save document to a JSON file.
pub fn save_document(doc: &Document, path: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(doc).context("Failed to serialize document")?;
    std::fs::write(path, json).context("Failed to write file")?;
    Ok(())
}

/// Load document from a JSON file.
pub fn load_document(path: &str) -> Result<Document> {
    let json = std::fs::read_to_string(path).context("Failed to read file")?;
    grafito_core::validation::validate_document_json(&json)
        .map_err(|e| anyhow::anyhow!("Document validation failed: {}", e))?;
    let doc: Document = serde_json::from_str(&json).context("Failed to parse document")?;
    grafito_core::validation::validate_document(&doc)
        .map_err(|e| anyhow::anyhow!("Document validation failed: {}", e))?;
    Ok(doc)
}

/// Export the document as SVG (basic implementation).
pub fn export_svg(doc: &Document, width: f64, height: f64) -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}">"#,
        width, height, 0, 0, width, height
    );
    svg.push_str(r#"<rect width="100%" height="100%" fill="white"/>"#);

    let view = doc.view();
    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        let c = obj.color();
        let rgb = format!(
            "rgb({},{},{})",
            (c.r * 255.0).clamp(0.0, 255.0) as u8,
            (c.g * 255.0).clamp(0.0, 255.0) as u8,
            (c.b * 255.0).clamp(0.0, 255.0) as u8
        );
        match obj {
            grafito_core::GeoObject::Point(p) => {
                let screen = view.world_to_screen(p.position);
                svg.push_str(&format!(
                    r#"<circle cx="{:.1}" cy="{:.1}" r="{}" fill="{}"/>"#,
                    screen.x, screen.y, p.size, rgb
                ));
            }
            grafito_core::GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    grafito_core::LineKind::Segment => (l.start, l.end),
                    _ => {
                        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                        let world_br =
                            view.screen_to_world(glam::Vec2::new(width as f32, height as f32));
                        let mut view_bounds = grafito_geometry::AABB::new(world_tl, world_tl);
                        view_bounds.expand(&world_br);
                        l.clip_to_aabb(view_bounds).unwrap_or((l.start, l.end))
                    }
                };
                let sa = view.world_to_screen(a);
                let sb = view.world_to_screen(b);
                svg.push_str(&format!(
                    r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.1}"/>"#,
                    sa.x, sa.y, sb.x, sb.y, rgb, l.width
                ));
            }
            grafito_core::GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = c.radius * view.scale;
                svg.push_str(&format!(
                    r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                    center.x, center.y, r, rgb, c.width
                ));
            }
            grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let points: Vec<String> = poly
                    .vertices
                    .iter()
                    .map(|v| {
                        let s = view.world_to_screen(*v);
                        format!("{:.1},{:.1}", s.x, s.y)
                    })
                    .collect();
                svg.push_str(&format!(
                    r#"<polygon points="{}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                    points.join(" "),
                    rgb,
                    poly.width
                ));
            }
            grafito_core::GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let steps = 200;
                let dx = (x_max - x_min) / steps as f64;
                let mut path = String::new();
                for i in 0..=steps {
                    let x = x_min + i as f64 * dx;
                    if let Ok(y) =
                        grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                    {
                        if y.is_finite() {
                            let s = view.world_to_screen(grafito_geometry::Point2::new(x, y));
                            if i == 0 {
                                path.push_str(&format!("M{:.1},{:.1} ", s.x, s.y));
                            } else {
                                path.push_str(&format!("L{:.1},{:.1} ", s.x, s.y));
                            }
                        }
                    }
                }
                if !path.is_empty() {
                    svg.push_str(&format!(
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                        path, rgb, f.width
                    ));
                }
            }
            grafito_core::GeoObject::Text(txt) => {
                let s = view.world_to_screen(txt.position);
                svg.push_str(&format!(
                    r#"<text x="{:.1}" y="{:.1}" fill="{}" font-size="{}">{}</text>"#,
                    s.x,
                    s.y,
                    rgb,
                    txt.font_size,
                    escape_xml(&txt.content)
                ));
            }
            _ => {}
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Export the document as a PNG raster image using CPU-side primitive rendering.
pub fn export_png(doc: &Document, width: u32, height: u32, path: &str) -> Result<()> {
    let mut img: RgbaImage = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let view = doc.view();

    let to_screen = |wp: Point2| -> (i32, i32) {
        let s = view.world_to_screen(wp);
        (s.x as i32, s.y as i32)
    };

    let to_color = |c: grafito_geometry::Color| -> Rgba<u8> {
        Rgba([
            (c.r * 255.0).clamp(0.0, 255.0) as u8,
            (c.g * 255.0).clamp(0.0, 255.0) as u8,
            (c.b * 255.0).clamp(0.0, 255.0) as u8,
            255,
        ])
    };

    // Draw axes
    let (ax, ay) = to_screen(Point2::new(0.0, 0.0));
    draw_line(
        &mut img,
        ax,
        0,
        ax,
        height as i32,
        Rgba([180, 180, 180, 255]),
    );
    draw_line(
        &mut img,
        0,
        ay,
        width as i32,
        ay,
        Rgba([180, 180, 180, 255]),
    );

    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        let color = to_color(obj.color());
        match obj {
            GeoObject::Point(p) => {
                let (px, py) = to_screen(p.position);
                draw_circle_filled(&mut img, px, py, p.size as i32, color);
            }
            GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    LineKind::Segment => (l.start, l.end),
                    _ => {
                        let wt = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                        let wb = view.screen_to_world(glam::Vec2::new(width as f32, height as f32));
                        let mut bounds = grafito_geometry::AABB::new(wt, wt);
                        bounds.expand(&wb);
                        l.clip_to_aabb(bounds).unwrap_or((l.start, l.end))
                    }
                };
                let (x1, y1) = to_screen(a);
                let (x2, y2) = to_screen(b);
                draw_line(&mut img, x1, y1, x2, y2, color);
            }
            GeoObject::Circle(c) => {
                let (cx_i, cy_i) = to_screen(c.center);
                let r = (c.radius * view.scale) as i32;
                draw_circle_outline(&mut img, cx_i, cy_i, r, color);
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                for i in 0..poly.vertices.len() {
                    let (x1, y1) = to_screen(poly.vertices[i]);
                    let (x2, y2) = to_screen(poly.vertices[(i + 1) % poly.vertices.len()]);
                    draw_line(&mut img, x1, y1, x2, y2, color);
                }
            }
            GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let steps = 500;
                let dx = (x_max - x_min) / steps as f64;
                let mut prev: Option<(i32, i32)> = None;
                for i in 0..=steps {
                    let x = x_min + i as f64 * dx;
                    if let Ok(y) =
                        grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                    {
                        if y.is_finite() {
                            let (sx, sy) = to_screen(Point2::new(x, y));
                            if let Some((px, py)) = prev {
                                draw_line(&mut img, px, py, sx, sy, color);
                            }
                            prev = Some((sx, sy));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    img.save(path).context("Failed to write PNG file")?;
    Ok(())
}

/// Export the document as LaTeX (tikz/pgfplots) code.
pub fn export_latex(doc: &Document) -> String {
    let mut tex = String::new();
    tex.push_str("\\documentclass{standalone}\n");
    tex.push_str("\\usepackage{tikz}\n");
    tex.push_str("\\usepackage{pgfplots}\n");
    tex.push_str("\\pgfplotsset{compat=1.18}\n");
    tex.push_str("\\begin{document}\n");
    tex.push_str("\\begin{tikzpicture}\n");

    // Coordinate transform: world → tikz (just pass world coords directly,
    // tikz uses mathematical coordinates natively)
    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        match obj {
            GeoObject::Point(p) => {
                let label = escape_latex(&p.label);
                tex.push_str(&format!(
                    "\\filldraw ({:.4},{:.4}) circle (2pt) node[above right]{{{}}};\n",
                    p.position.x, p.position.y, label
                ));
            }
            GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    LineKind::Segment => (l.start, l.end),
                    _ => (l.start, l.end),
                };
                let cmd = match l.kind {
                    LineKind::Segment => "--",
                    LineKind::Ray => "--",
                    LineKind::Line => "--",
                };
                tex.push_str(&format!(
                    "\\draw ({:.4},{:.4}) {} ({:.4},{:.4});\n",
                    a.x, a.y, cmd, b.x, b.y
                ));
            }
            GeoObject::Circle(c) => {
                tex.push_str(&format!(
                    "\\draw ({:.4},{:.4}) circle ({:.4});\n",
                    c.center.x, c.center.y, c.radius
                ));
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let pts: Vec<String> = poly
                    .vertices
                    .iter()
                    .map(|v| format!("({:.4},{:.4})", v.x, v.y))
                    .collect();
                tex.push_str(&format!("\\draw {} -- cycle;\n", pts.join(" -- ")));
            }
            GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let expr = escape_latex(&f.expr);
                tex.push_str(&format!(
                    "\\begin{{axis}}[xmin={:.2}, xmax={:.2}, axis lines=middle]\n",
                    x_min, x_max
                ));
                tex.push_str(&format!(
                    "\\addplot[domain={:.2}:{:.2}, samples=200] {{{}}};\n",
                    x_min, x_max, expr
                ));
                tex.push_str("\\end{axis}\n");
            }
            GeoObject::Ellipse(e) => {
                tex.push_str(&format!(
                    "\\draw[rotate around={{{:.2}deg:({:.4},{:.4})}}] ({:.4},{:.4}) ellipse ({:.4} and {:.4});\n",
                    e.angle.to_degrees(),
                    e.center.x, e.center.y,
                    e.center.x, e.center.y,
                    e.rx, e.ry
                ));
            }
            GeoObject::Text(txt) => {
                let content = escape_latex(&txt.content);
                tex.push_str(&format!(
                    "\\node at ({:.4},{:.4}) {{{}}};\n",
                    txt.position.x, txt.position.y, content
                ));
            }
            _ => {}
        }
    }

    tex.push_str("\\end{tikzpicture}\n");
    tex.push_str("\\end{document}\n");
    tex
}

fn escape_latex(s: &str) -> String {
    s.replace('\\', r"\textbackslash{}")
        .replace('{', r"\{")
        .replace('}', r"\}")
        .replace('$', r"\$")
        .replace('&', r"\&")
        .replace('#', r"\#")
        .replace('^', r"\textasciicircum{}")
        .replace('_', r"\_")
        .replace('~', r"\textasciitilde{}")
        .replace('%', r"\%")
}

fn draw_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if x >= 0 && x < img.width() as i32 && y >= 0 && y < img.height() as i32 {
            img.put_pixel(x as u32, y as u32, color);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn draw_circle_filled(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, color: Rgba<u8>) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                    img.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

fn draw_circle_outline(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, color: Rgba<u8>) {
    if r <= 0 {
        return;
    }
    let mut x = r;
    let mut y = 0;
    let mut err = 0;
    while x >= y {
        plot4(img, cx, cy, x, y, color);
        plot4(img, cx, cy, y, x, color);
        y += 1;
        if err <= 0 {
            err += 2 * y + 1;
        }
        if err > 0 {
            x -= 1;
            err -= 2 * x + 1;
        }
    }
}

fn plot4(img: &mut RgbaImage, cx: i32, cy: i32, dx: i32, dy: i32, color: Rgba<u8>) {
    for &(px, py) in &[
        (cx + dx, cy + dy),
        (cx - dx, cy + dy),
        (cx + dx, cy - dy),
        (cx - dx, cy - dy),
    ] {
        if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{Document, GeoObject, PointObj};

    #[test]
    fn test_export_latex_point_and_line() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(1.0, 2.0));
        doc.add_object(GeoObject::Point(p));
        let tex = export_latex(&doc);
        assert!(tex.contains("\\documentclass{standalone}"));
        assert!(tex.contains("\\begin{tikzpicture}"));
        assert!(tex.contains("(1.0000,2.0000)"));
        assert!(tex.contains("\\end{tikzpicture}"));
        assert!(tex.contains("\\end{document}"));
    }

    #[test]
    fn test_export_latex_escape() {
        assert_eq!(escape_latex("100%"), "100\\%");
        assert_eq!(escape_latex("a_b"), "a\\_b");
    }

    #[test]
    fn test_export_svg_basic() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(0.0, 0.0));
        doc.add_object(GeoObject::Point(p));
        let svg = export_svg(&doc, 800.0, 600.0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_export_png_no_panic() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(1.0, 1.0));
        doc.add_object(GeoObject::Point(p));
        let path = std::env::temp_dir().join("grafito_test_export.png");
        let result = export_png(&doc, 200, 200, path.to_str().unwrap());
        assert!(result.is_ok(), "export_png failed: {result:?}");
        assert!(path.exists());
    }
}
