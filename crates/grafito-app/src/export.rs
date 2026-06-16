//! File I/O: save/load documents and export images.

use anyhow::{Context, Result};
use grafito_core::Document;

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
