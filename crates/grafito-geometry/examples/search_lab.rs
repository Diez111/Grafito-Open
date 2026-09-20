//! Lab de problemas abiertos — corrida CLI del loop TOPP 39 (Fase B).
//!
//! Uso: `cargo run -p grafito-geometry --example search_lab -- [n] [seeds]`
//! Default: n=10, seeds=0..64. Imprime:
//! 1) comparación de construcciones clásicas (grilla, triangular, polígono, azar);
//! 2) una línea JSONL por corrida verificada del loop aleatorio;
//! 3) el mejor al final, re-verificado por la doble puerta.
//!
//! Sin I/O de red, sin UI: solo el motor puro.

use grafito_geometry::search::{
    grid_point_set, measure_point_set, regular_polygon, seeded_point_set, topp39_best_of,
    triangular_lattice, verify_search_run,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
    let seeds_count: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(64);
    let _ = (n, seeds_count);
    match run(n, seeds_count) {
        Ok(()) => {}
        Err(code) => std::process::exit(code),
    }
}

fn run(n: usize, seeds_count: u64) -> Result<(), i32> {
    // 1) Construcciones deterministas comparables (mismo n).
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    for (nombre, set) in [
        ("grid", grid_point_set(rows, cols, 1.0).ok()),
        ("triangular", triangular_lattice(rows, cols, 1.0).ok()),
        ("polygon", regular_polygon(n.max(3), 1.0).ok()),
        ("random", seeded_point_set(0, n, 5.0).ok()),
    ] {
        match set.map(|pts| measure_point_set(&pts)) {
            Some(Ok(m)) => println!(
                "# {nombre:12} n={:<4} unit={:<4} distinct={:<4} hash={}",
                m.n, m.unit, m.distinct, m.hash
            ),
            Some(Err(e)) => eprintln!("# {nombre}: error honesto: {e}"),
            None => eprintln!("# {nombre}: sin conjunto"),
        }
    }

    // 2) Loop aleatorio verificado.
    let seeds: Vec<u64> = (0..seeds_count).collect();
    match topp39_best_of(&seeds, n, 5.0, 1e-9) {
        Ok((best, all)) => {
            println!(
                "# TOPP 39 loop: n={n}, seeds={seeds_count}, verificadas={}",
                all.len()
            );
            for run in &all {
                println!("{}", run.to_jsonl());
            }
            println!("# mejor: {}", best.to_jsonl());
            match verify_search_run(&best, 1e-9) {
                Ok(true) => {
                    println!("# doble puerta: OK");
                    Ok(())
                }
                Ok(false) => {
                    eprintln!("# doble puerta: el mejor NO verifica; descartado");
                    Err(2)
                }
                Err(e) => {
                    eprintln!("# doble puerta: error honesto: {e}");
                    Err(3)
                }
            }
        }
        Err(e) => {
            eprintln!("# loop abortado: {e}");
            Err(1)
        }
    }
}
