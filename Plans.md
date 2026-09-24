# Plan: Ola 0 — Cableado, bugs de visibilidad y micro-optimizaciones

Fecha: 2026-09-23. Fuente: inventario de capacidades (mapa completo 2026-09-23)
+ gap analysis contra MathHook (mathhook.org) y MathCore (crates.io/crates/mathcore).
Cero menciones previas de ambas fuentes en el repo: nada estaba integrado.

## Motivo

La feature estrella de MathHook (step-by-step educativo) ya existe en Grafito y
NO está cableada. Además 7 comandos ejecutables no aparecen en la interfaz
(pausleta/docs/MCP) y hay motores presentes sin comando de usuario.

## Regla de arquitectura (fija para toda ola futura)

**UNA sola representación de expresión.** Todo algoritmo nuevo se implementa
sobre `Expr`/bytecode de Grafito y reusa `evaluate_cached` -> `compile_flat_ops`
-> `run_opcodes_flat` (`crates/grafito-geometry/src/expr.rs:1001,2238,1958`).
Nunca portar la representación `Expression` (32B hash-consed) de MathHook ni el
`Expr` boxeado de MathCore. Motivo: 87.6 ns vs 7.4 µs ya medidos
(`docs/profiling.md:21-36`) + evita 3 tipos de expresión en mantenimiento.

## Contrato de API entre agentes (bloqueante, no negociable)

`crates/grafito-geometry` expone (o hace `pub`):

```rust
pub fn poly_gcd_subresultant(a: Vec<f64>, b: Vec<f64>) -> Vec<f64>;   // symbolic.rs:2572 (hoy privado)
pub struct BiPoly;                                                     // solve.rs:773  (hoy privado)
pub fn sylvester_resultant(f1: &BiPoly, f2: &BiPoly, m: usize, n: usize)
    -> Option<Vec<f64>>;                                               // solve.rs:996  (hoy privado)
```

Ya son `pub` (no tocar firmas):
- `cas.rs:1463` `laurent_residue`
- `cas.rs:1586` `laurent_principal_part`
- `cas_steps.rs:1568` `steps_for_op(op: &CasOp) -> Result<Vec<CasStep>, String>`

## Ola 0 — tareas

### A. `grafito-geometry` (cerebro-audit)
1. `pub` en `poly_gcd_subresultant`, `BiPoly`, `sylvester_resultant` (contrato).
2. Dedupe `taylor_series`: `symbolic.rs:5533` (-> Result<String,String>) es la
   canónica; `matrices.rs:876` (-> Option<String>) delega o se elimina.
3. Dedupe financieras: `cas_extra.rs:771-807` vs `stats_extra.rs:319-357`.
4. Honestidad `Interval::new(_prec: u32, ...)`: el `prec` se ignora
   (`interval.rs:13`) -> usarlo o quitarlo, sin mentir.
5. Micro-opt con criterio: `MAX_COMPILED_EXPR_CACHE = 128` (`expr.rs:978`) es
   chico para un CAS -> medir 512/1024 con `expr_bench` antes de subir.

### B. `grafito-command` (general)
1. Registrar los 7 comandos FANTASMA (tienen handler, 0 apariciones en el
   registro -> invisibles en paleta/docs/MCP): `ImproperIntegral`, `SeriesSum`,
   `SequenceLimit`, `DoubleIntegral`, `LagrangeMultipliers`, `MeanValueCheck`,
   `SubspaceSum`.
2. Comandos nuevos + handlers: `PolyGCD`, `Resultant`, `Residue`,
   `PrincipalPart`, `StepByStep`.
3. Actualizar blindaje `registry_counts_match_documented_architecture`
   (`command_registry.rs:9904`).
4. Regenerar `docs/commands.md` vía `render_markdown()`.

### C. `grafito-mcp` + `grafito-assistant` (general)
1. `math_tool_schemas` (hoy 8) + `residue`, `principal_part`, `poly_gcd`,
   `resultant`, `steps`. Actualizar pins 8->13 y 23->28.
2. MCP las toma solas vía proxy (`bridge.rs:31-52`). Actualizar pin
   `tools.len() == 37` (`protocol.rs:448`).

### D. Núcleo numérico (perf-profiler)
1. Medir ANTES de optimizar (regla `docs/profiling.md`).
2. Ámbito: `grafito-core/**`, `grafito-complex/**`.
3. Levers candidatos: `lto = "thin"` -> `"fat"` (`Cargo.toml:120`), perfil PGO.

## Fuera de alcance explícito (olas 1-6)

EDPs/EDO simbólicas de MathHook, Fourier, parsers LaTeX/Wolfram,
n-ésima derivada, parciales, u-du, impropias calculadas, Gruntz real,
Laurent completa, sumas cerradas, no-conmutativo/cuaterniones, matrices
simbólicas, F4/F5, Lambert W, bigint/bigrational, FFT, BFGS, Gauss-Kronrod,
intervalos dirigidos, especiales avanzadas. Ver gap analysis 2026-09-23.

## Gates

`cargo fmt --all -- --check` ·
`cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo test --workspace --locked` ·
`cargo check --workspace --locked`
