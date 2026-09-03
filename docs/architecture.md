# docs/architecture.md — Grafito v1.2.35 (Plan supera GeoGebra 2026-08-26)

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
- **Infra**: packaging/deb, .github/workflows/ci.yml (14 jobs)

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

### 4.2 DocumentLifecycle (grafito-core, ValidatedDocument wrapper — implementado)

```
Empty -> Loading -> Validating -> Ready -> Mutating -> Persisting -> Ready
```
- `ValidatedDocument` (`validation.rs:40`): wrapper fail-closed `try_new(doc)` que ejecuta `validate_document` antes de persistir o exponer snapshot al render. Migrar `HashMap` → `BTreeMap` en `Document` sigue como deuda migratoria; por ahora `semantic_document_baseline` garantiza orden.
- MAX limits: MAX_OBJECT_COUNT 5000, MAX_EXPR_LENGTH 2000, MAX_TRANSFORM_DEPTH 64, MAX_DOCUMENT_SIZE 10M.
- Hash determinista: sorted vars antes de hashear (object.rs:2605 fix), BTreeMap en serializacion (deuda migratoria).
- Transformed try_new valida prepare_function_ast("z") y rechaza "0" singular.

### 4.3 AssistantRuntime (grafito-app/src/assistant.rs)

```
Idle -> Thinking -> Verifying -> Animating{job_id} -> Done | Failed | Cancelled
```
- Cada job (remote, proposal, model, image, agent, anim) en thread con CancellationToken y RequestBudget (max_input 8192, max_steps 8, timeout 60s).
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
- **App shell** (grafito-app/src/app.rs 4826L): eframe::App::update dispatch (820L god function, deuda P1), GrafitoApp ~75 campos (god object), `MAX_UNDO` 50 + `MAX_UNDO_BYTES` 50 MiB con `VecDeque<Document/ChangeSet>` (`pop_front` O(1), `Vec` previo era O(n) shift — corregido), `controllers.rs` stubs `DocumentController/ViewController/AssistantController` con `VecDeque` (P1), ViewMode/Perspective/CanvasMode redundancia (deuda P1), repaint intervals 150ms settle, 33ms multidimensional 30Hz, 16ms whiteboard 60Hz.

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
| Assistant | RequestBudget max_input | 8192 | assistant-types |
| Assistant | RequestBudget max_steps | 8 | assistant-types |
| Assistant | RequestBudget timeout | 60s | assistant-types |
| Assistant | AttachmentLimits max_bytes | 512 KiB | assistant-types |
| Assistant | AttachmentLimits max_total_bytes | 1 MiB | assistant-types |
| UI | BREAKPOINT_COMPACT | 1360 | tokens.rs (is_compact_viewport) |
| UI | PANEL_LEFT_DEFAULT | 260 (min 180, max 45% viewport via PANEL_LEFT_MAX_FRACTION) | tokens.rs + panels.rs/algebra.rs |
| UI | PANEL_LEFT_MIN | 180 | tokens.rs |
| UI | PANEL_LEFT_MAX_FRACTION | 0.45 (clamp + panel_left_max_width) | tokens.rs |
| UI | DRAWER_RIGHT_DEFAULT | 344 (min 292, max 440) | tokens.rs |
| UI | DRAWER_RIGHT_MIN | 292 | tokens.rs |
| UI | DRAWER_RIGHT_MAX | 440 | tokens.rs |
| UI | RAIL_WIDTH | 60 | tokens.rs + ui.rs |
| UI | TOP_BAR_HEIGHT | 48 | tokens.rs |
| UI | SPLASH_LOGO_SIZE | 128 | tokens.rs |
| UI | ASSISTANT_PANEL width | 340..460 (default 400) | assistant.rs (ASSISTANT_PANEL_MIN/MAX) |
| UI | Tessellation egui (rayon) | 1-2 ms/frame, 10K verts (egui/rayon tessellation paralela) | Cargo.toml `egui = { features = ["rayon"] }` + app.rs:6 presupuestos |
| GPU | domain_coloring_compute | 250k cells/dispatch (500×500, MAX_CELLS 250k) | grafito-render/domain_coloring_compute.rs:13 + lib.rs |
| App | MAX_UNDO | 50 (VecDeque pop_front O(1)) | app.rs:33 + controllers.rs:19 |
| App | MAX_UNDO_BYTES | 50 MiB (VecDeque, pop_front O(1), Document::estimated_bytes) | app.rs:40 + controllers.rs:21 |
| App | undo_stack | VecDeque<Document> + VecDeque\<ChangeSet\> (pop_front O(1), fix Vec::remove(0) O(n)) | app.rs + controllers.rs |
| Core | ValidatedDocument | fail-closed wrapper try_new | validation.rs:40 |
| Assets | mora.png / mora.svg | <32 KiB PNG embebido via include_bytes! (fallback dibujado si falla) | assets/mora.png, assets/mora.svg, app.rs:4707 |

