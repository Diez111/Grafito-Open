# docs/architecture.md — Grafito v1.2.35 (Plan supera GeoGebra 2026-08-26; sync BUILD 2026-09-04)

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
- **Infra**: packaging/deb, .github/workflows/ci.yml (17 jobs)

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
- Cada job (remote, proposal, model, image, agent, anim) en thread con CancellationToken y RequestBudget (max_input_chars 8192, max_steps 8, timeout_ms 60000 = 60s — `crates/grafito-assistant-types/src/lib.rs:198-209`; AttachmentLimits max_bytes 512 KiB + max_total_bytes 1 MiB — `lib.rs:245-255`).
- Protocolos por modelo (OpenCodeGo) — `crates/grafito-assistant/src/lib.rs:938-951` (`remote_protocol`): Chat Completions default (`chat_completion_endpoint`, `:899-901`), Responses API `POST {base}/responses` para Muse Spark 1.2/1.3 (`responses_endpoint`, `:905-907`; router `uses_responses_api` = `model.contains("muse-spark")`, `:54-56`; por Chat devuelven 500 instantáneo con cualquier payload, verificado 2026-09-04 — comentario en `:50-53`), Anthropic Messages `POST {base}/messages` para mimo-2.5-vl (`messages_endpoint`, `:915-917`; match en `:946-947`), Fusion (draft+audit) para `fusion` (`:948`; `FUSION_AUDIT_MODEL = "deepseek-v4-pro"`, `:65`). Modo agente con tools aún no soportado en Spark → fallback sólo-sesión a deepseek (`crates/grafito-app/src/assistant.rs:2470-2485`, `:2595-2613`; aviso Responses en `:2916-2917`).
- Modelos: default `deepseek-v4-flash` (`crates/grafito-app/src/utils.rs:59-61`); `qwen3.8-max` / `kimi-k3` sólo como sugerencia en el hint de error de Spark (`assistant.rs:2936`, sin tests dedicados); `mimo-2.5-vl` visión (`assistant.rs:36`, `lib.rs:48`); `fusion` (`lib.rs:49`). Tests wire: `spark_models_route_to_responses_api`, `responses_payload_shape_matches_verified_wire_format`, `responses_wire_uses_bearer_and_response_shape` (`lib.rs:2684-2920`); fallback sesión `assistant.rs:4585-4605`.
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
- **Assistant panel** (`crates/grafito-ui/src/assistant.rs:99-105`): SidePanel 300..520 (default 400) o TopBottomPanel bottom-sheet cuando el viewport < 740px (`ASSISTANT_SIDE_PANEL_MIN_VIEWPORT_WIDTH = 440 + 300`, `:103-105`; `assistant_uses_bottom_sheet`, `:1778-1780`), composer con clamp 88..260, sin ScrollArea envolvente (fix overflow), wrapping, clip.
- **App shell** (grafito-app/src/app.rs ~5564L): eframe::App::update dispatch (god function, deuda P1), GrafitoApp ~75 campos (god object), `MAX_UNDO` 50 + `MAX_UNDO_BYTES` 50 MiB con `VecDeque<Document/ChangeSet>` (`pop_front` O(1), `Vec` previo era O(n) shift — corregido), `controllers.rs` stubs `DocumentController/ViewController/AssistantController` con `VecDeque` (P1), ViewMode/Perspective/CanvasMode redundancia (deuda P1), repaint intervals 150ms settle, 33ms multidimensional 30Hz, 16ms whiteboard 60Hz.
- **Atajos verificados** (handlers en `grafito-app/src/app.rs:4088-4290`; menús en `ui.rs:141-227`; etiquetas toolbar en `grafito-ui/src/toolbar.rs:36-156`): Ctrl+N/O/S + Ctrl+Shift+S archivo (`lifecycle.rs:20-31`), Ctrl+Z/Y deshacer/rehacer (+Shift en Ctrl+Y = herramienta YIntercept, `app.rs:237-248`), Supr eliminar, Esc cancelar, F1-F6 herramientas 2D, F8 Esfera 3D + F9 Cubo 3D (`app.rs:4135-4144`), R/E/I/X/N/S/Y/V/M/G herramientas sin modificadores, Ctrl+A Analizar, Shift+L/K/J toggles log X/Y/ambos, G snap, Ctrl+K paleta, Ctrl+T tema (`app.rs:4259-4267`), Ctrl+P Lápiz + Ctrl+E Borrador (`app.rs:4268-4278`), Ctrl+Shift+1..9,0 perspectivas (10, `app.rs:4236-4242`). Cero fantasmas desde BUILD 2026-09-04 (antes: Ctrl+P, Ctrl+E, F8, F9 documentados sin handler).
- **Responsive shell**: rail 60px (`RAIL_WIDTH`, `tokens.rs:164`; `ui.rs:549-552`) visible sólo en Medium/Wide (≥1360, `lib.rs:417-424,441-442`) — colapsado en Compact, luego también <780px; drawer derecho 292..440 con clamp (`clamp_drawer_right_width`, `tokens.rs:207-210`; dock 3D `ui.rs:727-731`; Inspector `panels.rs:2125-2132`); panel izquierdo min 180 + max 45% viewport (`PANEL_LEFT_MIN`, `PANEL_LEFT_MAX_FRACTION`, `tokens.rs:151-154`; `panels.rs:1201-1206`).
- **Onboarding** (`app.rs:1763`, `:4922-5033`; `utils.rs:46-48`): gating `show_onboarding = !config.onboarding_completed`; Window 420px, 3 bullets (5/8/17 grupos), botones [Probar ejemplo][Empezar vacío][No mostrar]; Probar ejemplo y No mostrar persisten `onboarding_completed=true`.
- **Paleta de comandos** (`grafito-ui/src/command_palette.rs`): fuzzy subsecuencia sin tildes (`fuzzy_match`, `:224-251`), bilingüe es/en (`filtered_commands`, `:275-296`), footer en español con conteo "N de M · ↑↓ navegar · Enter abrir · Esc cerrar" (`:394-403`), 14 acciones UI en español con clave inglesa estable (`UI_ACTIONS`, `:58-199`; test `:528-584`).

