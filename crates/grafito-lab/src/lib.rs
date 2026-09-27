//! `grafito-lab`: Hadwiger-Nelson lab en Rust puro.
//!
//! Port de `lab/*.py` (parsers `.pt`/`.edge`, shrink, gadgets, CNF) sin
//! Python. Lo ya cubierto por `grafito-geometry::search`
//! (generadores seeded/grid/triangular, `unit_graph_edges`,
//! `export_dimacs_kcoloring`, `run_topp39_scan`, `verify_search_run`) y por
//! `grafito-mcp::sat` (runner kissat/cadical) se reutiliza, no se duplica.
//! Cada módulo cita su origen `.py` y pineea paridad con fixtures en
//! `tests/`.

pub mod edge;
pub mod gadget;
pub mod points;
pub mod shrink;
