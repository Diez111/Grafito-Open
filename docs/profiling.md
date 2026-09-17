# docs/profiling.md — Medir antes de optimizar (Ola 5/6, F14-perf 2026-09-17)

> Regla del repo: ninguna optimización entra sin número antes/después. Este
> archivo es el índice de instrumentos, los resultados medidos en este box y
> las decisiones que salieron de ellos (incluidas las **negativas**).

## 1. Instrumentos

| Instrumento | Comando | Qué mide |
|---|---|---|
| Puffin (feature `profile`) | `cargo run --release -p grafito-app --features profile` + `puffin_viewer --url 127.0.0.1:8585` | Scopes por frame (`app_update`, `ui`, `constraints`…), flames en vivo |
| Criterion (geometry) | `cargo bench -p grafito-geometry --bench expr_bench` | Intérprete walk vs plano, sampler por evaluación |
| Criterion (CAS/ODE/matrices) | `cargo bench -p grafito-geometry --bench cas_matrices_bench --bench ode_bench` | Buchberger, Gruntz, RK4, `Matrix::mul` |
| Criterion (core/render) | `cargo bench -p grafito-core --bench document_scenarios` · `cargo bench -p grafito-render --bench render_scenarios` | Undo/estimación de bytes, build de geometría por escena |
| Frame CPU (miss/hit) | `cargo test -p grafito-app --lib frame_cpu_escena_referencia -- --nocapture` | Costo de armar `Frame` sin GPU: primera vez (miss de cachés) vs siguiente (hit) |
| GPU compute | `WGPU_BACKEND=vulkan GRAFITO_REQUIRE_GPU_TESTS=1 cargo test -p grafito-render --test gpu_compute` | Paridad CPU/GPU y slots async |
| PGO | `bash scripts/pgo.sh [--interactive] [--install]` | Build con perfil real (ver §5) |

## 2. Resultados medidos (2026-09-17, AMD Zen / RADV RENOIR, Rust 1.92)

### 2.1 Evaluación de expresiones (`expr_bench`)

| Bench | Antes | Después Ola 2 | Nota |
|---|---|---|---|
| `interpreted_eval` | 7.4 µs | — | AST + HashMap de vars |
| `compiled_eval` | 87.6 ns | — | bytecode + validación una vez (94×) |
| `walk_trivial_5000` | 57.8 µs | 57 µs | intérprete de árbol |
| `flat_trivial_5000` | 122.7 µs | **35.3 µs** | loop plano con pila en `thread_local` (sin `memset` de 2 KiB) |
| `walk_sin_poly_5000` | 299 µs | 303 µs | referencia walk |
| `flat_sin_poly_5000` | 361 µs | **256 µs** | loop plano, gana walk |

Lección: el loop plano inicial era más lento **solo por el `memset` de la
pila `[0.0; 256]`** (~15 ns/eval fijos). Con la pila por hilo reutilizada,
el plano gana en ambas cargas. `MaybeUninit` no compila en stable para
`[f64; N]`; el `thread_local` + `RefCell` es sound sin `unsafe` (no hay
reentrancia del intérprete y cada hilo rayon tiene su buffer).

### 2.2 Allocator: mimalloc — **negativo, revertido**

A/B con `cargo bench -p grafito-app --bench native_rgba -- --test` (13
plantillas, 160×120×48), feature `allocator-mimalloc` vs default (glibc):

| Plantilla | glibc | mimalloc | Δ |
|---|---|---|---|
| universal | 22.8 ms | 28.7 ms | +26% |
| pitagoras | 18.9 ms | 34.7 ms | +84% |
| fractal | 70.7 ms | 71.2 ms | +1% |
| logistic-bifurcation | 129.6 ms | 141.8 ms | +9% |

mimalloc **perdió o empató en 13/13**. Decisión: revertido (no queda en
`Cargo.toml` ni `Cargo.lock`); se documenta acá para no repetir el
experimento. Si algún día se re-mide, usar `--features allocator-mimalloc`
(la feature se borró; recrear desde este registro).

### 2.3 Latencia de cuadros — `desired_maximum_frame_latency: Some(1)`

