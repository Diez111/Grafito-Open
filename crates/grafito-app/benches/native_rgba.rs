//! Bench RGBA por plantilla nativa (11): tiempo + presupuestos.
//!
//! Sin criterion (no es dependencia del workspace): son `#[test]`s
//! temporizados con `Instant` que imprimen `ms` por plantilla y pinean los
//! budgets (GIF 64 frames / 8 M px / 5 MB, set nativo 64 MiB).
//!
//! Corridas:
//! - `cargo test -p grafito-app --bench native_rgba -- --nocapture` (tabla)
//! - `cargo bench -p grafito-app --bench native_rgba -- --test` (harness libtest)
//!
//! Usa SOLO la API pública re-exportada (`render_anim_by_template` +
//! consts); el módulo `anim_native` sigue privado. Tamaño chico (160×120)
//! para que el bench mida draw, no allocs gigantes.

use grafito_app::{
    estimate_frames_bytes, render_anim_by_template, GIF_EXPORT_MAX_FILE_BYTES,
    GIF_EXPORT_MAX_FRAMES, GIF_EXPORT_MAX_TOTAL_PIXELS, NATIVE_ANIM_FRAME_COUNT,
    NATIVE_FRAME_BYTES_ESTIMADO_640X480, NATIVE_MAX_SET_BYTES, NATIVE_TEMPLATES,
};
use std::time::Instant;

/// Viewport del bench (sobre el mínimo 64, bajo el clamp de 64 MiB).
const BENCH_W: u32 = 160;
/// Alto del viewport del bench.
const BENCH_H: u32 = 120;

/// Renderiza una plantilla, pinnea 48 frames + bytes bajo el tope e imprime.
fn mide_plantilla(template: &str) {
    let t0 = Instant::now();
    let frames = render_anim_by_template(template, BENCH_W, BENCH_H);
    let dt = t0.elapsed();
    assert_eq!(
        frames.len(),
        NATIVE_ANIM_FRAME_COUNT,
        "set completo de 48 para {template}"
    );
    assert!(!frames.is_empty(), "sin frames para {template}");
    let (w, h) = (frames[0].size[0], frames[0].size[1]);
    let bytes = match estimate_frames_bytes(w, h, frames.len()) {
        Some(b) => b,
        None => panic!("overflow estimando bytes de {template}"),
    };
    assert!(
        bytes <= NATIVE_MAX_SET_BYTES,
        "{template}: {bytes} bytes excede el tope de {NATIVE_MAX_SET_BYTES}"
    );
    println!(
        "rgba {template}: {w}x{h}x{} en {dt:?} ({bytes} bytes)",
        frames.len()
    );
}

#[test]
fn plantillas_son_11() {
    assert_eq!(
        NATIVE_TEMPLATES,
        &[
            "derivative-slope",
            "integral-area",
            "taylor-series",
            "conformal-map",
            "pitagoras",
            "euler",
            "fourier",
            "logistic-bifurcation",
            "gradient-field",
            "mobius-transform",
            "universal",
        ]
    );
}

#[test]
fn rgba_derivative_slope() {
    mide_plantilla("derivative-slope");
}

#[test]
fn rgba_integral_area() {
    mide_plantilla("integral-area");
}

#[test]
fn rgba_taylor_series() {
    mide_plantilla("taylor-series");
}

#[test]
fn rgba_conformal_map() {
    mide_plantilla("conformal-map");
}

#[test]
fn rgba_pitagoras() {
    mide_plantilla("pitagoras");
}

#[test]
fn rgba_euler() {
    mide_plantilla("euler");
}

#[test]
fn rgba_fourier() {
    mide_plantilla("fourier");
}

#[test]
fn rgba_logistic_bifurcation() {
    mide_plantilla("logistic-bifurcation");
}

#[test]
fn rgba_gradient_field() {
    mide_plantilla("gradient-field");
}

#[test]
fn rgba_mobius_transform() {
    mide_plantilla("mobius-transform");
}

#[test]
fn rgba_universal() {
    mide_plantilla("universal");
}

#[test]
fn presupuestos_gif_y_set_pinneados() {
    assert_eq!(GIF_EXPORT_MAX_FRAMES, 64, "tope GIF 64 frames");
    assert_eq!(
        GIF_EXPORT_MAX_TOTAL_PIXELS, 8_000_000,
        "tope GIF 8 M píxeles"
    );
    assert_eq!(GIF_EXPORT_MAX_FILE_BYTES, 5 * 1024 * 1024, "tope GIF 5 MB");
    assert_eq!(
        NATIVE_MAX_SET_BYTES,
        64 * 1024 * 1024,
        "tope set nativo 64 MiB"
    );
    assert_eq!(NATIVE_ANIM_FRAME_COUNT, 48, "set canónico de 48");
    // El número documentado del set canónico 640×480×48 RGBA.
    assert_eq!(NATIVE_FRAME_BYTES_ESTIMADO_640X480, 58_982_400);
    assert_eq!(
        estimate_frames_bytes(640, 480, NATIVE_ANIM_FRAME_COUNT),
        Some(58_982_400)
    );
    // 4096²×48 RGBA (≈3 GiB) no entra: el render clásico hace clamp, nunca OOM.
    let gigante = estimate_frames_bytes(4096, 4096, NATIVE_ANIM_FRAME_COUNT);
    assert!(
        matches!(gigante, Some(b) if b > NATIVE_MAX_SET_BYTES),
        "4096²×48 debe exceder el tope, got: {gigante:?}"
    );
}
