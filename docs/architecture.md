# docs/architecture.md — Grafito v1.2.21 (Auditoria 2026-08-20)

## 1. Vision
Grafito es pizarra geometrica con **Cerebro** (Rust puro) y **Piel** (egui/wgpu).
- Cerebro nunca miente: invariants validados, type-safety, Result/Option.
- Piel solo renderiza &Estado: sin I/O, sin spawn.
- Animador es puente IPC versionado a Python/Manim con fallback nativo.

## 2. DAG de crates (workspace resolver 2)

```
grafito-geometry -+
grafito-complex  -+-> grafito-core --> grafito-command --> grafito-app
grafito-render   -+          |                     ^
grafito-anim     ------------+-> grafito-assistant +
grafito-whiteboard ----------+
grafito-ui       ------------> grafito-app  (Piel)
grafito-profile / pedagogy / plugins / assistant-types / agent
```

- **Cerebro puro** (sin egui, sin wgpu): core, geometry, command, complex, whiteboard, profile, pedagogy, plugins, assistant, assistant-types, agent
- **Puente**: anim (IPC stdio JSON v1)
- **Piel**: ui (tokens, theme, toolbar, assistant, animation), app (app.rs, panels.rs, canvas.rs, render_2d, anim_native/ui)
- **Infra**: packaging/deb, .github/workflows/ci.yml (8 jobs)

## 3. Principios (Slash Commands)

- **/j-space**: todo plan en Markdown antes de codigo (Plans.md, Tasks.md, progress.md, docs/architecture.md)
- **/statem**: flujos como enum Estado + transiciones tipadas que no compilan si son ilegales
- **/rust-design**: newtype (AnimJobId, Resolution, AnimDuration), Result/Option, clippy -D warnings =0, sin unwrap en prod
- **/rust-ui**: UI = fn render(&Estado) -> Frame, I/O en background thread
- **/vibecoder-guide**: error de compilador -> explicacion en lenguaje negocio + menu 2-3 opciones

## 4. Statems

### 4.1 AnimJob (grafito-anim/src/engine.rs)

```
Idle -> Spawning -> AwaitingHello{deadline} -> AwaitingPong{deadline} -> Ready
     -> Running{job_id, deadline} -> Cancelling{job_id} -> ShuttingDown{deadline}
     -> Completed{media_path} | Failed{code,msg} | TimedOut | Cancelled
```
- Transiciones via &mut self con can_submit()==Ready, deadline absoluta, poll 200ms para cancel.
- Validaciones: spawn NUL/workdir/bin path, line_cap 64KB OOM-safe, diagnostics cap 64 lineas, validate_media_path path_escape, Drop no-bloqueante (kill+try_wait).

### 4.2 DocumentLifecycle (grafito-core, propuesto ValidatedDocument wrapper)

```
Empty -> Loading -> Validating -> Ready -> Mutating -> Persisting -> Ready
```
- MAX limits: MAX_OBJECT_COUNT 5000, MAX_EXPR_LENGTH 2000, MAX_TRANSFORM_DEPTH 64, MAX_DOCUMENT_SIZE 10M.
- Hash determinista: sorted vars antes de hashear (object.rs:2605 fix), BTreeMap en serializacion (deuda migratoria).
- Transformed try_new valida prepare_function_ast("z") y rechaza "0" singular.

### 4.3 AssistantRuntime (grafito-app/src/assistant.rs)

```
Idle -> Thinking -> Verifying -> Animating{job_id} -> Done | Failed | Cancelled
```
- Cada job (remote, proposal, model, image, agent, anim) en thread con CancellationToken y RequestBudget (max_input 4096, max_steps 24, timeout 15s).
- I/O nunca en UI thread; UI solo renderiza &AssistantState.

### 4.4 Interval/ExprEval (grafito-geometry)

```
Raw -> Parsed -> Validated -> Evaluated | Failed
```
- Cache LRU 128, midpoint lo+(hi-lo)/2, safe_sample valida finite y MAX_SAMPLES 100k.

## 5. Type-Safety

- **AnimJobId**: newtype String try_new regex ^[A-Za-z0-9_-]{1,64}
- **Resolution**: try_new 64..=4096, as_tuple()
- **AnimDuration**: try_new 0.1..=30s, as_millis()
- **AnimParams**: validate template/concept/spec + params finite + duration/resolution
- **ExportFormat**: Gif/PngSequence/Mp4
- **WireMessage**: Hello{protocol_version, capabilities} | Progress | Result | Error{code,msg} | Pong ; versionado ANIM_PROTOCOL_VERSION=1
- **Matrix**: try_* con MAX_MATRIX_DIMENSION 1000, MAX_ELEMENTS 1M, debug_assert en zeros/identity, SVD singular_value_tolerance

## 6. Generador de Animaciones v2

### 6.1 Protocolo (protocol.rs + python __main__.py)
- JSON v1 sobre stdio, line_cap 64KB, backpressure sync_channel(128)
- Python sandbox: AST whitelist + Denylist Attribute/Subscript/Lambda, MAX_NODES 200, MAX_EXPR_LEN 500, SAFE_FUNCS, dunder "__" bloqueado, JOB_RE, ALLOW_EXPORT/TEMPLATE, safe_path relative_to symlink-safe, parse_canvas 64..4096, placeholder hex estable, manim_is_available cache, render_with_manim por template (derivative-slope, integral-area, taylor-series, conformal-map) con media_path relative_to check, progress 30->60->100, fallback placeholder con stderr log.