## 8. Presupuestos y Limites

| Dominio | Constante | Valor | Ubicacion verificada |
|---------|-----------|-------|----------------------|
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
| Assistant | RequestBudget max_input_chars | 8192 | assistant-types/src/lib.rs:201 (+validate cap :214) |
| Assistant | RequestBudget max_output_chars | 2048 | assistant-types/src/lib.rs:202 (+validate cap :217) |
| Assistant | RequestBudget max_steps | 8 | assistant-types/src/lib.rs:203 (+validate cap :220) |
| Assistant | RequestBudget timeout_ms | 60000 (60s) | assistant-types/src/lib.rs:204-206 (+rango 100..=120000 :223) |
| Assistant | AttachmentLimits max_bytes | 512 KiB | assistant-types/src/lib.rs:248 |
| Assistant | AttachmentLimits max_total_bytes | 1 MiB | assistant-types/src/lib.rs:251 |
| Assistant | AttachmentLimits max_pixels / max_total_pixels | 1 MiP / 2 MiP | assistant-types/src/lib.rs:249,252 |
| Assistant | AttachmentLimits max_attachments | 2 | assistant-types/src/lib.rs:250 |
| Comandos | COMMANDS registrados | 232 (`command!(`) | command/src/command_registry.rs (`grep -c 'command!('` = 232 únicos) |
| Comandos | palette-visible | 193 (39 ocultos) + 14 acciones UI = 207 en paleta | command_registry.rs + grafito-ui/src/command_palette.rs:58-199 |
| Comandos | categorías visibles | 26 (30 etiquetas raw con duplicados con/sin tilde) | command_registry.rs (verificado por script, BUILD 2026-09-04) |
| Toolbar | ToolGroupId / UNIVERSITY | 17 (PRIMARY 5, SECONDARY 8) | grafito-ui/src/toolbar.rs:263-284 (+tests :1317-1319) |
| Toolbar | Tool variantes | 76 | grafito-ui/src/lib.rs `pub enum Tool` (+Parallel/Arc/Sector F9) |
| App | Perspectivas | 10 (Ctrl+Shift+1..9,0) | grafito-app/src/lib.rs:90-111 + app.rs:4236-4242 |
| Workspace | crates | 18 | `crates/` (agent, anim, app, assistant, assistant-types, classroom, command, complex, core, geometry, ggb, pedagogy, plugins, profile, release-tests, render, ui, whiteboard) |
| UI | BREAKPOINT_COMPACT | 1360 | tokens.rs:142 (is_compact_viewport :188-191) |
| UI | PANEL_LEFT_DEFAULT | 260 (min 180, max 45% viewport via PANEL_LEFT_MAX_FRACTION) | tokens.rs + panels.rs/algebra.rs |
| UI | PANEL_LEFT_MIN | 180 | tokens.rs |
| UI | PANEL_LEFT_MAX_FRACTION | 0.45 (clamp + panel_left_max_width) | tokens.rs |
| UI | DRAWER_RIGHT_DEFAULT | 344 (min 292, max 440) | tokens.rs |
| UI | DRAWER_RIGHT_MIN | 292 | tokens.rs |
| UI | DRAWER_RIGHT_MAX | 440 | tokens.rs |
| UI | RAIL_WIDTH | 60 | tokens.rs + ui.rs |
| UI | TOP_BAR_HEIGHT | 48 | tokens.rs |
| UI | SPLASH_LOGO_SIZE | 128 | tokens.rs |
| UI | ASSISTANT_PANEL width | 300..520 (default 400); bottom-sheet si viewport < 740 | assistant.rs:99-105 + `assistant_uses_bottom_sheet` :1778-1780 |
| UI | Tessellation egui (rayon) | 1-2 ms/frame, 10K verts (egui/rayon tessellation paralela) | Cargo.toml `egui = { features = ["rayon"] }` + app.rs:6 presupuestos |
| GPU | domain_coloring_compute | 250k cells/dispatch (500×500, MAX_CELLS 250k) | grafito-render/domain_coloring_compute.rs:13 + lib.rs |
| App | MAX_UNDO | 50 (VecDeque pop_front O(1)) | app.rs:33 + controllers.rs:19 |
| App | MAX_UNDO_BYTES | 50 MiB (VecDeque, pop_front O(1), Document::estimated_bytes) | app.rs:40 + controllers.rs:21 |
| App | undo_stack | VecDeque<Document> + VecDeque\<ChangeSet\> (pop_front O(1), fix Vec::remove(0) O(n)) | app.rs + controllers.rs |
| Core | ValidatedDocument | fail-closed wrapper try_new | validation.rs:40 |
| Assets | mora.png / mora.svg | <32 KiB PNG embebido via include_bytes! (fallback dibujado si falla) | assets/mora.png, assets/mora.svg, app.rs:4707 |

