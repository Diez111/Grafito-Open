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

    // Pineo de presupuestos (corre en modo --test y full): si alguien
    // cambia estos topes, el bench falla honesto en vez de regresión
    // silenciosa. GIF 64/8M/5MB vive en `grafito-app` (fuera de scope
    // anim) y se verifica por lectura; aquí se pinea lo que anim posee.
    {
        use grafito_anim::parametric::{PARAMETRIC_MAX_BYTES, PARAMETRIC_MAX_FRAMES};
        use grafito_anim::protocol::{AnimDuration, Resolution};
        assert_eq!(PARAMETRIC_MAX_FRAMES, 48);
        assert!(FrameCount::try_new(48).is_ok());
        assert!(FrameCount::try_new(49).is_err());
        assert_eq!(PARAMETRIC_MAX_BYTES, 64 * 1024 * 1024);
        assert!(Resolution::try_new(64, 64).is_ok());
        assert!(Resolution::try_new(4096, 4096).is_ok());
        assert!(Resolution::try_new(63, 480).is_err());
        assert!(Resolution::try_new(4097, 480).is_err());
        assert!(AnimDuration::try_new(0.1).is_ok());
        assert!(AnimDuration::try_new(60.0).is_ok());
        assert!(AnimDuration::try_new(0.09).is_err());
        assert!(AnimDuration::try_new(60.1).is_err());
        println!("F5-presupuestos: frames=48 set=64MiB res=64..4096 dur=0.1..60s OK");
    }

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
