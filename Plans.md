# Plans.md — Auditoría E2E Completa Grafito v2
> **Slash Commands activos**: `/j-space` `/vibecoder-guide` `/statem` `/rust-design` `/rust-ui`
> **Reglas**: capas (Cerebro → Piel), estados tipados, type-safety, separación total UI/lógica

## Visión Ejecutiva (vibecoder-guide: en criollo)
Grafito es una **pizarra geométrica** que piensa (Cerebro en Rust puro) y dibuja (Piel en egui/wgpu).
- **Si el Cerebro miente**, la pizarra dibuja cualquier cosa.
- **Si la Piel toca el Cerebro**, se rompe la previsibilidad.
- **Si el Animador se cuelga**, toda la app se congela.

Objetivo de esta auditoría: revisar **TODO de inicio a fin**, pulir hasta dejarlo "a prueba de tontos",
y que el generador de animaciones sea **confiable, cancelable y seguro**.

## Mapa del Workspace (DAG real)
```
grafito-geometry  ──┐
grafito-complex   ──┼─> grafito-core ──> grafito-command ──> grafito-app
grafito-render    ──┘          │                    ▲
grafito-anim      ─────────────┼─> grafito-assistant┘
grafito-whiteboard ────────────┘
grafito-ui        ─────────────> grafito-app (Piel)
grafito-profile/pedagogy/plugins/assistant-types
```
- **Cerebro puro**: core, geometry, command, complex, whiteboard, profile, pedagogy, plugins, assistant
- **Puente**: anim (IPC JSON v1 stdio a Python/manim)
- **Piel**: ui (tokens, theme, assistant, animation), app (app.rs 4826L, assistant.rs 4731L, render_2d 4750L, panels 3177L, etc.)
- **Infra**: packaging/deb, .github/workflows/ci.yml (17 jobs)

## Principios Invariantes (CORE)
- **CORE-1 /j-space**: Nada de código sin que esté en Plans.md/Tasks.md/progress.md primero.
- **CORE-2 /statem**: Todo flujo con estados inválidos imposibles → `enum Estado` + transiciones tipadas que no compilan si son ilegales.
- **CORE-3 /rust-design**: newtype (AnimJobId, Resolution, AnimDuration), Result/Option en bordes, nunca unwrap en prod, clippy -D warnings = 0.
- **CORE-4 /rust-ui**: La UI es función pura `fn render(&Estado) -> Frame`. Cero I/O, cero spawn, cero lógica en el `Ui::`.
- **CORE-5 /vibecoder-guide**: Cada error de compilador → explicación en lenguaje de negocio + menú de 2-3 opciones.

## Fases por Capas (orden estricto)

### Fase 0 — Inventario & Baseline [✅ DONE parcial, refrescar]
- Baseline gates: `cargo fmt --check` ✅, `cargo clippy -D warnings` ✅ (29s), `cargo test --workspace` ⏳ verificar completo.
- Deuda registrada en progress.md (RUSTSEC quick-xml/zbus, matriz singular, HashMap no determinista).
- Salida: este Plans.md v2 + Tasks.md v2 + docs/architecture.md

### Fase 1 — Cerebro: Núcleo Lógico Puro [PRIO 1 — TOCAR PRIMERO]
**Objetivo negocio**: que una figura mal escrita nunca rompa el documento ni el solver.

- **1.1 grafito-core** (document 4998L, object 3603L, validation 1215L, constraints, numeric_solver 1484L, persistence 1368L)
  - Statem DocumentLifecycle: Empty → Loading → Validating → Ready → Mutating → Persisting → Ready
  - Type-safety: ObjectId newtype ya, reforzar Resolution/Transform depth, ValidatedDocument wrapper
  - Revisar: MAX limits, HachMap debug (BTreeMap), Transformed depth 64 y matriz singular, Clonación Document en apply_plan
  - Errores: CoreError tipado (ya existe) → auditar que ningún `String` quede sin tipar

- **1.2 grafito-geometry** (expr, cas, matrices, fractals, statistics, interval, ode, special_*, exact)
  - Statem ExprEval: Raw → Parsed → Validated → Evaluated | Failed
  - Matrices: singular_value_tolerance ya, pero Transformed no valida → añadir `ValidatedMatrix`
  - Revisar: expr eval cache, safe_* clamps, MAX_MATRIX_DIM/DENSITY, panics por unwrap

- **1.3 grafito-command** (catálogo, assistant_plan, proposals, context)
  - Statem CommandApply: Proposed → Preflight → Validated → Applied | Rejected (con razón tipada)
  - Type-safety: ProposalRejectionKind ya, extender a todos los comandos

- **1.4 grafito-complex / render / whiteboard / profile / plugins / assistant**
  - Complex: algebraic_mappings, opcode — validar rangos
  - Render: compute shaders wgpu, gpu_compute test headless
  - Whiteboard: overlay nativo sobre grafito-whiteboard (no toca Document/GeoObject) — verificar
  - Plugins: manifest/validate/registry — sandbox

### Fase 2 — Generador de Animaciones v2 [PRIO 0 — CORAZÓN PEDIDO]
**Objetivo negocio**: el docente pide "mostrá la derivada" y en <2s ve un GIF, sin colgar la app, sin que un script rompa el sistema.

