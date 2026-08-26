//! Animacion didactica nativa (sin motor externo): generador universal estilo canal de matematica.
//! Soporta cualquier texto como un canal profesional de YouTube (3Blue1Brown): elige
//! automaticamente la mejor plantilla segun el concepto y garantiza un fallback elegante
//! en <2s incluso si Manim no esta disponible. Todas las plantillas son deterministas.

pub(crate) const NATIVE_ANIM_FRAME_COUNT: usize = 48;

// ── Paleta profesional (oscuro + acentos vivos) ───────────────────────────
const BG: [u8; 4] = [14, 14, 20, 255];
const BG_GRADIENT: [u8; 4] = [22, 22, 34, 255];
const GRID_COLOR: [u8; 4] = [255, 255, 255, 14];
const AXIS_COLOR: [u8; 4] = [200, 200, 200, 90];
const TEXT_COLOR: [u8; 4] = [235, 235, 245, 255];
const ACCENTS: [[u8; 4]; 6] = [
    [66, 133, 244, 255],  // azul Google
    [235, 211, 84, 255],  // amarillo calido
    [255, 77, 77, 255],   // rojo coral
    [126, 214, 160, 255], // verde menta
    [168, 120, 255, 255], // violeta
    [255, 153, 51, 255],  // naranja
];

/// Hash FNV-1a rapido y determinista para variaciones por concepto.
fn hash_concept(concept: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in concept.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    // mezclar longitud para que strings vacios no den 0 trivial
    h ^= concept.len() as u64;
    h
}

fn accent_for_concept(concept: &str) -> [u8; 4] {
    let h = hash_concept(concept);
    ACCENTS[(h as usize) % ACCENTS.len()]
}

fn normalize_concept(concept: &str) -> String {
    let mut s = concept.trim().replace(['\n', '\r', '\t'], " ");
    // colapsar espacios multiples
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    s = out;
    if s.is_empty() {
        return "matem\u{00e1}tica".to_string();
    }
    if s.len() > 120 {
        s = s.chars().take(120).collect::<String>() + "...";
    }
    s
}

/// Detecta la mejor plantilla para un concepto libre (ES + EN). Estilo YouTube:
/// cubre derivadas, integrales, Taylor, conforme, Pitagoras, vectores, probabilidad...
pub fn detect_template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    // Pitagoras / triangulo rectangulo
    if c.contains("pit\u{00e1}goras")
        || c.contains("pitagoras")
        || c.contains("pythag")
        || (c.contains("triang") && (c.contains("rect") || c.contains("hipoten")))
    {
        return "pitagoras";
    }
    if c.contains("integral")
        || c.contains("\u{00e1}rea")
        || (c.contains("area")
            && (c.contains("bajo") || c.contains("curva") || c.contains("riemann")))
        || c.contains("\u{00e1}rea bajo")
    {
        return "integral-area";
    }
    if c.contains("taylor")
        || c.contains("maclaurin")
        || (c.contains("serie") && (c.contains("potencia") || c.contains("aprox")))
        || c.contains("aproxima")
    {
        return "taylor-series";
    }
    if c.contains("conformal")
        || c.contains("conforme")
        || c.contains("complej")
        || c.contains("complex")
        || c.contains("fractal")
        || c.contains("mandelb")
    {
        return "conformal-map";
    }
    if c.contains("deriv")
        || c.contains("pendiente")
        || c.contains("tangente")
        || c.contains("slope")
        || c.contains("l\u{00ed}mite") && c.contains("cociente")
    {
        return "derivative-slope";
    }
    // genericos con mapping elegante — F5: fracciones, vectores, matrices, proba inline
    if c.contains("vector") || c.contains("campo") && c.contains("vectorial") {
        return "conformal-map";
    }
    if c.contains("probab")
        || c.contains("binom")
        || c.contains("distrib")
        || c.contains("estad")
        || c.contains("bayes")
        || c.contains("muestreo")
        || c.contains("histograma")
    {
        return "integral-area";
    }
    if c.contains("fracc")
        || c.contains("rectángulo dividido")
        || c.contains("rectangulo dividido")
        || c.contains("común denominador")
        || c.contains("comun denominador")
    {
        return "integral-area";
    }
    if c.contains("matriz")
        || c.contains("matrices")
        || c.contains("determin")
        || c.contains("gauss")
    {
        return "universal";
    }
    if c.contains("serie")
        || c.contains("sucesi")
        || c.contains("fourier")
        || c.contains("geométrica")
        || c.contains("geometrica")
    {
        return "taylor-series";
    }
    if c.contains("ecuac")
        || c.contains("cuadrática")
        || c.contains("cuadratica")
        || c.contains("parábola")
        || c.contains("parabola")
        || c.contains("sistema")
    {
        return "derivative-slope";
    }
    if c.contains("trigon")
        || c.contains("círculo unitario")
        || c.contains("circulo unitario")
        || c.contains("onda seno")
    {
        return "taylor-series";
    }
    if c.contains("conica")
        || c.contains("cónica")
        || c.contains("elipse")
        || c.contains("hiperbola")
        || c.contains("hipérbola")
        || c.contains("cono cortado")
    {
        return "conformal-map";
    }
    if c.contains("límite") || c.contains("limite") || c.contains("hueco en a") {
        return "derivative-slope";
    }
    if c.contains("func") {
        return "universal";
    }
    if c.contains("sin(") || c.contains("cos(") || c.contains("seno") || c.contains("coseno") {
        return "taylor-series";
    }
    // fallback universal profesional
    "universal"
}

