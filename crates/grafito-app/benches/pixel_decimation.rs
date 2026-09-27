#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Throughput + igualdad de diezmado por píxel y batch de strokes.
//!
//! Sin criterion (no es dev-dep de grafito-app) y sin tocar `render_2d.rs`,
//! que otros agentes están optimizando en paralelo: es harness libtest como
//! `native_rgba.rs`, con escenas sintéticas de curvas trig densas.
//!
//! - `decimate_to_pixel_buckets`: referencia del contrato visual — cada bucket
//!   de 1px conserva su envolvente (min/max); `diezmo_conserva_envolvente`
//!   lo aserta por bucket y pineea primer/último punto.
//! - `batch_strokes` vs `individual_strokes`: `batch_igual_a_individuales`
//!   aserta mismos vértices + mismos rangos.
//! - `throughput_*`: imprimen ms y Kpts/s (determinismo por asserts, el tiempo
//!   solo se reporta, nunca se aserta).
//!
//! Corridas:
//! - `cargo test -p grafito-app --bench pixel_decimation -- --nocapture` (tabla)
//! - `cargo bench -p grafito-app --bench pixel_decimation -- --test`

use grafito_geometry::Point2;
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::time::Instant;

/// Escena sintética: curva trig densa de `n` puntos en `[0, 4π]`
/// (`sin(37t) + 0.5·sin(101t)`: dos frecuencias no conmensuradas).
fn dense_trig_scene(point_count: usize) -> Vec<Point2> {
    (0..point_count)
        .map(|i| {
            let t = i as f64 / point_count as f64 * TAU * 2.0;
            Point2::new(t, (37.0 * t).sin() + 0.5 * (101.0 * t).sin())
        })
        .collect()
}

/// Ancho de bucket en mundo para 1 píxel a `scale` px/unidad.
fn bucket_width_for_scale(scale: f64) -> f64 {
    1.0 / scale.max(1e-6)
}

/// Diezmado por bucket de píxel que conserva la envolvente visual.
///
/// Por cada bucket no vacío emite min-y y max-y en orden de aparición (1 solo
/// punto si coinciden), más el primer y último punto globales. Contrato que el
/// optimizador debe preservar: min/max por bucket idénticos a la entrada.
fn decimate_to_pixel_buckets(points: &[Point2], bucket_width: f64) -> Vec<Point2> {
    if points.is_empty() || !bucket_width.is_finite() || bucket_width <= 0.0 {
        return points.to_vec();
    }
    let x0 = points[0].x;
    let mut decimated = Vec::new();
    let mut bucket = isize::MIN;
    let mut min_index = 0usize;
    let mut max_index = 0usize;
    let mut open = false;
    // Vacía el bucket abierto emitiendo su envolvente en orden de aparición.
    let flush = |decimated: &mut Vec<Point2>, min_index: usize, max_index: usize| {
        if min_index <= max_index {
            decimated.push(points[min_index]);
            if max_index != min_index {
                decimated.push(points[max_index]);
            }
        } else {
            decimated.push(points[max_index]);
            decimated.push(points[min_index]);
        }
    };
    for (i, point) in points.iter().enumerate() {
        let current = ((point.x - x0) / bucket_width).floor() as isize;
        if !open {
            bucket = current;
            min_index = i;
            max_index = i;
            open = true;
        } else if current != bucket {
            flush(&mut decimated, min_index, max_index);
            bucket = current;
            min_index = i;
            max_index = i;
        } else {
            if point.y < points[min_index].y {
                min_index = i;
            }
            if point.y > points[max_index].y {
                max_index = i;
            }
        }
    }
    if open {
        flush(&mut decimated, min_index, max_index);
    }
    if decimated.first() != points.first() {
        decimated.insert(0, points[0]);
    }
    if decimated.last() != points.last() {
        decimated.push(points[points.len() - 1]);
    }
    decimated
}

/// Batch de strokes: un solo buffer + rangos `(inicio, fin)` por stroke.
fn batch_strokes(strokes: &[Vec<Point2>]) -> (Vec<Point2>, Vec<(usize, usize)>) {
    let total: usize = strokes.iter().map(Vec::len).sum();
    let mut vertices = Vec::with_capacity(total);
    let mut ranges = Vec::with_capacity(strokes.len());
    for stroke in strokes {
        let start = vertices.len();
        vertices.extend_from_slice(stroke);
        ranges.push((start, vertices.len()));
    }
    (vertices, ranges)
}