- **Statem completo** (ya iniciado en engine.rs, pulir):
  ```rust
  Idle -> Spawning -> AwaitingHello{deadline} -> AwaitingPong{deadline} -> Ready
       -> Running{job_id, deadline} -> Cancelling{job_id} -> ShuttingDown{deadline}
       -> Completed{media_path} | Failed{code,msg} | TimedOut | Cancelled
  // transiciones solo vía fn que consume Self y devuelve Result<NextState, Error>
  ```
- **Type-safety ampliada**:
  - Resolution::try_new(w,h) -> valida 64..8192 (ya en protocol pero falta newtype dedicado)
  - AnimDuration::try_new(secs) -> 0.1..30s
  - AnimJobId ya newtype, pero falta Display/Hash correcto (revisar PartialEq<String>)
  - ExportFormat ya, pero validar en request.validate()
  - WireMessage versionado (ANIM_PROTOCOL_VERSION=1) → añadir major/minor negoc.

- **Engine (engine.rs 754L) — auditoría fina**:
  - spawn: args NUL, workdir validación, leak fix (kill+wait) ✅, falta validar command[0] existe
  - wait_ready: deadline absoluta ✅, poll 250ms ✅, falta pong handshake explícito
  - submit: validar can_submit() == Ready, serializar con line_cap
  - recv_event: filtrar por job_id ✅, line_cap OOM fix ✅ (oversized drain)
  - shutdown: cooperativo (send SHUTDOWN → wait idle_timeout → kill) ✅, falta garantizar Drop no bloquea UI thread
  - run_job: cancel poll 200ms ✅, job_timeout deadline absoluta ✅, validate_media_path ✅ (path_escape)
  - diagnostics: Mutex poison-aware ✅
  - Faltantes: comando no encontrado → error tipado, stderr drain con cap, retry con backoff

- **Python (manim_engine/__main__.py 239L)**:
  - safe_eval: AST whitelist ✅ (MAX_NODES 200, MAX_EXPR_LEN 500, SAFE_FUNCS) — falta bloquear `__import__` y atributos `.__class__`
  - safe_path: JOB_RE, ALLOW_EXPORT ✅, falta symlink escape (canonicalize + startswith)
  - placeholder_media: GIF89a 1x1 + PNG 1x1 ✅, fallback correcto, falta garantizar workdir/canvas coherente
  - manim_is_available: try import ✅, falta cache y timeout en render_with_manim
  - render_with_manim: Axes + FunctionGraph + MathTex ✅, falta manejo de manim config.media_dir race, canvas validado
  - Protocolo: hello/ping/pong/shutdown/render_request/progress/result/error ✅
  - Mejoras pedidas: más templates (integral-area, taylor-series, conformal-map ya declaradas pero no implementadas), progress real % por frames, duración_ms real, export mp4, verbose logging a stderr

- **Nativo fallback (anim_native.rs 189L)**:
  - render_native_animation_frames ✅, render_pitagoras_frames ✅ — falta: unificar a AnimEngine trait, parametrizar x0 y slope, test visual, no alocar en UI thread

### Fase 3 — Piel: Separación Total Cerebro/Piel [SOLO DESPUÉS DE F1+F2]
- **grafito-ui** (animation.rs, assistant.rs 120L+, tokens, theme, toolbar):
  - Tokens tipográficos/spacing/radios/icons ✅ — auditar hardcodes restantes
  - assistant.rs: ASSISTANT_PANEL widths, composer height 116+44+32+20+112 (ya fixeado sin ScrollArea envolvente) ✅ — verificar wrap, clip, TopBottomPanel heights
  - animation.rs: ThinkingOrb state machine → Statem

- **grafito-app** (app.rs 4826L, assistant.rs 4731L, anim_ui 110L, anim_native 189L, panels 3177L, canvas 1649L, render_2d 4750L, ui 1462L):
  - anim_ui.rs: progress bar, ScrollArea horizontal, load_texture con id único por frame ✅ — falta: max_height, bar_width, wrap, export dialog, no I/O
  - assistant.rs: AssistantRuntime (remote_job, proposal_job, model_job, agent_job, anim_job) — lifecycle: Idle -> Thinking -> Verifying -> Animating -> Done. Separar: todo I/O a thread, UI solo renderiza &State
  - app.rs: eframe::App::update dispatch — auditar repaint intervals, MAX_UNDO 50, ViewMode

### Fase 4 — Estabilidad Total (Puertas del AGENTS.md)
- F3.1 fmt --check
- F3.2 clippy --workspace --all-targets -- -D warnings (MSRV 1.92 + stable)
- F3.3 check --workspace --locked
- F3.4 test --workspace --locked
- F3.5 check examples/benches
- F3.6 doc --no-deps (RUSTDOCFLAGS -D warnings)
- F3.7 gpu-compute headless
- F3.8 packaging deb + fixtures

### Fase 5 — Documentación & Handoff
- docs/architecture.md (DAG, statems, budgets)
- README.md / PROGRESS, CHANGELOG promoción
- Riesgos residuales y next steps (RUSTSEC 2026-12-31, etc.)