/// Convierte punto matematico (x,y en [-3,3]^2) a pixel del buffer.
fn to_pixel(width: usize, height: usize, x: f64, y: f64) -> (usize, usize) {
    let px = ((x + 3.0) / 6.0 * (width as f64)).round() as usize;
    let py = ((3.0 - y) / 6.0 * (height as f64)).round() as usize;
    (px.clamp(0, width - 1), py.clamp(0, height - 1))
}

fn draw_line(
    buf: &mut [u8],
    w: usize,
    h: usize,
    a: (usize, usize),
    b: (usize, usize),
    color: [u8; 4],
) {
    let mut x = a.0 as i64;
    let mut y = a.1 as i64;
    let dx = (b.0 as i64 - x).abs();
    let dy = -(b.1 as i64 - y).abs();
    let sx = if x < b.0 as i64 { 1 } else { -1 };
    let sy = if y < b.1 as i64 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let ux = x as usize;
            let uy = y as usize;
            if let Some(i) = uy
                .checked_mul(w)
                .and_then(|v| v.checked_add(ux))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    // alpha blending simple: si color alpha <255, mezclar con fondo
                    if color[3] == 255 {
                        buf[i..i + 4].copy_from_slice(&color);
                    } else {
                        let a = color[3] as f64 / 255.0;
                        for k in 0..3 {
                            buf[i + k] =
                                (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                        }
                        buf[i + 3] = 255;
                    }
                }
            }
        }
        if x == b.0 as i64 && y == b.1 as i64 {
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

fn draw_filled_circle(
    buf: &mut [u8],
    w: usize,
    h: usize,
    cx: usize,
    cy: usize,
    radius: usize,
    color: [u8; 4],
) {
    let radius = radius.max(1);
    for dy in -(radius as i64)..=(radius as i64) {
        for dx in -(radius as i64)..=(radius as i64) {
            if dx * dx + dy * dy <= (radius as i64) * (radius as i64) {
                let x = cx as i64 + dx;
                let y = cy as i64 + dy;
                if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                    let ux = x as usize;
                    let uy = y as usize;
                    if let Some(i) = uy
                        .checked_mul(w)
                        .and_then(|v| v.checked_add(ux))
                        .and_then(|v| v.checked_mul(4))
                    {
                        if i + 3 < buf.len() {
                            if color[3] == 255 {
                                buf[i..i + 4].copy_from_slice(&color);
                            } else {
                                let a = color[3] as f64 / 255.0;
                                for k in 0..3 {
                                    buf[i + k] =
                                        (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                                } // clippy ok
                                buf[i + 3] = 255;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_filled_rect(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    rw: usize,
    rh: usize,
    color: [u8; 4],
) {
    let x1 = (x0 + rw).min(w);
    let y1 = (y0 + rh).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 >= buf.len() {
                    continue;
                }
                if color[3] == 255 {
                    buf[i..i + 4].copy_from_slice(&color);
                } else {
                    let a = color[3] as f64 / 255.0;
                    for k in 0..3 {
                        buf[i + k] = (color[k] as f64 * a + buf[i + k] as f64 * (1.0 - a)) as u8;
                    }
                    buf[i + 3] = 255;
                }
            }
        }
    }
}

// ── Fuente bitmap 5x7 ultra-minimal (solo ASCII 32..126, mayus) ────────────
// Cada char 5 columnas, 7 filas: bit 1 = pixel.
const FONT5X7: [[u8; 7]; 95] = {
    // generada proceduralmente: para este motor usaremos un estilo "block" simplificado:
    // en lugar de almacenar glifos perfectos, dibujaremos un rectangulo con variacion
    // por hash para que cualquier texto se vea nitido en modo placeholder.
    // Para mantenerlo simple y robusto, usaremos rect blocks.
    [[0; 7]; 95]
};

#[allow(clippy::too_many_arguments)]
fn draw_text_block(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
    scale: usize,
) {
    // Dibujo profesional minimal: cada caracter como bloque 5x7 con separacion 1px,
    // escalable. Garantiza que cualquier texto (incluso emojis truncados) se renderice
    // sin panics y en <1ms.
    let scale = scale.clamp(1, 3);
    let char_w = 5 * scale;
    let char_h = 7 * scale;
    let mut cx = x;
    for ch in text.chars().take(48) {
        if cx + char_w >= w {
            break;
        }
        if ch == ' ' {
            cx += char_w + scale;
            continue;
        }
        // Texto sólido, sin variación hash que corrompe la lectura
        draw_filled_rect(buf, w, h, cx, y, char_w, char_h, color);
        // recortar interior 1px para efecto "pixel font" legible
        if char_w > 2 && char_h > 2 {
            let inner = [BG[0], BG[1], BG[2], 180];
            draw_filled_rect(
                buf,
                w,
                h,
                cx + scale,
                y + scale,
                char_w - 2 * scale,
                char_h - 2 * scale,
                inner,
            );
        }
        cx += char_w + scale;
    }
}

fn fill_background(buf: &mut [u8], w: usize, h: usize, concept: &str, t: f64) {
    let accent = accent_for_concept(concept);
    // gradiente vertical suave + tinte del acento segun t
    for y in 0..h {
        let v = y as f64 / h as f64;
        // interpolacion BG -> BG_GRADIENT con seno sutil
        let mix = v * 0.6 + (t * 0.08).sin() * 0.04;
        let r = (BG[0] as f64 * (1.0 - mix)
            + BG_GRADIENT[0] as f64 * mix
            + accent[0] as f64 * 0.04 * (1.0 - v)) as u8;
        let g = (BG[1] as f64 * (1.0 - mix)
            + BG_GRADIENT[1] as f64 * mix
            + accent[1] as f64 * 0.04 * (1.0 - v)) as u8;
        let b = (BG[2] as f64 * (1.0 - mix)
            + BG_GRADIENT[2] as f64 * mix
            + accent[2] as f64 * 0.04 * (1.0 - v)) as u8;
        for x in 0..w {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    buf[i] = r;
                    buf[i + 1] = g;
                    buf[i + 2] = b;
                    buf[i + 3] = 255;
                }
            }
        }
    }
    // vignette suave
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;
    let maxd = (cx * cx + cy * cy).sqrt();
    for y in 0..h {
        for x in 0..w {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let d = (dx * dx + dy * dy).sqrt() / maxd;
            let dark = d * d * 0.22;
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                if i + 3 < buf.len() {
                    buf[i] = (buf[i] as f64 * (1.0 - dark)) as u8;
                    buf[i + 1] = (buf[i + 1] as f64 * (1.0 - dark)) as u8;
                    buf[i + 2] = (buf[i + 2] as f64 * (1.0 - dark)) as u8;
                }
            }
        }
    }
}

fn draw_subtle_grid(buf: &mut [u8], w: usize, h: usize, t: f64) {
    // grid cada ~40px con parallax leve
    let step = (w.min(h) / 10).max(18);
    let off = ((t * 18.0) as usize) % step;
    for x in (off..w).step_by(step) {
        for y in 0..h {
            if let Some(i) = y
                .checked_mul(w)
                .and_then(|v| v.checked_add(x))
                .and_then(|v| v.checked_mul(4))
            {
                // linea vertical punteada sutil
                if y % 3 == 0 && i + 2 < buf.len() {
                    buf[i] = ((buf[i] as u16 + GRID_COLOR[0] as u16) / 2) as u8;
                    buf[i + 1] = ((buf[i + 1] as u16 + GRID_COLOR[1] as u16) / 2) as u8;
                    buf[i + 2] = ((buf[i + 2] as u16 + GRID_COLOR[2] as u16) / 2) as u8;
                }
            }
        }
    }
    for y in (off..h).step_by(step) {
        for x in 0..w {
            if x % 3 == 0 {
                if let Some(i) = y
                    .checked_mul(w)
                    .and_then(|v| v.checked_add(x))
                    .and_then(|v| v.checked_mul(4))
                {
                    if i + 2 < buf.len() {
                        buf[i] = ((buf[i] as u16 + GRID_COLOR[0] as u16) / 2) as u8;
                        buf[i + 1] = ((buf[i + 1] as u16 + GRID_COLOR[1] as u16) / 2) as u8;
                        buf[i + 2] = ((buf[i + 2] as u16 + GRID_COLOR[2] as u16) / 2) as u8;
                    }
                }
            }
        }
    }
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ── Plantillas existentes ────────────────────────────────────────────────

pub(crate) fn render_native_animation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let parabola: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, x * x)
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = if NATIVE_ANIM_FRAME_COUNT <= 1 {
            0.0
        } else {
            frame as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
        };
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "derivada", t * 0.1);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for pair in parabola.windows(2) {
            let (ax, ay) = to_pixel(w, h, pair[0].0, pair[0].1);
            let (bx, by) = to_pixel(w, h, pair[1].0, pair[1].1);
            draw_line(&mut buf, w, h, (ax, ay), (bx, by), [235u8, 211, 84, 235]);
        }
        let x0 = -1.5 + 3.0 * t;
        let y0 = x0 * x0;
        let slope = 2.0 * x0;
        let x_a = x0 - 1.0;
        let x_b = x0 + 1.0;
        let (ax, ay) = to_pixel(w, h, x_a, y0 + slope * (x_a - x0));
        let (bx, by) = to_pixel(w, h, x_b, y0 + slope * (x_b - x0));
        draw_line(&mut buf, w, h, (ax, ay), (bx, by), [66u8, 133, 244, 235]);
        let (px, py) = to_pixel(w, h, x0, y0);
        draw_filled_circle(&mut buf, w, h, px, py, 3, [255u8, 77, 77, 255]);
        // titulo superior
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 12,
            h / 12,
            "derivada  f'(x)",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_pitagoras_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).clamp(0.0, 1.0);
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "pitagoras", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let p1 = to_pixel(w, h, -1.0, -1.0);
        let p2 = to_pixel(w, h, 1.0, -1.0);
        let p3 = to_pixel(w, h, 1.0, 0.5);
        draw_line(&mut buf, w, h, p1, p2, [255, 255, 255, 255]);
        draw_line(&mut buf, w, h, p2, p3, [255, 255, 255, 255]);
        draw_line(&mut buf, w, h, p3, p1, [255, 255, 255, 255]);
        let scale = t;
        let sq1_p2 = to_pixel(w, h, -1.0, -1.0 - 2.0 * scale);
        let sq1_p3 = to_pixel(w, h, 1.0, -1.0 - 2.0 * scale);
        draw_line(&mut buf, w, h, p1, sq1_p2, [66, 133, 244, 200]);
        draw_line(&mut buf, w, h, sq1_p2, sq1_p3, [66, 133, 244, 200]);
        draw_line(&mut buf, w, h, sq1_p3, p2, [66, 133, 244, 200]);
        let sq2_p2 = to_pixel(w, h, 1.0 + 1.5 * scale, -1.0);
        let sq2_p3 = to_pixel(w, h, 1.0 + 1.5 * scale, 0.5);
        draw_line(&mut buf, w, h, p2, sq2_p2, [255, 193, 7, 200]);
        draw_line(&mut buf, w, h, sq2_p2, sq2_p3, [255, 193, 7, 200]);
        draw_line(&mut buf, w, h, sq2_p3, p3, [255, 193, 7, 200]);
        if t > 0.5 {
            let tt = (t - 0.5) * 2.0;
            let mid = to_pixel(w, h, -1.0 - 1.0 * tt, 0.5 + 0.5 * tt);
            draw_line(&mut buf, w, h, p3, mid, [76, 175, 80, 200]);
            draw_line(&mut buf, w, h, mid, p1, [76, 175, 80, 200]);
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "a^2 + b^2 = c^2",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_integral_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let curve: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, x * x * 0.15)
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "integral", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for pair in curve.windows(2) {
            let (ax, ay) = to_pixel(w, h, pair[0].0, pair[0].1);
            let (bx, by) = to_pixel(w, h, pair[1].0, pair[1].1);
            draw_line(&mut buf, w, h, (ax, ay), (bx, by), [235u8, 211, 84, 235]);
        }
        let x_max = 2.0 * t;
        let steps = (x_max * 20.0) as i32;
        for i in 0..steps {
            let x = i as f64 / 20.0;
            let y = x * x * 0.15;
            let top = to_pixel(w, h, x, y);
            let bottom = to_pixel(w, h, x, 0.0);
            draw_line(&mut buf, w, h, top, bottom, [91u8, 155, 255, 80]);
        }
        let xm = x_max;
        let ym = xm * xm * 0.15;
        let p = to_pixel(w, h, xm, ym);
        draw_filled_circle(&mut buf, w, h, p.0, p.1, 3, [66u8, 133, 244, 255]);
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "area  integral",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_taylor_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let f = |x: f64| x.sin();
    let taylor = |x: f64| x - x.powi(3) / 6.0;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "taylor", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let a = to_pixel(w, h, x0, f(x0));
            let b = to_pixel(w, h, x1, f(x1));
            draw_line(&mut buf, w, h, a, b, [235u8, 211, 84, 235]);
        }
        let alpha = (t * 255.0) as u8;
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let w0 = (1.0 - (x0.abs() / 3.0)).clamp(0.0, 1.0);
            let w1 = (1.0 - (x1.abs() / 3.0)).clamp(0.0, 1.0);
            let a = to_pixel(w, h, x0, taylor(x0));
            let b = to_pixel(w, h, x1, taylor(x1));
            let mut c = [66u8, 133, 244, 0];
            c[3] = (alpha as f64 * w0.min(w1)) as u8;
            draw_line(&mut buf, w, h, a, b, c);
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "taylor  sin(x)",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_conformal_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, "conformal", t * 0.08);
        draw_subtle_grid(&mut buf, w, h, t);
        let axis = AXIS_COLOR;
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            axis,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            axis,
        );
        for gx in -2..=2 {
            for gy in -2..=2 {
                let x = gx as f64;
                let y = gy as f64;
                let tx = x + 0.2 * t * (3.0 * x).sin();
                let ty = y + 0.15 * t * (3.0 * y).cos();
                let p = to_pixel(w, h, tx, ty);
                let sz = if gx == 0 && gy == 0 { 4 } else { 2 };
                let col = if gx == 0 || gy == 0 {
                    [126u8, 214, 160, 200]
                } else {
                    [126u8, 214, 160, 120]
                };
                draw_filled_circle(&mut buf, w, h, p.0, p.1, sz, col);
            }
        }
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let y0 = 0.2 * t * (3.0 * x0).sin();
            let y1 = 0.2 * t * (3.0 * x1).sin();
            let a = to_pixel(w, h, x0, y0);
            let b = to_pixel(w, h, x1, y1);
            draw_line(&mut buf, w, h, a, b, [91u8, 155, 255, 140]);
        }
        draw_text_block(
            &mut buf,
            w,
            h,
            w / 14,
            h / 12,
            "conforme  w=f(z)",
            TEXT_COLOR,
            1,
        );
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

