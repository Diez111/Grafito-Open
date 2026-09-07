#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! F5 — bench INFORMATIVO (sin gate) del path SSE-truncado.
//!
//! Mide el parseo puro de deltas (`responses_sse_delta_text`) y la junta de un
//! cuerpo `text/event-stream` grande con marcador `response.incomplete`
//! (`collect_responses_sse_text` → `(texto_parcial, truncated=true)`).
//!
//! Presupuesto real (transporte, no este bench):
//! `RESPONSES_MAX_BODY_BYTES` = 256 KiB por request
//! (`crates/grafito-assistant/src/lib.rs:81`); el cuerpo SSE se drena por chunks
//! de 4 KiB y el texto final pasa por `completion_from_text` con
//! `max_output_chars`. Este bench solo costea el parseo, sin red.
//!
//! Criterio de aceptacion F5: el bench existe, corre con
//! `cargo bench -p grafito-assistant --bench sse_truncation -- --test`,
//! imprime su numero base (bytes/eventos/truncated) y NO bloquea CI
//! (job `bench-regression` informativo). Sin `unwrap` en prod (solo bench).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use grafito_assistant::{collect_responses_sse_text, responses_sse_delta_text};

/// Un evento delta tipico (~70 bytes de JSON).
const DELTA_EVENT: &str =
    r#"{"type":"response.output_text.delta","delta":"abcdefghijklmnopqrstuvwxyz0123"}"#;
/// Eventos por cuerpo sintetico: ~2000 × ~80 B/linea ≈ 160 KiB de deltas.
const N_EVENTS: usize = 2000;

/// Cuerpo SSE sintetico y determinista: `N` deltas + (opcional) marcador de
/// corte que fuerza la ruta parcial `truncated=true` sin mas texto despues.
fn build_sse_body(events: usize, with_incomplete: bool) -> String {
    let mut body = String::new();
    for _ in 0..events {
        body.push_str("data: ");
        body.push_str(DELTA_EVENT);
        body.push('\n');
    }
    if with_incomplete {
        body.push_str("data: {\"type\":\"response.incomplete\"}\n");
    }
    body.push_str("data: [DONE]\n");
    body
}

fn bench_sse_truncation(c: &mut Criterion) {
    // Numero base impreso (visible con `-- --test --nocapture` y en el log CI).
    let body = build_sse_body(N_EVENTS, true);
    let (partial, truncated) = collect_responses_sse_text(&body);
    println!(
        "F5-base sse_truncation: body_bytes={} events={} partial_chars={} truncated={}",
        body.len(),
        N_EVENTS,
        partial.chars().count(),
        truncated
    );
    assert!(
        truncated,
        "el marcador incomplete debe dar parcial truncado"
    );
    assert!(!partial.is_empty(), "el parcial debe conservar texto");

    let mut group = c.benchmark_group("sse_truncation");
    group.throughput(Throughput::Bytes(body.len() as u64));

    group.bench_function("delta_event_parse", |b| {
        b.iter(|| responses_sse_delta_text(black_box(DELTA_EVENT)));
    });

    group.bench_function("collect_2k_deltas_incomplete", |b| {
        b.iter(|| collect_responses_sse_text(black_box(&body)));
    });

    // Ruta completa sin corte: mismo costo, `truncated=false`.
    let full_body = build_sse_body(N_EVENTS, false);
    group.bench_function("collect_2k_deltas_complete", |b| {
        b.iter(|| collect_responses_sse_text(black_box(&full_body)));
    });
    group.finish();
}

criterion_group!(benches, bench_sse_truncation);
criterion_main!(benches);
