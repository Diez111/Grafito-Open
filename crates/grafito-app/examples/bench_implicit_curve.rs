//! Benchmark the implicit-curve evaluator.
//!
//! Run with:
//!   cargo run --example bench_implicit_curve --release

use grafito_core::object::{ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;
use std::time::Instant;

fn main() {
    let ic = ImplicitCurveObj::new("x^3 + y^3", "3*x*y", RelationOperator::Eq);
    let vars = HashMap::new();
    let view_bounds = (-3.0, 3.0, -3.0, 3.0);
    let grid_size = 256;

    let iters = 20;
    let mut total_ms = 0.0;
    for _ in 0..iters {
        // Use a fresh object so each iteration pays the full evaluation cost.
        let fresh = ic.clone();
        let start = Instant::now();
        let guard = grafito_core::implicit_curve::segments_or_compute(
            &fresh,
            view_bounds,
            grid_size,
            &vars,
        );
        let segs = guard.iter().map(|(_, s)| s.len()).sum::<usize>();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        total_ms += elapsed;
        println!(
            "grid={}x{} segments={} time={:.3} ms",
            grid_size, grid_size, segs, elapsed
        );
    }
    println!(
        "average over {} runs: {:.3} ms",
        iters,
        total_ms / iters as f64
    );
}