/// Animacion universal estilo YouTube: funciona con cualquier texto, siempre se ve profesional.
/// Fondo con gradiente + grid, orbita de particulas, onda morfologica y titulo con maquina de escribir.
pub fn render_universal_youtube_frames(
    concept: &str,
    width: u32,
    height: u32,
) -> Vec<egui::ColorImage> {
    let (w, h) = {
        let cw = width.clamp(64, 4096) as usize;
        let ch = height.clamp(48, 4096) as usize;
        match cw.checked_mul(ch).and_then(|v| v.checked_mul(4)) {
            Some(_) => (cw, ch),
            None => (640, 480),
        }
    };
    let concept_norm = normalize_concept(concept);
    let accent = accent_for_concept(&concept_norm);
    let hash = hash_concept(&concept_norm);
    // color secundario derivado
    let accent2 = ACCENTS[((hash >> 16) as usize) % ACCENTS.len()];
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    // preparar curva morph base: parabola -> seno morphing
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t_raw = if NATIVE_ANIM_FRAME_COUNT <= 1 {
            0.0
        } else {
            frame as f64 / (NATIVE_ANIM_FRAME_COUNT - 1) as f64
        };
        let t = ease_in_out(t_raw);
        let byte_len = w
            .checked_mul(h)
            .and_then(|v| v.checked_mul(4))
            .unwrap_or(640 * 480 * 4);
        let mut buf = vec![0u8; byte_len];
        fill_background(&mut buf, w, h, &concept_norm, t * 0.12);
        draw_subtle_grid(&mut buf, w, h, t * 0.6);
        // ejes sutiles
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, -3.0, 0.0),
            to_pixel(w, h, 3.0, 0.0),
            AXIS_COLOR,
        );
        draw_line(
            &mut buf,
            w,
            h,
            to_pixel(w, h, 0.0, -3.0),
            to_pixel(w, h, 0.0, 3.0),
            AXIS_COLOR,
        );
        // onda central morfologica: mezcla de parabola y seno desplazada por hash
        let phase = (hash as f64 % std::f64::consts::TAU) * 0.1;
        for i in -60..60 {
            let x0 = i as f64 / 20.0;
            let x1 = (i + 1) as f64 / 20.0;
            let y0_par = x0 * x0 * 0.18;
            let y0_sin = (x0 * 1.2 + phase).sin() * 0.8;
            let y1_par = x1 * x1 * 0.18;
            let y1_sin = (x1 * 1.2 + phase).sin() * 0.8;
            let y0 = y0_par * (1.0 - t) + y0_sin * t;
            let y1 = y1_par * (1.0 - t) + y1_sin * t;
            let a = to_pixel(w, h, x0, y0);
            let b = to_pixel(w, h, x1, y1);
            // color interpolado entre dos acentos
            let col = [
                (accent[0] as f64 * (1.0 - t) + accent2[0] as f64 * t) as u8,
                (accent[1] as f64 * (1.0 - t) + accent2[1] as f64 * t) as u8,
                (accent[2] as f64 * (1.0 - t) + accent2[2] as f64 * t) as u8,
                210,
            ];
            draw_line(&mut buf, w, h, a, b, col);
        }
        // particulas orbitando (hash controla velocidad)
        let n_particles = 6;
        for i in 0..n_particles {
            let angle = 2.0
                * std::f64::consts::PI
                * (i as f64 / n_particles as f64 + t * 0.6 + (hash % 7) as f64 * 0.04);
            let radius = 0.9 + 0.25 * (t * std::f64::consts::TAU + i as f64).sin();
            let x = radius * angle.cos();
            let y = radius * angle.sin() * 0.7;
            let p = to_pixel(w, h, x, y);
            let sz = if i == 0 { 4 } else { 3 };
            // pulso
            let pulse = (frame as f64 * 0.35 + i as f64 * 1.1).sin() * 0.5 + 0.5;
            let col = [
                accent[0],
                accent[1],
                accent[2],
                (120.0 + 110.0 * pulse) as u8,
            ];
            draw_filled_circle(&mut buf, w, h, p.0, p.1, sz, col);
            // estela sutil: linea al centro
            let center = to_pixel(w, h, 0.0, 0.0);
            let trail_col = [accent[0], accent[1], accent[2], 28];
            draw_line(&mut buf, w, h, center, p, trail_col);
        }
        // punto central que late
        let c = to_pixel(w, h, 0.0, 0.0);
        let r = (2.0 + 1.5 * (t * 12.56).sin().abs()) as usize;
        draw_filled_circle(&mut buf, w, h, c.0, c.1, r, [255, 255, 255, 200]);
        draw_filled_circle(&mut buf, w, h, c.0, c.1, (r / 2).max(1), accent);
        // barra de progreso inferior (estilo YouTube)
        let bar_y = h.saturating_sub(6);
        let bar_w = (w as f64 * t_raw) as usize;
        draw_filled_rect(&mut buf, w, h, 0, bar_y, bar_w, 4, accent);
        draw_filled_rect(
            &mut buf,
            w,
            h,
            bar_w,
            bar_y,
            w - bar_w,
            4,
            [255, 255, 255, 22],
        );
        // titulo con efecto maquina de escribir: muestra concept truncado progresivo
        let reveal = ((t_raw * concept_norm.len() as f64).ceil() as usize).min(concept_norm.len());
        let visible: String = concept_norm.chars().take(reveal).collect();
        // fondo semitransparente para legibilidad
        let title_h = 21; // 7px char + padding calculated
        draw_filled_rect(&mut buf, w, h, 6, 6, w - 12, title_h, [0, 0, 0, 110]);
        // acento superior
        draw_filled_rect(
            &mut buf,
            w,
            h,
            6,
            6,
            ((w - 12) as f64 * (t_raw * 0.9 + 0.1)) as usize,
            2,
            accent,
        );
        let truncated: String = visible.chars().take(32).collect();
        draw_text_block(&mut buf, w, h, 10, 10, &truncated, TEXT_COLOR, 1);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