/// Runs individuales: un buffer por stroke, luego concatenados. Debe dar lo
/// mismo que [`batch_strokes`] vértice a vértice.
fn individual_strokes(strokes: &[Vec<Point2>]) -> (Vec<Point2>, Vec<(usize, usize)>) {
    let buffers: Vec<Vec<Point2>> = strokes.to_vec();
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    for buffer in &buffers {
        let start = vertices.len();
        vertices.extend_from_slice(buffer);
        ranges.push((start, vertices.len()));
    }
    (vertices, ranges)
}

/// 200 strokes sintéticos de 500 pts (fases trig distintas por stroke).
fn synthetic_strokes(stroke_count: usize, points_per_stroke: usize) -> Vec<Vec<Point2>> {
    (0..stroke_count)
        .map(|k| {
            let phase = k as f64 * 0.37;
            (0..points_per_stroke)
                .map(|i| {
                    let t = i as f64 / points_per_stroke as f64 * TAU;
                    Point2::new(t, phase.sin() * (3.0 * t).sin() + (7.0 * t + phase).cos())
                })
                .collect()
        })
        .collect()
}

fn bucket_of(x: f64, x0: f64, bucket_width: f64) -> isize {
    ((x - x0) / bucket_width).floor() as isize
}

#[test]
fn diezmo_conserva_envolvente_por_bucket() {
    let points = dense_trig_scene(20_000);
    let bucket_width = bucket_width_for_scale(50.0);
    let decimated = decimate_to_pixel_buckets(&points, bucket_width);
    assert!(
        decimated.len() <= points.len(),
        "el diezmo no debe crecer: {} > 20000",
        decimated.len()
    );
    assert_eq!(decimated.first(), points.first(), "pineea el inicio");
    assert_eq!(decimated.last(), points.last(), "pineea el fin");

    let x0 = points[0].x;
    let mut full: HashMap<isize, (f64, f64)> = HashMap::new();
    for point in &points {
        let entry = full
            .entry(bucket_of(point.x, x0, bucket_width))
            .or_insert((point.y, point.y));
        entry.0 = entry.0.min(point.y);
        entry.1 = entry.1.max(point.y);
    }
    let mut diet: HashMap<isize, (f64, f64)> = HashMap::new();
    for point in &decimated {
        let entry = diet
            .entry(bucket_of(point.x, x0, bucket_width))
            .or_insert((point.y, point.y));
        entry.0 = entry.0.min(point.y);
        entry.1 = entry.1.max(point.y);
    }
    for (bucket, (min_y, max_y)) in &full {
        let got = diet.get(bucket);
        assert_eq!(
            got,
            Some(&(*min_y, *max_y)),
            "el bucket {bucket} debe conservar su envolvente"
        );
    }
}

#[test]
fn batch_igual_a_runs_individuales() {
    let strokes = synthetic_strokes(200, 500);
    let (batched_vertices, batched_ranges) = batch_strokes(&strokes);
    let (single_vertices, single_ranges) = individual_strokes(&strokes);
    assert_eq!(
        batched_vertices, single_vertices,
        "el batch debe emitir los mismos vértices que los runs individuales"
    );
    assert_eq!(
        batched_ranges, single_ranges,
        "el batch debe emitir los mismos rangos que los runs individuales"
    );
    assert_eq!(batched_vertices.len(), 200 * 500);
    assert_eq!(batched_ranges.len(), 200);
    for (i, (start, end)) in batched_ranges.iter().enumerate() {
        assert_eq!(
            (*start, *end),
            (i * 500, (i + 1) * 500),
            "rango del stroke {i}"
        );
    }
}

#[test]
fn throughput_diezmo_20k() {
    let points = dense_trig_scene(20_000);
    let bucket_width = bucket_width_for_scale(50.0);
    let t0 = Instant::now();
    let decimated = decimate_to_pixel_buckets(&points, bucket_width);
    let dt = t0.elapsed();
    println!(
        "diezmo 20k trig → {} pts en {dt:?} ({:.0} Kpts/s entrada)",
        decimated.len(),
        20_000.0 / dt.as_secs_f64() / 1000.0
    );
    assert!(!decimated.is_empty());
}

#[test]
fn throughput_batch_200x500() {
    let strokes = synthetic_strokes(200, 500);
    let t0 = Instant::now();
    let (vertices, ranges) = batch_strokes(&strokes);
    let dt = t0.elapsed();
    println!(
        "batch 200×500 → {} vértices/{} rangos en {dt:?} ({:.0} Kpts/s)",
        vertices.len(),
        ranges.len(),
        100_000.0 / dt.as_secs_f64() / 1000.0
    );
    assert_eq!(vertices.len(), 100_000);
}