### 6.2 Engine (engine.rs)
- Config: command argv, working_dir, idle_timeout 8s, job_timeout 90s, line_cap 64KB
- Metodos: spawn, wait_ready (hello+pong), submit (solo Ready), recv_event (filtra job_id), shutdown (cooperativo 8s), cancel() (Running->Cancelling->Cancelled), run_job (efimero con cancel poll 200ms, deadline absoluta, validate_media_path), diagnostics(), state()
- Tests: 5 tests en engine::tests (stub python, health_check, media_path reject, propagate errors, timeout, statem reject)

### 6.3 Nativo fallback (anim_native.rs)
- render_native_animation_frames (parabola tangente), render_pitagoras_frames, render_integral_frames, render_taylor_frames, render_conformal_frames, dispatcher render_anim_by_template(template,w,h)
- Tests: 5 tests (bounded, distinct, integral, taylor, conformal, dispatcher fallback)
- UI (anim_ui.rs): AnimPreviewState {template, concept, progress, status, media_path, frames, source_turn}, draw_anim_panel con bar_width 420, max_height 84, textura id unico por frame, sin I/O, build_anim_params con Resolution/Duration

## 7. Piel (grafito-ui / grafito-app)

- **Tokens** (grafito-ui/src/tokens.rs): TYPE_XS..XXL (11..28, ratio 1.25), SPACE_XS..XXL (4..32, base 4), RADIUS_SM..LG, ICON_SM..XL — unica fuente de verdad.
- **Assistant panel** (grafito-ui/src/assistant.rs): SidePanel 340..460 o TopBottomPanel bottom compacto (<780px), composer 116+44+32+20+112 con clamp 88..260, sin ScrollArea envolvente (fix overflow), wrapping, clip.
- **App shell** (grafito-app/src/app.rs 4826L): eframe::App::update dispatch (820L god function, deuda P1), GrafitoApp ~75 campos (god object), MAX_UNDO 50 Vec<Document> (O(n) shift, deuda P2), ViewMode/Perspective/CanvasMode redundancia (deuda P1), repaint intervals 150ms settle, 33ms multidimensional 30Hz, 16ms whiteboard 60Hz.

## 8. Presupuestos y Limites

| Dominio | Constante | Valor | Ubicacion |
|---------|-----------|-------|-----------|
| Documento | MAX_DOCUMENT_SIZE_BYTES | 10M | validation.rs |
| Documento | MAX_OBJECT_COUNT | 5000 | validation.rs |
| Expr | MAX_EXPR_LENGTH | 2000 | validation.rs |
| Matriz | MAX_MATRIX_DIMENSION | 1000 | matrices.rs |
| Matriz | MAX_MATRIX_ELEMENTS | 1M | matrices.rs |
| Cache | MAX_COMPILED_EXPR_CACHE | 128 | expr.rs |
| Interval | MAX_SAMPLES | 100k | interval.rs |
| Anim | MAX_CANVAS | 4096 | python |
| Anim | MIN_CANVAS | 64 | python |
| Anim | MAX_EXPR_LEN | 500 | python |
| Anim | MAX_NODES | 200 | python |
| Anim | line_cap | 64KB | engine.rs |
| Anim | diagnostics cap | 64 lineas | engine.rs |
| Assistant | RequestBudget max_input | 4096 | assistant-types |
| Assistant | AttachmentLimits max_bytes | 5M | assistant-types |
| UI | ASSISTANT_PANEL width | 340..460 | assistant.rs |
| App | MAX_UNDO | 50 | app.rs |

## 9. Verificacion CI (8 jobs)

1. cargo check --workspace --locked
2. cargo check -p grafito-app --target x86_64-pc-windows-gnu --all-features --locked
3. cargo test --workspace --locked
4. cargo test -p grafito-render --test gpu_compute --locked (WGPU_BACKEND=gl headless)
5. cargo check --workspace --examples/benches --locked
6. cargo doc --workspace --no-deps --locked (RUSTDOCFLAGS -D warnings)
7. cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
8. cargo fmt --all -- --check
+ packaging: build-deb.sh + packaging-fixtures.sh

Baseline 2026-08-20: 7/8 PASS, gpu_compute SKIP headless, release-build SKIP 45m.

## 10. Riesgos residuales

- RUSTSEC-2026-0194/0195 quick-xml/zbus expiran 2026-09-30
- HashMap debug no determinista en Document (migrar a BTreeMap, P0)
- Transformed matriz singular solo check trivial "0" (P1, falta Jacobian det)
- AssistantRuntime sin Statem formal (P1, distribuido)
- App God Object y God Function update (P1)

## 11. Proximos pasos (por capas)

1. F1 Cerebro: ValidatedDocument wrapper + BTreeMap + CoreError 100% + ValidatedMatrix
2. F2 Anim: cancel() API publica + Cancelling estado + transiciones consume Self + tests <2s placeholder
3. F2 Anim: templates completos con progress por frames reales + export mp4 + verbose stderr
4. F3 Piel: mover I/O assistant a thread, UI fn render(&State), ThinkingOrb Statem, fix ViewMode duplicado
5. F4 Estabilidad: check examples/benches, doc -D warnings, WGPU_BACKEND=gl, packaging fixtures