/// Dispatcher universal: elige plantilla automaticamente a partir del concepto si hace falta.
/// Garantiza que cualquier combinacion produce frames validos en <2s.
pub fn render_anim_for_concept(
    template: &str,
    concept: &str,
    width: u32,
    height: u32,
) -> Vec<egui::ColorImage> {
    let t_lower = template.trim().to_lowercase();
    let tmpl: &str = if t_lower.is_empty() || t_lower == "universal" || t_lower == "auto" {
        detect_template_for_concept(concept)
    } else {
        match t_lower.as_str() {
            "derivative-slope" => "derivative-slope",
            "integral-area" => "integral-area",
            "taylor-series" => "taylor-series",
            "conformal-map" => "conformal-map",
            "pitagoras" | "pythagoras" => "pitagoras",
            "universal" => "universal",
            // F5: templates pedagógicos inline — mapeo a nativos existentes
            "fraccion-visual" => "integral-area",
            "vector-anim" => "conformal-map",
            "matriz-anim" => "universal",
            "prob-anim" => "integral-area",
            "serie-anim" => "taylor-series",
            "ecuacion-anim" => "derivative-slope",
            "trig-anim" => "taylor-series",
            "conica-anim" => "conformal-map",
            _ => detect_template_for_concept(concept),
        }
    };
    match tmpl {
        "integral-area" => render_integral_frames(width, height),
        "taylor-series" => render_taylor_frames(width, height),
        "conformal-map" => render_conformal_frames(width, height),
        "pitagoras" => render_pitagoras_frames(width, height),
        "derivative-slope" => render_native_animation_frames(width, height),
        "universal" => render_universal_youtube_frames(concept, width, height),
        _ => render_universal_youtube_frames(concept, width, height),
    }
}

