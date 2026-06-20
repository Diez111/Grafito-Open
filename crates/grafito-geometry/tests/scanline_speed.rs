#[cfg(test)]
mod scanline_speed_tests {
    use grafito_geometry::expr::prepare_function_ast;
    use std::collections::HashMap;
    use std::time::Instant;

    /// Simula un scanline fill: para cada fila, evaluar f en N samples.
    /// `N` modela el stride: N=1 es el peor caso (1 sample por pixel),
    /// N=8 da 8x speedup con linear refinement entre samples.
    fn scanline_simulate<F: Fn(f64, f64) -> f64>(
        rows: usize,
        cols: usize,
        stride: usize,
        f: F,
    ) -> usize {
        let mut count = 0;
        for y in 0..rows {
            for x in (0..cols).step_by(stride) {
                let _ = f(x as f64, y as f64);
                count += 1;
            }
        }
        count
    }

    #[test]
    #[ignore]
    fn bench_scanline_speedup() {
        // Disco: x^2 + y^2 < 1 sobre canvas 1000x600.
        // AST: x^2 + y^2 (4 nodos).
        let ast = prepare_function_ast("x^2 + y^2", &HashMap::new(), &["x", "y"]).unwrap();
        let f = |x: f64, y: f64| ast.eval_2d("x", x, "y", y);
        let rows = 600;
        let cols = 1000;

        for stride in &[1, 2, 4, 8, 16] {
            let start = Instant::now();
            let count = scanline_simulate(rows, cols, *stride, &f);
            let elapsed = start.elapsed();
            let per_call = elapsed / count.max(1) as u32;
            println!(
                "stride={:2}: {} evals in {:?} ({:?}/eval)",
                stride, count, elapsed, per_call
            );
        }
    }

    #[test]
    #[ignore]
    fn bench_scanline_complex() {
        // Expresión compleja con ~20 nodos.
        let expr = "sin(x)*cos(y) + exp(-(x^2 + y^2)) * log(x^2 + y^2 + 1)";
        let ast = prepare_function_ast(expr, &HashMap::new(), &["x", "y"]).unwrap();
        let f = |x: f64, y: f64| ast.eval_2d("x", x, "y", y);
        let rows = 600;
        let cols = 1000;

        for stride in &[1, 2, 4, 8, 16] {
            let start = Instant::now();
            let count = scanline_simulate(rows, cols, *stride, &f);
            let elapsed = start.elapsed();
            let per_call = elapsed / count.max(1) as u32;
            println!(
                "complex stride={:2}: {} evals in {:?} ({:?}/eval)",
                stride, count, elapsed, per_call
            );
        }
    }
}
