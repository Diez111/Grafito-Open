#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente P3 (H5): parseo markdown del transcript del asistente.
//!
//! El transcript llama `AssistantBlocksCache::blocks` por turno y por frame.
//! - `h5_chat_200_lines_cold`: parse total sin cache (primer frame / turno nuevo).
//! - `h5_chat_200_lines_hit`: mismo contenido repetido (debe reutilizar,
//!   sin re-parsear).
//! - `h5_chat_stream_extension`: el contenido crece con una línea (streaming:
//!   congela bloques cerrados y solo re-parsea el sufijo).
//!
//! Gate P3: cachear filas por turno solo si el `hit` no reutiliza o el
//! `cold` por frame es ≥10% del presupuesto de frame (1-2 ms).
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_ui::assistant::AssistantBlocksCache;
use std::time::Duration;

/// Chat de ~200 líneas: encabezados, tabla, código, math, bullets, citas.
fn chat_200_lines() -> String {
    let mut out = String::new();
    out.push_str("# Resultado\n\n");
    for i in 0..20 {
        out.push_str(&format!("- observación {i} con **negrita** y `código`\n"));
    }
    out.push_str("\n| x | f(x) | nota |\n| --- | --- | --- |\n");
    for i in 0..60 {
        out.push_str(&format!("| {i} | {} | texto largo {i} |\n", i * i));
    }
    out.push_str("\n```grafito\nFunction[sin(x)]\n```\n\n");
    out.push_str("$$\\frac{x^2}{2}$$\n\n");
    for i in 0..70 {
        out.push_str(&format!("Párrafo {i} con algo de texto para envolver.\n"));
    }
    for i in 0..25 {
        out.push_str(&format!("> cita {i}\n"));
    }
    out.push_str("\n1) Primero\n2) Después\n");
    out
}

fn bench_h5_cold(c: &mut Criterion) {
    let content = chat_200_lines();
    assert!(
        content.lines().count() >= 180,
        "el chat debe rondar 200 líneas"
    );
    c.bench_function("h5_chat_200_lines_cold", |b| {
        b.iter(|| {
            let mut cache = AssistantBlocksCache::default();
            let blocks = cache.blocks(black_box(&content));
            black_box(blocks.len());
        })
    });
}

fn bench_h5_hit(c: &mut Criterion) {
    let content = chat_200_lines();
    let mut cache = AssistantBlocksCache::default();
    let _ = cache.blocks(&content);
    c.bench_function("h5_chat_200_lines_hit", |b| {
        b.iter(|| {
            let blocks = cache.blocks(black_box(&content));
            black_box(blocks.len());
        })
    });
}

fn bench_h5_stream_extension(c: &mut Criterion) {
    let base = chat_200_lines();
    let extended = format!("{base}\nNueva línea de streaming.\n");
    c.bench_function("h5_chat_stream_extension", |b| {
        b.iter(|| {
            let mut cache = AssistantBlocksCache::default();
            let _ = cache.blocks(black_box(&base));
            let blocks = cache.blocks(black_box(&extended));
            black_box(blocks.len());
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(8));
    targets = bench_h5_cold, bench_h5_hit, bench_h5_stream_extension
}
criterion_main!(benches);