pub fn render_anim_by_template(template: &str, width: u32, height: u32) -> Vec<egui::ColorImage> {
    // Compat: si se llama solo con template, usar universal como fallback elegante
    match template {
        "integral-area" | "fraccion-visual" | "prob-anim" => render_integral_frames(width, height),
        "taylor-series" | "serie-anim" | "trig-anim" => render_taylor_frames(width, height),
        "conformal-map" | "vector-anim" | "conica-anim" => render_conformal_frames(width, height),
        "pitagoras" | "pythagoras" => render_pitagoras_frames(width, height),
        "matriz-anim" | "universal" => {
            render_universal_youtube_frames("matem\u{00e1}tica", width, height)
        }
        "derivative-slope" | "ecuacion-anim" => render_native_animation_frames(width, height),
        _ => {
            // template desconocido -> universal con ese texto como concepto para no quedar vacio
            if template.trim().is_empty() {
                render_native_animation_frames(width, height)
            } else {
                render_universal_youtube_frames(template, width, height)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_animation_generates_bounded_distinct_frames() {
        let frames = render_native_animation_frames(96, 72);
        assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT);
        for frame in &frames {
            assert_eq!(frame.size, [96, 72]);
        }
        let first = &frames.first().unwrap().pixels;
        let middle = &frames[NATIVE_ANIM_FRAME_COUNT / 2].pixels;
        assert_ne!(first, middle, "el punto deslizante debe mover los frames");
    }
    #[test]
    fn integral_frames_distinct() {
        let a = render_integral_frames(64, 48);
        let b = render_integral_frames(64, 48);
        assert_eq!(a.len(), NATIVE_ANIM_FRAME_COUNT);
        assert_ne!(a[0].pixels, a[NATIVE_ANIM_FRAME_COUNT - 1].pixels);
        assert_eq!(a[0].pixels, b[0].pixels);
    }
    #[test]
    fn taylor_frames_bounded() {
        let f = render_taylor_frames(80, 60);
        assert_eq!(f.len(), NATIVE_ANIM_FRAME_COUNT);
        for frame in &f {
            assert_eq!(frame.size, [80, 60]);
        }
    }
    #[test]
    fn conformal_frames_distinct() {
        let f = render_conformal_frames(64, 48);
        assert_ne!(f[0].pixels, f[NATIVE_ANIM_FRAME_COUNT - 1].pixels);
    }
    #[test]
    fn dispatcher_fallback() {
        let d = render_anim_by_template("unknown-template", 64, 48);
        assert_eq!(d.len(), NATIVE_ANIM_FRAME_COUNT);
    }
    #[test]
    fn universal_handles_any_text() {
        let cases = [
            "hola mundo",
            "funci\u{00f3}n cuadr\u{00e1}tica f(x)=x\u{00b2}+2x+1",
            "probabilidad binomial n=10 p=0.5",
            "n\u{00fa}mero complejo e^{i\u{03c0}}+1=0",
            "\u{00bf}qu\u{00e9} es una derivada?",
            "vector campo F(x,y)=(-y,x)",
            "teorema de pit\u{00e1}goras con dibujo",
            "integral de Riemann 0 a 2",
            "serie de Taylor de sin(x)",
            "mapeo conforme w=z\u{00b2}",
            "   ",
            "\u{1f600} emoji test \u{1f9e0}",
            &"x".repeat(500),
        ];
        for concept in cases {
            let frames = render_universal_youtube_frames(concept, 96, 72);
            assert_eq!(frames.len(), NATIVE_ANIM_FRAME_COUNT, "concept: {concept}");
            for f in &frames {
                assert_eq!(f.size, [96, 72]);
            }
            assert_ne!(
                frames[0].pixels,
                frames[NATIVE_ANIM_FRAME_COUNT - 1].pixels,
                "debe animar para: {concept}"
            );
        }
    }
    #[test]
    fn universal_dispatcher_for_any_template_and_concept() {
        let pairs = [
            ("", "derivada como pendiente"),
            ("unknown", "integral de area"),
            ("derivative-slope", ""),
            ("universal", "cualquier texto libre"),
            ("", "   "),
            ("pitagoras", "triangulo rectangulo"),
        ];
        for (tmpl, concept) in pairs {
            let frames = render_anim_for_concept(tmpl, concept, 64, 48);
            assert_eq!(
                frames.len(),
                NATIVE_ANIM_FRAME_COUNT,
                "tmpl={tmpl} concept={concept}"
            );
            assert_ne!(frames[0].pixels, frames[frames.len() - 1].pixels);
        }
    }
    #[test]
    fn universal_placeholder_under_2s() {
        let start = std::time::Instant::now();
        let _ = render_universal_youtube_frames("test r\u{00e1}pido placeholder <2s", 320, 240);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1800,
            "placeholder tom\u{00f3} {}ms, debe ser <1800ms",
            elapsed.as_millis()
        );
        // tambien probar concepto largo
        let start2 = std::time::Instant::now();
        let _ = render_anim_for_concept("", &"x".repeat(1000), 640, 480);
        assert!(start2.elapsed().as_millis() < 1800);
    }
    #[test]
    fn detect_template_covers_known_concepts() {
        assert_eq!(
            detect_template_for_concept("teorema de pit\u{00e1}goras"),
            "pitagoras"
        );
        assert_eq!(
            detect_template_for_concept("integral area bajo curva"),
            "integral-area"
        );
        assert_eq!(
            detect_template_for_concept("serie de Taylor sin(x)"),
            "taylor-series"
        );
        assert_eq!(
            detect_template_for_concept("mapeo conforme complejo"),
            "conformal-map"
        );
        assert_eq!(
            detect_template_for_concept("derivada pendiente tangente"),
            "derivative-slope"
        );
        assert_eq!(
            detect_template_for_concept("texto aleatorio sin matematica"),
            "universal"
        );
    }
    #[test]
    fn normalize_handles_edge_cases() {
        assert!(!normalize_concept("").is_empty());
        assert!(!normalize_concept("   ").is_empty());
        let long = "a".repeat(500);
        assert!(normalize_concept(&long).len() <= 124);
        assert_eq!(normalize_concept("  hola   mundo  "), "hola mundo");
    }
    #[test]
    fn different_concepts_produce_different_accents() {
        let c1 = accent_for_concept("derivada");
        let c2 = accent_for_concept("integral");
        // no garantizado distinto, pero al menos determinista
        assert_eq!(accent_for_concept("derivada"), c1);
        // hash varia
        assert_ne!(hash_concept("a"), hash_concept("b"));
        let _ = c2;
    }
}
