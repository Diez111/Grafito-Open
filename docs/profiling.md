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

### 2.5 RAM (Ola 4 + T2 2026-09-17)

- `HISTORY_FULL_FRAMES_MAX` 3→1 (los frames completos del asistente se
  retenían ×3; el replay válido es cap 1 — test `frames_compartidos_roundtrip_*`).
- Undo/redo: `redo_stack` con `mem::take` (sin clonar ~50 MiB por eje).
- `RetentionQueue` con tope por bytes (`RETENTION_MAX_BYTES` 32 MiB) además
  del tope por ítems.
- Pizarra: `MAX_WHITEBOARD_TOTAL_BYTES` 32 MiB (validación fail-closed).
- Buffers wgpu: shrink con histéresis ×4 y piso 1 MiB (`ensure_buffer_capacity`).
- Stream del asistente: acumulador cap 256 KiB con flag `truncated` (antes
  podía crecer sin cota si el proveedor no cortaba).
- **RSS medido E2E** (proceso real, `/proc/PID/status`): idle vacío 80 MiB
  (21 MiB anónimos = heap propio; resto file-mapped compartido). Startup
  cold ~2 s (splash con 3 gates reales GPU→escena→plugins, self-report
  919 ms). Generador de escena grande:
  `cargo run --release -p grafito-app --example generar_escena_grande`
  (2000 objetos, 1.3 MB JSON) + `grafito /tmp/escena_grande.json`.
- **Undo por gesto (T6)**: los sliders de panel pusheaban 1 `Document`
  entero por frame con cambio (un drag = ~100 pushes que evictaban el
  historial). Ahora `panel_gesture_begin/commit` (mismo patrón que los
  sliders del canvas): 1 entrada por gesto; teclado sigue inmediato.
  Test `gesto_slider_undo_coalescea_un_drag_en_una_sola_entrada`.
- **Presupuesto undo verificado con doc grande (T2)**:
  500 funciones + 200 pushes → pila ≤50 y bytes ≤50 MiB
  (test `t2_presupuesto_undo_acota_pila_con_documento_grande`).

## 2.6 Frame real en release (T1 2026-09-17, escena 300, 800×600)

Atribución por categoría (hit, min-de-7): 200 funciones etiquetadas
10 ms; +30 paramétricas / +20 vectores / +10 implícitas / +40 puntos
marginal ~0 (caches compartidos entre etapas del test). Escena 300:
miss 108 ms / hit 20 ms. Desglose del hit (200 funciones): pipeline
muestreo+refine+proyección 3.3 ms + labels 2.4 ms + shapes 3.6 ms.

- **Pipeline con scratch (V2)**: `refine_function_samples_into` +
  proyección escriben en buffers thread-local reutilizados (0 allocs en
  hit). A/B intercalado: V1 fresco 3.4 ms / V2 reuse 3.3 ms / V3+V5
  fusionado 4.6-4.8 ms. **La fusión en una pasada PIERDE ~35%** (causa
  microarquitectural no determinada; el dato manda): no fusionar.
- **Labels a bitmap (T7)**: los auto-labels (`F₃₉`) caían al camino ASCII
  por falta de glifos ₀-₉ en la fuente bitmap → teselado por frame.
  Agregados los 10 glifos (`tex_raster.rs`): 104 blits bitmap, 0 textos;
  `draw_auto_labels` ≈ `draw_con_etiquetas`. Pin end-to-end en el test.
- **Paramétricas/polar adaptativas**: steps 4000 fijos → grid del
  viewport (igual que funciones): 32k→6.4k vértices por curva; emisión
  por runs (`Shape::line` por run en sólido en vez de 1 shape por
  segmento: los caps redondos costaban 5×). Teselado 50 curvas: 79→24 ms.
- **Teselado domina**: 50 funciones 10 ms, 50 paramétricas 24 ms de
  teselado vs ~3-9 ms de draw CPU. El resto del frame está en egui/wgpu,
  fuera de nuestro código caliente.
- **Cobertura flat 100%** (test `flat_corpus_aula`, 50/50: 32 funciones,
  12 componentes paramétricos, 6 implícitas). SIMD sigue diferido: con
  transcendentales libm dominando el sampler, el techo es bajo y el
  riesgo alto; reabrir solo si el sampler domina un perfil real.

## 2.7 Presupuesto de vértices (crash real → fix)

Escena válida de 2000 objetos (bajo `MAX_OBJECT_COUNT` 5000) mataba el
proceso: `egui_vertex_buffer` de 329 MB > tope wgpu 256 MB (panic fatal).
Medición t1f (teselado headless real): 2000 funciones ≈ 13M vértices ≈
260 MB; escala ~5k vértices/función. Fix: `FRAME_VERTEX_BUDGET`
(6M vértices ≈ 120 MB, 2× de margen para chrome UI + atlas) con
`estimate_object_vertices` barato por objeto (grids conocidos, sin
muestrear); al excederse se trunca el plan (los primeros objetos siempre
se dibujan) y se muestra insignia honesta en el canvas en vez de
crashear. Tests: calibración (2000 excede, 300 entra con 2×),
truncado end-to-end, pin de cobertura estimador-vs-brazos-de-dibujo.
Hallazgo lateral del mismo trabajo: `ORDERED_VISIBLE_CACHE` y
`DISPLAY_OVERRIDE_CACHE` keyeaban solo por versión/label → dos
documentos con igual versión colisionaban (canvas erróneo al abrir un
archivo nuevo); ahora llevan `Document::cache_nonce` (fresco en
`new`/deserialize, preservado en `clone`) con test de regresión.