## 9. Verificacion CI (17 jobs) — .github/workflows/ci.yml

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
| 14 | `coverage` | `cargo llvm-cov --workspace --all-targets --all-features --locked` con gate `--fail-under-lines 75` + fallback `cargo test` | stable + llvm-tools-preview, artefacto lcov 14 días |
| 15 | `bench-regression` | `cargo bench --workspace --benches --locked` (criterion, baseline `main`, regresión >10%) | stable, artefacto target/criterion 7 días |
| 16 | `mutation` | `cargo mutants 24.11.1 --workspace --timeout 60 --in-place` | sólo `schedule` semanal / `workflow_dispatch`, 60 min |
| 17 | `package-debian` | `desktop-file-validate` + `packaging/build-deb.sh` + `dpkg-deb --info/--ctrl-tarfile` ownership `root:root`, permisos, `lintian --fail-on error`, `dpkg --install` + `/usr/bin/grafito --help` + purge | ubuntu-22.04, Needs `dpkg-dev lintian desktop-file-utils` |

Notas:
- `gpu-compute` ahora **requerido** con `WGPU_BACKEND=vulkan` (antes `gl` headless SKIP); `GRAFITO_REQUIRE_GPU_TESTS=1` hace fail-closed si el adapter no esta disponible.
- Packaging fixtures (`packaging/tests/packaging-fixtures.sh`) es gate en `workflow-lint`: verifica iconos `16..512` + scalable `hicolor/scalable/apps/grafito.svg`, `grafito-icon.svg`, abort si falta asset, y `desktop Icon=grafito`, mas plugins `usr/share/grafito/plugins` (`j-space`), `postrm` parse, MSRV 1.92 docs, MSVC static CRT, e icon asset existencia; `assets/mora.png/.svg` existen y se embeben via `include_bytes!` (verificado en `app.rs:4870` test `<32 KiB`).
- Baseline 2026-08-20: 7/8 PASS (gpu_compute SKIP headless, release-build SKIP 45m). Desde 14-job split: gpu-compute ya no SKIP, package-debian y workflow-lint son blocking. BUILD 2026-09-04: 17 jobs (se suman `coverage` 75%, `bench-regression` >10%, `mutation` semanal).

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

## 13. Tabla doc→código (BUILD 2026-09-04 — todo número con origen verificado)

