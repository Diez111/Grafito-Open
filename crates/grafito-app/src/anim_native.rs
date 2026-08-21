//! Animacion didactica nativa (sin motor externo): recta tangente deslizante
//! sobre una parabola. Genera frames rasterizados que el chat reproduce.

pub(crate) const NATIVE_ANIM_FRAME_COUNT: usize = 48;

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
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_pitagoras_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let w = width.max(64) as usize;
    let h = height.max(48) as usize;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).clamp(0.0, 1.0);
        let mut buf = vec![0u8; w * h * 4];
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
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_integral_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let w = width.max(64) as usize;
    let h = height.max(48) as usize;
    let curve: Vec<(f64, f64)> = (-60..=60)
        .map(|i| {
            let x = i as f64 / 20.0;
            (x, (x * x) * 0.15)
        })
        .collect();
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let mut buf = vec![0u8; w * h * 4];
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
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_taylor_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let w = width.max(64) as usize;
    let h = height.max(48) as usize;
    let f = |x: f64| x.sin();
    let taylor = |x: f64| x - x.powi(3) / 6.0;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let mut buf = vec![0u8; w * h * 4];
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
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub(crate) fn render_conformal_frames(width: u32, height: u32) -> Vec<egui::ColorImage> {
    let w = width.max(64) as usize;
    let h = height.max(48) as usize;
    let mut frames = Vec::with_capacity(NATIVE_ANIM_FRAME_COUNT);
    for frame in 0..NATIVE_ANIM_FRAME_COUNT {
        let t = frame as f64 / (NATIVE_ANIM_FRAME_COUNT as f64 - 1.0).max(1.0);
        let mut buf = vec![0u8; w * h * 4];
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
        frames.push(egui::ColorImage::from_rgba_unmultiplied([w, h], &buf));
    }
    frames
}

pub fn render_anim_by_template(template: &str, width: u32, height: u32) -> Vec<egui::ColorImage> {
    match template {
        "integral-area" => render_integral_frames(width, height),
        "taylor-series" => render_taylor_frames(width, height),
        "conformal-map" => render_conformal_frames(width, height),
        "pitagoras" | "pythagoras" => render_pitagoras_frames(width, height),
        _ => render_native_animation_frames(width, height),
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
        let n = render_native_animation_frames(64, 48);
        assert_eq!(d.len(), n.len());
    }
}