## 2.8 Texturas managed: gracia obligatoria (crash real → fix)

Segundo crash de la misma escena: `Queue::submit: Texture with
'egui_texid_Managed(N)' label has been destroyed`. Causa: los LRU de
texturas (`TEX_LABEL_TEXTURES` 64, `FRACTAL_TEXTURES` 8,
`COMPLEX_GRID_TEXTURES` 16) dropeaban el `TextureHandle` en el frame de
la evicción; con cientos de rótulos (auto-labels `F₃₉` ahora van a
bitmap por los glifos ₀-₉) el churn destruía texturas aún referenciadas
por el submit en vuelo. Fix: `GracefulTextureCache` (LRU + cola de
retiro con `TEXTURE_GRACE_FRAMES` 3, mismo patrón que
`FillTextureCacheStore`), tick por frame en `draw_objects`, y política
**sin-evictar** para rótulos (`try_insert` + cap 256 + fallback ASCII
cuando está lleno: sin churn no hay drops bajo presión). Tests:
`graceful_texture_cache_evicta_con_gracia_y_try_insert_no_churnea` y
`draw_tex_label_300_con_cache_llena_cae_a_ascii_sin_panico`.
Verificación E2E: la escena de 2000 objetos **abre y renderiza** (576
dibujados + insignia "1424 objetos omitidos") sin panic; la vista
guardada de esa escena (scale 50 con 1200 funciones oscilando más
rápido que el píxel) se ve como bandas por solapamiento de trazos —
alias visual esperable de la escena extrema, no un defecto del motor.

## 2.9 Binario: eframe sin backend glow (T5)

`eframe` default-features traía `glow`/`glutin` (backend OpenGL que la
app no usa: solo wgpu). `default-features = false` +
`["wgpu", "accesskit", "default_fonts", "wayland", "web_screen_reader",
"x11"]` saca del árbol `egui_glow`, `glutin`, `glutin-winit`,
`glutin_egl_sys`, `glutin_glx_sys`, `sctk-adwaita`, `cgl` (7 crates,
verificado en el diff del lock y `cargo tree -i egui_glow` = vacío).
`glow` sigue en el lock por `wgpu-hal` (backend GL de wgpu, no
instanciado). Binario release: 44.8 MB; smoke E2E con ventana real OK.
Sin delta de tamaño A/B exacto (el baseline no-PGO previo era de otro
commit); el argumento es de árbol de dependencias, no de MB.

## 2.10 Tokens de contexto (F24 2026-09-17): harness y producto

> Medición con `wc -c/4` sobre los archivos reales (estimador conservador
> chars/4) y `opencode debug config` para verificar el payload inyectado.

| Payload por request | Antes | Después | Cómo |
|---|---|---|---|
| Instructions opencode (5 archivos) | 20 049 tok | **3 543 tok** | `instructions` = AGENTS.md + `.jspace/WORKSPACE.md` + MEMORY.md; `docs/architecture.md` (10.7k) y `docs/SKILLS-CATALOG.md` pasan a on-demand (skill `grafito-architecture`, router `skills-catalog`); ledger comprimido (20 305 → 3 911 chars) con historial archivado en `docs/ledger-archive-2026-09.md`; MEMORY.md deduplicado (5 473 → 3 440 chars) |
| Tool schemas MCP | 6 servers (~40 tools) | 5 servers (~26 tools) | `filesystem` MCP `enabled:false` (redundante con read/write/glob/grep) |
| `small_model` (títulos/tareas livianas) | muse-spark-1.3-contributor (razonador) | `deepseek-v4.1-flash` | `opencode.json` + agentes `plan/token-saver/memory-keeper/orchestrator` |
| Watcher | `.opencode/skills/**` vigilado (67 MB) | ignorado | `watcher.ignore` con `.opencode/**` |

Producto (asistente de Grafito):

| Payload por request | Antes | Después | Cómo |
|---|---|---|---|
| System prompt remoto | ~1.7k tok | ~1.8k tok | +directiva de datos no confiables (`UNTRUSTED_DATA_DIRECTIVE`); sigue siendo prefix estable (cacheable por DeepSeek: hit ≈30× más barato, `api.deepseek.com/guides/kv_cache`) |
| Contexto de documento (200 objetos) | error `input exceeds` (fallaba la consulta) | **≤ 8 192 chars** con nota "… y N objetos omitidos" | `bounded_context_prompt` trunca por presupuesto ranking estable (test `documento_denso_se_recorta_por_presupuesto_sin_fallar`) |
| Historial 6×4096 + doc denso | `wire validate` fallaba | request válido acotado | test `historial_y_documento_densos_no_rompen_el_presupuesto` |
| Telemetría real | invisible | JSONL opt-in `GRAFITO_USAGE_LOG` | chars in/out + tokens reales (in/out/reasoning/cached) + ms + stale_context por turno |

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
- Tests del asistente **serializados** por un candado estático
  (`rate_limit_test_guard` en `src/lib.rs`, `src/agent.rs` y
  `tests/remote_transport.rs`): los stubs 429 dejan pausa real y, en
  paralelo, otro test podía fallar rápido antes de conectar su stub y
  colgarse en `join` (visto 2026-09-17). Un guard por test, helpers sin
  candado; 3 corridas 272/272 sin hang.

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