| Afirmación en docs | Origen verificado en código |
|---|---|
| RequestBudget 8192 / 2048 / 8 / 60s | `crates/grafito-assistant-types/src/lib.rs:198-209` |
| AttachmentLimits 512 KiB / 1 MiB / 1-2 MiP / 2 adjuntos | `crates/grafito-assistant-types/src/lib.rs:245-255` |
| 232 comandos (`command!(`), 193 visibles en paleta | `crates/grafito-command/src/command_registry.rs` (232 únicos; F9 suma `dynamic.trace`/Rastro) |
| 14 acciones UI + fuzzy + footer es | `crates/grafito-ui/src/command_palette.rs:58-199`, `:224-251`, `:394-403` |
| 17 grupos toolbar (PRIMARY 5, SECONDARY 8, UNIVERSITY 17) | `crates/grafito-ui/src/toolbar.rs:263-284`, tests `:1317-1319` |
| 76 herramientas (`Tool`) | `crates/grafito-ui/src/lib.rs` `pub enum Tool` (+Parallel/Arc/Sector F9) |
| 10 perspectivas (Ctrl+Shift+1..9,0) | `crates/grafito-app/src/lib.rs:90-111`, `app.rs:4236-4242` |
| 18 crates workspace | `crates/` (ls: +classroom R5, +ggb F9) |
| 17 jobs CI | `.github/workflows/ci.yml:24-496` |
| Spark vía Responses API (`POST {base}/responses`) | `crates/grafito-assistant/src/lib.rs:50-56`, `:905-907`, `:938-951` |
| Modelo default `deepseek-v4-flash` | `crates/grafito-app/src/utils.rs:59-61` |
| Fallback sesión spark→deepseek | `crates/grafito-app/src/assistant.rs:2470-2485`, `:2595-2613` |
| Ctrl+T tema | `crates/grafito-app/src/app.rs:4259-4267` + menú `ui.rs:209` |
| Ctrl+P/E lápiz/borrador, F8/F9 esfera/cubo | `crates/grafito-app/src/app.rs:4135-4144`, `:4268-4278` |
| Onboarding 420px, 3 bullets, 3 botones | `crates/grafito-app/src/app.rs:4922-5033`, gating `:1763`, `utils.rs:46-48` |
| Rail 60px, drawer 292..440, panel izq 180+45% | `crates/grafito-ui/src/tokens.rs:151-164,207-210`; `app/src/ui.rs:549-552,727-731`; `app/src/panels.rs:1201-1206,2125-2132` |

## 14. Paridad GeoGebra 2026 — frente F10-C (BUILD 2026-09-05, rama f10-plan-total)

Cerebro puro en `crates/grafito-core/src/symbolic/` (`csv.rs`, `solids.rs`,
`exchange.rs`, `mod.rs` con `groebner_gate`); piel fina en
`crates/grafito-app/src/render_3d.rs` (`OrthoProjection`,
`project_point_ortho`, `solid_measure_text`); helps honestos en
`crates/grafito-command/src/command_registry.rs` (Groebner 2×2, Net L).
Sin tocar geometría exacta, A11Y ni perf; sin `unwrap` (gates §9).

| Categoría | Grafito hoy (archivo) | GeoGebra | Esfuerzo |
|---|---|---|---|
| Capas | `symbolic/exchange.rs` (`LayerTable` 0..=255 + visibilidad) | capas con orden | S cerrado (API; wiring panel P2) |
| Bar/Pie | `symbolic/exchange.rs` (`bar_chart_stub`/`pie_chart_stub` validan y derivan a Histogram) | BarChart/PieChart | S cerrado honesto |
| Tabla viva lectura | `symbolic/exchange.rs` (`datatable_rows`/`cell`/`to_csv` sobre `DataTableObj`) | spreadsheet viva | S cerrado (edición P2) |
| Volumen/área 3D | `symbolic/solids.rs` (esfera/cubo/cilindro/cono/toro/tetra/pirámide/prisma exactos; cuádrica → `None` + `solid_measure_status`) | Volume/Area 3D | S cerrado |
| Vistas ortográficas | `symbolic/solids.rs` (`OrthoView` alzado/planta/perfil) + `render_3d.rs` (`OrthoProjection`, píxeles egui) | vistas 3D | S cerrado (cableado cámara P2) |
| Groebner | `symbolic/mod.rs` (`groebner_gate`: 2×2 lineal exacto, >2×2 `Err` → Eliminate) | CAS Groebner | S cerrado; Buchberger = L (F10.W5) |
| PDF | `symbolic/exchange.rs` (`document_to_pdf` 1.4 mínimo, 1 pág.); vectorial `printpdf` pendiente lead en `app/src/export.rs:3850-3880` | export PDF | M parcial (bloqueador: dep `printpdf` por crate) |
| CSV RFC 4180 | `symbolic/csv.rs` (`to_csv` CRLF + `parse_csv` con `""`, cotas 20k filas/10M) | import/export CSV | S cerrado (wiring UI P2) |
| Clipboard SVG/PNG | `symbolic/exchange.rs` (SVG real punto/círculo/polígono/texto; PNG `Err` honesto) | copiar SVG/PNG | S+M parcial (bloqueador: raster `image`/`tiny-skia` en app) |
| Gruntz / Risch / marching cubes / Net / iroh / CRDT | `symbolic/exchange.rs` (`l_stub` siempre `Err` + diseño en mensaje) | CAS y P2P | L solo diseño + stub (F10.W5) |
