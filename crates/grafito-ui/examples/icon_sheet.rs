//! Hoja de iconos headless para revisión visual (NO es parte de los gates).
//!
//! Dibuja iconos con el código real (`draw_icon`), tesela con el propio
//! egui y rasteriza los triángulos a PPM. Uso:
//! `cargo run -p grafito-ui --example icon_sheet && python3 -c
//! "from PIL import Image; Image.open('/tmp/opencode/icon-sheet.ppm').save('/tmp/opencode/icon-sheet.png')"`

use egui::epaint::Primitive;
use grafito_ui::icons::{draw_icon, Icon};

const CELL: usize = 96;
const COLS: usize = 5;

fn main() {
    let icons = [
        Icon::Send,
        Icon::Sparkles,
        Icon::Search,
        Icon::Copy,
        Icon::Paperclip,
    ];
    assert_eq!(icons.len(), COLS);
    // Fila chica (20px): legibilidad real como en el composer.
    let small = [Icon::Sparkles, Icon::Search, Icon::Send, Icon::Paperclip];
    let rows = 2usize;
    let width = COLS * CELL;
    let height = rows * CELL;
    // Fondo oscuro tipo panel.
    let mut pixels = vec![0u8; width * height * 3];
    for chunk in pixels.chunks_exact_mut(3) {
        chunk[0] = 0x24;
        chunk[1] = 0x28;
        chunk[2] = 0x30;
    }

    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (i, icon) in icons.iter().enumerate() {
                let rect = egui::Rect::from_min_size(
                    egui::pos2((i * CELL) as f32, 0.0),
                    egui::vec2(CELL as f32, CELL as f32),
                );
                let color = if *icon == Icon::Send {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(0x9a, 0xa3, 0xb2)
                };
                draw_icon(ui.painter(), rect, *icon, color);
            }
            // Fila chica: candidatos a 20px como en el composer.
            for (i, icon) in small.iter().enumerate() {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(
                        (i * CELL) as f32 + (CELL as f32 - 20.0) / 2.0,
                        CELL as f32 + (CELL as f32 - 20.0) / 2.0,
                    ),
                    egui::vec2(20.0, 20.0),
                );
                draw_icon(
                    ui.painter(),
                    rect,
                    *icon,
                    egui::Color32::from_rgb(0x9a, 0xa3, 0xb2),
                );
            }
        });
    });
    let prims = ctx.tessellate(output.shapes, 1.0);
    eprintln!(
        "prims={} screen={:?}",
        prims.len(),
        ctx.input(|i| i.screen_rect)
    );
    for prim in &prims {
        let egui::epaint::ClippedPrimitive {
            clip_rect,
            primitive,
        } = prim;

        let Primitive::Mesh(mesh) = primitive else {
            continue;
        };
        let (cx0, cy0, cx1, cy1) = (
            clip_rect.min.x as i32,
            clip_rect.min.y as i32,
            clip_rect.max.x.ceil() as i32,
            clip_rect.max.y.ceil() as i32,
        );
        for tri in mesh.indices.chunks_exact(3) {
            let v = [
                mesh.vertices[tri[0] as usize],
                mesh.vertices[tri[1] as usize],
                mesh.vertices[tri[2] as usize],
            ];
            let pts = [
                (v[0].pos.x, v[0].pos.y),
                (v[1].pos.x, v[1].pos.y),
                (v[2].pos.x, v[2].pos.y),
            ];
            let (mut min_x, mut max_x, mut min_y, mut max_y) =
                (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
            for (x, y) in pts {
                min_x = min_x.min(x.floor() as i32);
                max_x = max_x.max(x.ceil() as i32);
                min_y = min_y.min(y.floor() as i32);
                max_y = max_y.max(y.ceil() as i32);
            }
            let edge = |ax: f32, ay: f32, bx: f32, by: f32, cx: f32, cy: f32| {
                (cx - ax) * (by - ay) - (cy - ay) * (bx - ax)
            };
            let area = edge(pts[0].0, pts[0].1, pts[1].0, pts[1].1, pts[2].0, pts[2].1);
            if area == 0.0 {
                continue;
            }
            let col = v[0].color;
            for py in min_y.max(0).max(cy0)..=max_y.min(height as i32 - 1).min(cy1) {
                for px in min_x.max(0).max(cx0)..=max_x.min(width as i32 - 1).min(cx1) {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = edge(pts[1].0, pts[1].1, pts[2].0, pts[2].1, fx, fy);
                    let w1 = edge(pts[2].0, pts[2].1, pts[0].0, pts[0].1, fx, fy);
                    let w2 = edge(pts[0].0, pts[0].1, pts[1].0, pts[1].1, fx, fy);
                    let inside = if area > 0.0 {
                        w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
                    } else {
                        w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
                    };
                    if inside {
                        let at = ((py as usize) * width + px as usize) * 3;
                        // Sin alpha blending: los iconos son opacos sobre el fondo.
                        pixels[at] = col.r();
                        pixels[at + 1] = col.g();
                        pixels[at + 2] = col.b();
                    }
                }
            }
        }
    }

    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    ppm.extend_from_slice(&pixels);
    if let Err(error) = std::fs::write("/tmp/opencode/icon-sheet.ppm", ppm) {
        eprintln!("icon-sheet: no se pudo escribir el PPM: {error}");
        return;
    }
    println!("icon-sheet: {width}x{height}, {} primitivas", prims.len());
}
