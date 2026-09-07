#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! F5 — bench INFORMATIVO (sin gate) del seam nativo de morph (F2a).
//!
//! Mide `PolylineMorph::frames_puntos()` — el pipeline puro que
//! `render_morph_frames` (grafito-app, `pub(crate)`) consume 1:1
//! (1 frame = 1 polilinea en mundo). Dos talles: chico (16 muestras,
//! 5 frames, igual al test `morph_lineal`) y completo (64 muestras,
//! 48 frames = `NATIVE_ANIM_FRAME_COUNT`).
//!
//! BLOQUEADOR honesto: el render RGBA (`render_morph_frames` /
//! `render_integral_frames` a `egui::ColorImage`) vive tras
//! `pub(crate) mod anim_native` y `crates/grafito-app/src/lib.rs` esta sucio
//! (F1-F4, no tocable en F5); cuando se abra la visibilidad, agregar
//! `crates/grafito-app/benches/native_rgba.rs` con 64×64.
//!
//! Criterio de aceptacion F5: corre con
//! `cargo bench -p grafito-anim --bench native_frames -- --test`, imprime su
//! numero base y NO bloquea CI. Sin `unwrap` en prod (solo bench).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_anim::parametric::{FrameCount, PolylineMorph, ShapeEasing};

fn cuadrada() -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn triangulo() -> Vec<[f64; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]
}

fn bench_native_morph(c: &mut Criterion) {
    // Numero base impreso (visible con `-- --test --nocapture` y en el log CI).
    let base = PolylineMorph::try_new(
        cuadrada(),
        triangulo(),
        16,
        FrameCount::try_new(5).expect("5 frames validos"),
        false,
        true,
        ShapeEasing::Linear,
    )
    .expect("morph base valido");
    let puntos = base.frames_puntos().expect("puntos base validos");
    println!(
        "F5-base native_morph: frames={} samples_por_frame={} puntos_totales={}",
        puntos.len(),
        puntos.first().map_or(0, Vec::len),
        puntos.iter().map(Vec::len).sum::<usize>()
    );
    assert_eq!(puntos.len(), 5);

    c.bench_function("morph_chico_16x5", |b| {
        b.iter(|| {
            let morph = PolylineMorph::try_new(
                black_box(cuadrada()),
                black_box(triangulo()),
                black_box(16),
                FrameCount::try_new(5).expect("frames validos"),
                false,
                true,
                ShapeEasing::Linear,
            )
            .expect("morph valido");
            morph.frames_puntos().expect("puntos validos")
        });
    });

    c.bench_function("morph_completo_64x48", |b| {
        b.iter(|| {
            let morph = PolylineMorph::try_new(
                black_box(cuadrada()),
                black_box(triangulo()),
                black_box(64),
                FrameCount::try_new(48).expect("frames validos"),
                false,
                true,
                ShapeEasing::CubicInOut,
            )
            .expect("morph valido");
            morph.frames_puntos().expect("puntos validos")
        });
    });
}

criterion_group!(benches, bench_native_morph);
criterion_main!(benches);