## 9. Verificacion CI (14 jobs) — .github/workflows/ci.yml

MSRV 1.92 (`rust-version.workspace = "1.92"`) verificada en matriz `toolchain: ['1.92', stable]` para `check`, `test`, `lint`; `cargo metadata --locked` con 1.92 valida lockfile completo; docs advierten 1.92 en `Cargo.toml`, `ci.yml`, `CONTRIBUTING.md`, `AGENTS.md`, `README*.md`, `packaging/README.md` ( packaging-fixtures.sh lo exige).

| # | Job | Comando / descripcion | Runner / notas |
|---|-----|------------------------|----------------|
| 1 | `check` | `cargo check --workspace --locked` + `cargo check -p grafito-app --target x86_64-pc-windows-gnu --all-features --locked` (solo 1.92) | matrix 1.92 + stable, apt cache libgmp/mpfr/mpc/dbus, mingw-w64 para 1.92 |
| 2 | `test` | `cargo test --workspace --locked` | matrix 1.92 + stable |
| 3 | `gpu-compute` | `cargo test -p grafito-render --test gpu_compute --locked` **required** | `WGPU_BACKEND=vulkan`, `GRAFITO_REQUIRE_GPU_TESTS=1`, `mesa-vulkan-drivers` + `libvulkan1`, no longer `WGPU_BACKEND=gl headless` ni SKIP; falla si GPU no disponible |
| 4 | `examples` | `cargo check --workspace --examples --locked` | stable |
| 5 | `benches` | `cargo check --workspace --benches --locked` | stable (separado de examples desde 14-job split) |
| 6 | `docs` | `cargo doc --workspace --no-deps --locked` con `RUSTDOCFLAGS=-D warnings` | stable |
| 7 | `lint` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | matrix 1.92 + stable, components clippy |
| 8 | `release-build` | `cargo build --workspace --release --locked` | solo push main, ubuntu-22.04 |
| 9 | `fmt` | `cargo fmt --all -- --check` | stable, rustfmt |
| 10 | `all-targets` | `cargo check --workspace --all-targets --all-features --locked` | stable |
| 11 | `cross-platform-smoke` | `cargo test -p grafito-app --test app_smoke --locked` | matrix os: ubuntu/windows/macos, fail-fast false |
| 12 | `supply-chain` | `cargo audit 0.22.2` + `cargo deny 0.20.2` + `verify_advisory_exceptions.py` | cache cargo-tools |
| 13 | `workflow-lint` | `actionlint 1.7.7` + `shellcheck` + `bash -n` + `bash packaging/tests/packaging-fixtures.sh` + Debian version mapping (`1.2.20~beta < 1.2.20`) | valida packaging fixtures como gate |
| 14 | `package-debian` | `desktop-file-validate` + `packaging/build-deb.sh` + `dpkg-deb --info/--ctrl-tarfile` ownership `root:root`, permisos, `lintian --fail-on error`, `dpkg --install` + `/usr/bin/grafito --help` + purge | ubuntu-22.04, Needs `dpkg-dev lintian desktop-file-utils` |

