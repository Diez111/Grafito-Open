//! Animación didáctica nativa (sin motor externo): recta tangente deslizante
//! sobre una parábola. Genera frames rasterizados que el chat reproduce.

pub(crate) const NATIVE_ANIM_FRAME_COUNT: usize = 48;

/// Convierte punto matemático (x,y en [-3,3]^2) a píxel del buffer.
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
            let i = (y as usize * w + x as usize) * 4;
            buf[i..i + 4].copy_from_slice(&color);
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

pub(crate) fn render_native_animation_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let w = width.max(64) as usize;
    let h = height.max(48) as usize;
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
        let mut buf = vec![0u8; w * h * 4];
        // Fondo transparente + ejes.
        let axis = [200u8, 200, 200, 90];
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
        // Curva.
        for pair in parabola.windows(2) {
            let (ax, ay) = to_pixel(w, h, pair[0].0, pair[0].1);
            let (bx, by) = to_pixel(w, h, pair[1].0, pair[1].1);
            draw_line(&mut buf, w, h, (ax, ay), (bx, by), [235u8, 211, 84, 235]);
        }
        // Tangente deslizante en x0 = -1.5 + 3*t.
        let x0 = -1.5 + 3.0 * t;
        let y0 = x0 * x0;
        let slope = 2.0 * x0;
        let x_a = x0 - 1.0;
        let x_b = x0 + 1.0;
        let (ax, ay) = to_pixel(w, h, x_a, y0 + slope * (x_a - x0));
        let (bx, by) = to_pixel(w, h, x_b, y0 + slope * (x_b - x0));
        draw_line(&mut buf, w, h, (ax, ay), (bx, by), [66u8, 133, 244, 235]);
        // Punto sobre la curva.
        let (px, py) = to_pixel(w, h, x0, y0);
        draw_filled_circle(&mut buf, w, h, px, py, 3, [255u8, 77, 77, 255]);
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
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
                    let i = (y as usize * w + x as usize) * 4;
                    buf[i..i + 4].copy_from_slice(&color);
                }
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
}