Default wgpu: 2. Grafito pinta por demanda (repaints presupuestados 16/33/
150 ms) y lo que importa es input→fotón (lápiz, sliders, pan). Guía de
`egui-wgpu`: “Use `1` for low-latency, and `2` for high-throughput”.
Aplicado en `app.rs` (`WgpuConfiguration`). No hay bench sintético válido
para esto fuera de una sesión real; el cambio es de configuración, no de
código caliente, y se revierte en una línea si una sesión real lo empeora.

### 2.4 GPU async (Ola 3)

- Domain coloring: el shader ahora deriva los centros de celda desde
  uniforms (`DomainGrid`) — se eliminó el buffer `in_points` (2 MiB por
  dispatch de 500×500).
- Slots async (`PendingGpuComputeJob::{DomainColoring, Parametric, ParametricSurface, Vector, …}`):
  el frame no bloquea esperando el readback; el resultado se consume cuando
  está listo (máquina `DomainGridState`/`resolve_*_job`). Paridad con el
  camino sync cubierta por tests GPU (`gpu_compute.rs`).

### 2.5 RAM (Ola 4)

- `HISTORY_FULL_FRAMES_MAX` 3→1 (los frames completos del asistente se
  retenían ×3; el replay válido es cap 1 — test `frames_compartidos_roundtrip_*`).
- Undo/redo: `redo_stack` con `mem::take` (sin clonar ~50 MiB por eje).
- `RetentionQueue` con tope por bytes (`RETENTION_MAX_BYTES` 32 MiB) además
  del tope por ítems.
- Pizarra: `MAX_WHITEBOARD_TOTAL_BYTES` 32 MiB (validación fail-closed).
- Buffers wgpu: shrink con histéresis ×4 y piso 1 MiB (`ensure_buffer_capacity`).
- Stream del asistente: acumulador cap 256 KiB con flag `truncated` (antes
  podía crecer sin cota si el proveedor no cortaba).

## 3. Simd (`wide`) — decisión diferida

`wide 0.7.33` está en el lock solo como transitiva (simba/alkahest-cas);
`wide 1.x` choca con `deny.toml` (`multiple-versions = "deny"`). Además,
medido el loop plano escalar (§2.1) ya supera al walk en las cargas reales
del sampler. Vectorizar antes de tener un perfil que muestre el sampler como
cuello es optimización especulativa: **diferido** hasta que puffin/criterion
lo justifiquen. Si se retoma: f64x4 en `run_opcodes_flat` (los `Opcode` son
escalares; requiere empaquetar pares `PushVar`/`BinOp` en superops) y
respetar la paridad exacta de `trig_reduce` (senos grandes) bajo el test
adversarial ya existente.

## 4. Qué corre en CI

- `bench-regression` (semanal/manual, informativo): `cargo bench --workspace
  --benches --locked` compila **todos** los benches nuevos (`expr_bench`,
  `ode_bench`, `cas_matrices_bench`, `native_rgba`, `frame_cpu` via lib test)
  e imprime números base. No es gate de PR por diseño (flaky entre runners,
  sin `critcmp`; ver `docs/architecture.md` §9).
- `gpu-compute` (gate real): `WGPU_BACKEND=vulkan GRAFITO_REQUIRE_GPU_TESTS=1`.

## 5. PGO (`scripts/pgo.sh`)

```bash
bash scripts/pgo.sh               # instrumenta → entrena headless (render + lib tests) → merge → rebuild
bash scripts/pgo.sh --interactive # entrena con una sesión real de la GUI (mejor perfil)
bash scripts/pgo.sh --install     # además instala en /usr/local/bin/grafito
```

- Usa el `llvm-profdata` del toolchain Rust (misma LLVM 21.1.3 que rustc;
  `rustup component add llvm-tools-preview`). Fail-closed: si el merge no
  encuentra `.profraw`, corta.
- El entrenamiento ideal es `--interactive` (cubre pintado egui, que los
  tests headless no tocan). El headless cubre samplers/render/matemática.
- No cambia defaults del repo ni CI: el binario PGO queda en
  `target/release/grafito`.