Notas:
- `gpu-compute` ahora **requerido** con `WGPU_BACKEND=vulkan` (antes `gl` headless SKIP); `GRAFITO_REQUIRE_GPU_TESTS=1` hace fail-closed si el adapter no esta disponible.
- Packaging fixtures (`packaging/tests/packaging-fixtures.sh`) es gate en `workflow-lint`: verifica iconos `16..512` + scalable `hicolor/scalable/apps/grafito.svg`, `grafito-icon.svg`, abort si falta asset, y `desktop Icon=grafito`, mas plugins `usr/share/grafito/plugins` (`j-space`), `postrm` parse, MSRV 1.92 docs, MSVC static CRT, e icon asset existencia; `assets/mora.png/.svg` existen y se embeben via `include_bytes!` (verificado en `app.rs:4870` test `<32 KiB`).
- Baseline 2026-08-20: 7/8 PASS (gpu_compute SKIP headless, release-build SKIP 45m). Desde 14-job split: gpu-compute ya no SKIP, package-debian y workflow-lint son blocking.

## 10. Novedades v1.2.35 — supera GeoGebra (2026-08-26)

**Pedagogía multi-nivel (primaria→ingeniería)**
- `grafito-pedagogy::Curriculum` 42 LOs: UTN AM1 8, AM2 7, Álgebra 6, Prob 6, Secundaria 10, Primaria 5 (level_min, requires DAG, tags, topological_order Kahn)
- `UdlProfile {Concrete,Graphic,Symbolic,Formal}` + `level_value()` (Primary 2, Secondary 8, AM1 12...)
- `SocraticFsm` Review→HeuristicQ→AwaitStudent→Rectify→Summarize (Telling<5%), `ScaffoldEngine` ya usa `history`
- `ExerciseGenerator::generate_with_seed` wyhash paramétrico (a,b,c) + `ValidatorKind` + `FeedbackEngine` 8 misconceptions (Sign/Distributive/ChainRule/Fraction/Domain/Notation)

**Perfil adaptativo**
- `BKT` bayesiano + `Scheduler` Leitner `next_interval=86400*2^(level-1)*(2-mastery)` → `BranchState {next_review_epoch, box_level, bkt_p_known}` + `branches_due()` + `recommend_next_with_scheduler()`
- `WorkingMemory` sesión (steps_tried, misconception_counts) sincronizado `assistant.rs:520` con `StudentProfile`

**Asistente OpenCode Go socrático**
- `PedagogyDispatcher` 6 tools puras (`scaffold`, `generate_exercise`, `assess_answer`, `get_curriculum`, `suggest_next`, `generate_animation`) + 3 base = 9 `all_safe_tool_schemas()` OpenAI-compat (`agent.rs` 680L, 14 tests)
- `TeachingTopic` 14 variantes, `teaching_ui::whiteboard_elements_for_hint` 14 mappings (fracción, vector, matriz, prob, serie, trig, cónica...), `anim_native` templates pedagógicos, `AssistantExerciseCard` inline `grafito-exercise`

**UI Scandinavian sin laberinto**
- `toolbar.rs` `PRIMARY 5` `SECONDARY 8` `UNIVERSITY 17` + `toolbar_groups_for_level_value(u32)` + `udl.rs` helper sin depender de pedagogy; `filter_groups_by_level`
- `AssistantPanelState` `max_composer 88..260` quiet, tokens 64%/44%/10%/5% documentados, `whiteboard:WhiteboardDoc` persistente en `Document` (serde, cota 500 elementos), `spreadsheet` `=A1+B1` stripping, `AppConfig::onboarding_completed`

**Gates:** `cargo fmt 0` `clippy -D warnings 0` `test --workspace ~1500 verdes`

## 11. Riesgos residuales

- BTreeMap determinismo total pendiente (mitigado `ValidatedDocument` + ordenación explícita; próximo: migrar `objects: HashMap→BTreeMap`)
- `Transformed` Jacobian det pendiente
- `fill_compute` aún `None` (ahorra 128 MiB, habilitar lazy si `ImplicitCurve != Eq`)
- App God Object `app.rs 4752L` parcialmente extraído (`controllers.rs` stubs)

## 12. Próximos pasos

1. Migrar `Document {objects,variables}` a `BTreeMap` + `ValidatedMatrix`
2. `WhiteboardDoc` export SVG + `GeoObject::Whiteboard` independiente
3. `fill_compute` lazy + `DomainColoring` en export
4. Classroom P2P opt-in (feature flag)
