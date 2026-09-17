# docs/architecture.md — Grafito v1.1.0 (fuente de verdad: `Cargo.toml`/tag/dist; auditoría F14 2026-09-13; paridad GeoGebra: subset verificado, resto stub honesto)

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
- `ValidatedDocument` (`validation.rs:40`): wrapper fail-closed `try_new(doc)` que ejecuta `validate_document` antes de persistir o exponer snapshot al render. `Document` ya es `BTreeMap` total en sus mapas de dominio; `semantic_document_baseline` refuerza orden.
- MAX limits: MAX_OBJECT_COUNT 5000, MAX_EXPR_LENGTH 2000, MAX_TRANSFORM_DEPTH 64, MAX_DOCUMENT_SIZE 10M.
- Hash determinista: sorted vars antes de hashear (object.rs:2605 fix), BTreeMap en serializacion (deuda migratoria).
- Transformed try_new valida prepare_function_ast("z") y rechaza "0" singular.

### 4.3 AssistantRuntime (grafito-app/src/assistant.rs)

```
Idle -> Composing -> Thinking -> AwaitingAuthorization -> Animating{job_id} -> Failed | Cancelled
(terminal hoy: Failed; sin Done — verificado AS5; local corre síncrono en UI, resto en thread)
```
> Sin `Verifying`: el preflight de propuestas corre síncrono dentro de `Thinking` (worker, vía `set_proposal_preflight_results` en `grafito-ui/src/assistant.rs`); la variante muerta se eliminó en auditoría en vez de mapear un flag ficticio (test `lifecycle_sin_variantes_muertas`).
- Cada job remoto/proposal/model/agent/anim en thread con CancellationToken y RequestBudget (max_input_chars 8192, max_steps 8, timeout_ms 60000 = 60s — `crates/grafito-assistant-types/src/lib.rs:198-209`; AttachmentLimits max_bytes 512 KiB + max_total_bytes 1 MiB — `lib.rs:245-255`). Excepción: local síncrono en UI. Spark streaming: body cap 256 KiB con parcial `truncated` + aviso (`assistant.rs:2809-2813`); loop agente input cap 48K (`agent.rs:1130`).
- Protocolos por modelo (OpenCodeGo) — `crates/grafito-assistant/src/lib.rs:938-951` (`remote_protocol`): Chat Completions default (`chat_completion_endpoint`, `:899-901`), Responses API `POST {base}/responses` para Muse Spark 1.2/1.3 (`responses_endpoint`, `:905-907`; router `uses_responses_api` = `model.contains("muse-spark")`, `:54-56`; por Chat devuelven 500 instantáneo con cualquier payload, verificado 2026-09-04 — comentario en `:50-53`), Anthropic Messages `POST {base}/messages` para mimo-2.5-vl (`messages_endpoint`, `:915-917`; match en `:946-947`), Fusion (draft+audit) para `fusion` (`:948`; `FUSION_AUDIT_MODEL = "deepseek-v4-pro"`, `:65`). Catálogo vigente 2026-09-08 (docs Zen): `muse-spark-1.3`, `muse-spark-1.2` (pagos) + `muse-spark-1.3-contributor-free` (gratis, exige sesión); IDs viejos siguen ruteando por `contains`. El tier gratuito exige sesión → HTTP 400 `MissingSessionID` hace fallback automático a deepseek con aviso de una línea (`is_session_or_account_error`, `assistant.rs:4725+`). Modo agente con tools aún no soportado en Spark → fallback sólo-sesión a deepseek (`crates/grafito-app/src/assistant.rs:2470-2485`, `:2595-2613`; aviso Responses en `:2916-2917`).
- Modelos: default `deepseek-v4-flash` (`crates/grafito-app/src/utils.rs:59-61`); `qwen3.8-max` / `kimi-k3` sólo como sugerencia en el hint de error de Spark (`assistant.rs:2936`, sin tests dedicados); `mimo-2.5-vl` visión (`assistant.rs:36`, `lib.rs:48`); `fusion` (`lib.rs:49`). Tests wire: `spark_models_route_to_responses_api`, `responses_payload_shape_matches_verified_wire_format`, `responses_wire_uses_bearer_and_response_shape` (`lib.rs:2684-2920`); fallback sesión `assistant.rs:4585-4605`.
- I/O nunca en UI thread; UI solo renderiza &AssistantState.
- **F18 streaming tipado + razonamiento + tokens**: el preview monolítico se retiró; el motor emite `StreamDelta { Reasoning | Text | Status }` por `stream_progress_sender` (sufijos por flujo, `try_send` best-effort sobre canal 128) — `crates/grafito-assistant/src/lib.rs` (`StreamDelta`, `stream_progress_sender`, `read_responses_sse_stream`). **Los tres protocolos streamean**: Responses, Chat y Anthropic Messages (`content_block_delta` con `text_delta`/`thinking_delta`, `message_start`/`message_delta` con usage mergeado y `message_stop`; `request_anthropic_completion_streaming`; mimo-2.5-vl ya no espera muda). `reasoning_content` / `response.reasoning_*` alimentan el bloque plegable por turno (`ConversationTurn.reasoning` + `reasoning_ms`, `grafito-assistant-types`; cap `MAX_CONVERSATION_REASONING_CHARS` 8192) y jamás entran al texto final ni al historial remoto. `usage` real (input/output/reasoning/cached/total) se parsea en Responses (`response.completed`), Chat (chunk final con `stream_options.include_usage`), Anthropic y Fusion (`parse_token_usage`), viaja en `RemoteCompletion.usage` y se muestra por turno + sesión; sin datos se oculta (nunca se inventa). Modo razonador opt-in: `AssistantRequest.reasoning_effort` → wire `reasoning_effort` (Chat) / `reasoning.effort` (Responses) con degradación honesta si el proveedor responde 400/422 (`strip_reasoning_knobs` + reintento único en `post_json_with_reasoning_fallback`) + directiva `REASONING_BINDING_DIRECTIVE` en el system prompt (cubre el modo agente). Buscar en internet opt-in: `AssistantRequest.web_search` → worker pre-flight `web::web_search` (DuckDuckGo HTML + fallback Instant Answer, sin claves, 5 resultados, snippet 400, timeout 8s, `web.rs`), contexto inyectado al prompt (cap `MAX_WEB_CONTEXT_CHARS` 4096 contado al input) y tool `web_search` opt-in en el agente (`agent.rs`; fuera de `all_safe_tool_schemas` a propósito). UI: disclosure por turno con auto-expand mientras piensa y auto-colapso al responder (`reasoning_open` por id; header `secondary` en vivo → `tertiary` quieto), chip de tokens con hover, total de sesión en el header, botón ghost **Copiar** en el turno cerrado, chips **Razonar**/**Buscar** (ghost pill `composer_mode_chip`, piso táctil 24px) en el composer y checkboxes en Configuración (`AssistantUiAction::ReasoningModeChanged/WebSearchChanged`, persistidos en `AppConfig.assistant_reasoning_enabled/assistant_web_search_enabled`). Fix visual "chat oculto": `scroll_to_cursor` forzado cuando el transcript sigue el fondo, tolerancia 2px, `ASSISTANT_TRANSCRIPT_MIN_HEIGHT` 56px que capa el composer en viewports chicos. Perf: `AssistantBlocksCache` devuelve `Arc<Vec<...>>` (hit sin clonar), el reveal no clona el contenido del último turno y `humanize_prose_text` devuelve `Cow` (sin copia cuando no hay identificadores de control).

### 4.4 Interval/ExprEval (grafito-geometry)

```
Raw -> Parsed -> Validated -> Evaluated | Failed
```
- Cache LRU 128, midpoint lo+(hi-lo)/2, safe_sample valida finite y MAX_SAMPLES 100k.

## 5. Type-Safety

- **AnimJobId**: newtype String try_new regex ^[A-Za-z0-9_-]{1,64}
- **Resolution**: try_new 64..=4096, as_tuple()
- **AnimDuration**: try_new 0.1..=60s (P0.1 long-form; `protocol.rs:263`), as_millis()
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
- **App shell** (grafito-app/src/app.rs 9275L, assistant.rs 14429L, panels.rs 9442L, anim_native.rs 15915L, ui/assistant.rs 18616L, command/command_registry.rs 9941L y command/commands.rs 32751L, medidos 2026-09-17 con `wc -l`): eframe::App::update dispatch (god function, deuda P1), GrafitoApp 136 campos (god object), `MAX_UNDO` 50 + `MAX_UNDO_BYTES` 50 MiB con `VecDeque<Document/ChangeSet>` (`pop_front` O(1), `Vec` previo era O(n) shift — corregido), `controllers.rs` reales `DocumentController/ViewController/AssistantController` con `VecDeque` + tests (F14: `GrafitoApp` ya delega por puente `from_parts`/`into_parts` en snapshot/undo/redo/replace/view/examen; wiring fino P2), `AssistantTurnState` en `assistant_jobs.rs` (derivado del runtime con `derive_from`; sin consumidor productivo — P2), ViewMode/Perspective/CanvasMode redundancia (deuda P1), repaint intervals 150ms settle, 33ms multidimensional 30Hz, 16ms whiteboard 60Hz. Animación sin Python (`engines/python` eliminado del árbol): 13 plantillas nativas (11 protocolo + `subspace` + `fractal`, sync 13↔13↔13 en `protocol.rs::CANONICAL_TEMPLATES` ↔ `anim_native.rs::NATIVE_TEMPLATES` ↔ `anim_ui.rs::PLANTILLAS_COMBO`) + MP4 vía ffmpeg-sidecar (`anim_native.rs:481`, `FfmpegMissing` honesto sin `ffmpeg` en PATH). CAS experto opt-in: feature `cas-nativo` con `alkahest-cas 3` (`grafito-assistant/Cargo.toml:44`).
- **Atajos verificados** (handlers en `grafito-app/src/shortcuts.rs` — F14 los movió de `app.rs`; menús en `ui.rs:141-227`; etiquetas toolbar en `grafito-ui/src/toolbar.rs:36-156`): Ctrl+N/O/S + Ctrl+Shift+S archivo (`lifecycle.rs:20-31`), Ctrl+Z/Y deshacer/rehacer (+Shift en Ctrl+Y = herramienta YIntercept, `shortcuts.rs:25-34`), Supr eliminar, Esc cancelar (overlay-aware con `consume_key` + fallback único, F14), F1-F6 herramientas 2D (`shortcuts.rs:41-65`), F8 Esfera 3D + F9 Cubo 3D (`shortcuts.rs:69-74`), R/E/I/X/N/S/Y/V/M/G herramientas sin modificadores, Ctrl+A Analizar, Shift+L/K/J toggles log X/Y/ambos, G snap, Ctrl+K paleta, Ctrl+T tema (`shortcuts.rs:195`), Ctrl+P Lápiz + Ctrl+E Borrador (`shortcuts.rs:204,209`), Ctrl+Shift+1..9,0 perspectivas (10, `shortcuts.rs:169-183`). Cero fantasmas desde BUILD 2026-09-04.
- **Responsive shell**: rail 68px (`RAIL_WIDTH`, `tokens.rs:164`; items 60px insetados 4px por lado para que el borde no clipee; `ui.rs:549-552`) visible sólo en Medium/Wide (≥1360, `lib.rs:417-424,441-442`) — colapsado en Compact, luego también <780px; drawer derecho 292..440 con clamp (`clamp_drawer_right_width`, `tokens.rs:207-210`; dock 3D `ui.rs:727-731`; Inspector `panels.rs:2125-2132`); panel izquierdo min 180 + max 45% viewport (`PANEL_LEFT_MIN`, `PANEL_LEFT_MAX_FRACTION`, `tokens.rs:151-154`; `panels.rs:1201-1206`).
- **Onboarding** (`app.rs:1763`, `:4922-5033`; `utils.rs:46-48`): gating `show_onboarding = !config.onboarding_completed`; Window 420px, 3 bullets dibujados (primary/secondary/tertiary, `app.rs:8277-8290`); el copy "18 grupos" (5/8/18) vive en `bullet_university` (`i18n.rs:247`) con claves en 6 idiomas pero sin uso — deuda menor honesta; botones [Probar ejemplo][Empezar vacío][No mostrar]; Probar ejemplo y No mostrar persisten `onboarding_completed=true`.
- **Paleta de comandos** (`grafito-ui/src/command_palette.rs`): fuzzy subsecuencia sin tildes (`fuzzy_match`, `:224-251`), bilingüe es/en (`filtered_commands`, `:275-296`), footer en español con conteo "N de M · ↑↓ navegar · Enter abrir · Esc cerrar" (`:394-403`), 15 acciones UI en español con clave inglesa estable (`UI_ACTIONS`; test actualizado).

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
| Anim | AnimDuration | 0.1..=60s (P0.1 long-form) | anim/src/protocol.rs:263 |
| Anim | Resolution | 64..=4096 por lado | anim/src/protocol.rs (tests :3366-3371) |
| Anim | VIDEO_LONGFORM_MAX_FRAMES | 1500 (Mp4/Webm; Gif/PngSequence cap 64) | anim/src/protocol.rs:341,345,438 |
| Anim | PREVIEW_SHORT_MAX_FRAMES (GIF) | 64 | anim/src/protocol.rs:341 |
| Anim | LONGFORM_CHUNK_MAX_BYTES | 64 MiB (streaming por chunks, sin Vec total) | anim/src/protocol.rs:349,364-389 |
| Anim | MAX_TIMELINE_DURATION_MS | 60_000 (60 s) | anim/src/protocol.rs:931 |
| Anim | AudioTrack offset_ms / gain | 0..=60000 / 0.0..=2.0 finita | anim/src/protocol.rs:468,470,503-543 |
| Anim | CAPTION_MAX_OUTPUT_BYTES (SRT/ASS) | 256 KiB | anim/src/captions.rs:33 |
| Anim | VOICEOVER_MAX_PALABRAS (por paso) | 40 | anim/src/guion.rs:64 |
| Anim | SHORT_MIN/MAX_PALABRAS (short total) | 110 / 130 | anim/src/guion.rs:66,68 |
| Anim | run_command texto | no vacío, ≤2000 chars, sin NUL, una línea | assistant-types/src/lib.rs:17 + assistant/src/agent.rs:2381-2399 |
| Assistant | RequestBudget max_input_chars | 8192 | assistant-types/src/lib.rs:201 (+validate cap :214) |
| Assistant | RequestBudget max_output_chars | 2048 | assistant-types/src/lib.rs:202 (+validate cap :217) |
| Assistant | RequestBudget max_steps | 8 | assistant-types/src/lib.rs:203 (+validate cap :220) |
| Assistant | RequestBudget timeout_ms | 60000 (60s) | assistant-types/src/lib.rs:204-206 (+rango 100..=120000 :223) |
| Assistant | AttachmentLimits max_bytes | 512 KiB | assistant-types/src/lib.rs:248 |
| Assistant | AttachmentLimits max_total_bytes | 1 MiB | assistant-types/src/lib.rs:251 |
| Assistant | AttachmentLimits max_pixels / max_total_pixels | 1 MiP / 2 MiP | assistant-types/src/lib.rs:249,252 |
| Assistant | AttachmentLimits max_attachments | 2 | assistant-types/src/lib.rs:250 |
| Assistant | ConversationTurn reasoning cap | 8192 (`MAX_CONVERSATION_REASONING_CHARS`, espeja `REASONING_MAX_CHARS` del motor) | assistant-types/src/lib.rs + assistant/src/lib.rs |
| Assistant | Web search | 5 resultados / snippet 400 / query 256 / contexto 4096 / timeout 8s, sin claves | assistant/src/web.rs |
| Assistant | Fallback razonador | 400/422 con "reasoning" → 1 reintento sin el campo | assistant/src/lib.rs (`strip_reasoning_knobs`, `post_json_with_reasoning_fallback`) |
| Assistant | tools seguras (`all_safe_tool_schemas`, assistant) | 21 (3 base + 8 pedag + 8 math + 2 harness1) | assistant/src/agent.rs:2250-2259,2721-2766 (math 8 verificado por conteo `ToolSchema::new`) |
| Assistant | tools seguras (`all_safe_tool_schemas`, agent hoja) | 9 (3 base + 6 pedag, sin math/harness) | agent/src/tools.rs:3060-3081 |
| Tex | TEX_INPUT_MAX_BYTES | 8 KiB | tex/src/lib.rs:37 |
| Comandos | COMMANDS registrados | 639 (`command!(`) | command/src/command_registry.rs (blindaje `registry_counts_match_documented_architecture`) |
| Comandos | palette-visible | 599 (40 ocultos) + 15 acciones UI = 614 en paleta | command_registry.rs + grafito-ui/src/command_palette.rs (R3.1: Rename stub→visible; 3D-A2: +Vista3D; P0.2: +41 álgebra; P0.3: +7 complejo; P0.4: +3 demostración; P0.5: +8 optimización+gráficos; P1: +82 listas/estadística/probabilidad; P1b: +1 List persistible; P2: +25 medidas/geometría; P3b: +11 scripting + Execute real; P4: +105 CAS/listas/geometría/stats/display; P5: +18 cierre nominal (Slope/SetValue/GroebnerLexDeg/SD/SampleSD/SampleVariance/SetSeed/Seed/Turtle*)) |
| Comandos | categorías visibles | 25 (`VALID_CATEGORIES`, registry.rs:3664-3690) | command_registry.rs (G-F audit) |
| Toolbar | ToolGroupId / UNIVERSITY | 18 (PRIMARY 5, SECONDARY 8) | grafito-ui/src/toolbar.rs:263-284 + UNIVERSITY_TOOL_GROUPS :348-365 (+tests :1865-1868; F3a 17→18) |
| Toolbar | ToolGroupId / ALL_GROUPS | 15 clásico intencional (UNIVERSITY 18 suma Dynamics/ThreeD/FourD; disclosure progresivo, no bug) | grafito-ui/src/toolbar.rs:298-315 |
| Toolbar | Tool variantes | 87 | grafito-ui/src/lib.rs `pub enum Tool` (contado F5, 87 variantes; Parallel/Arc/Sector F9 ya incluidos) |
| App | Perspectivas | 10 (Ctrl+Shift+1..9,0) | grafito-app/src/lib.rs:90-111 + app.rs:4236-4242 |
| Workspace | crates | 18 members (tex incluido) + root; 19 dirs en `crates/` | `Cargo.toml` members (app, agent, anim, assistant, assistant-types, classroom, command, complex, core, geometry, ggb, pedagogy, plugins, profile, render, tex, ui, whiteboard) + `crates/grafito-release-tests/` no listado |
| UI | BREAKPOINT_COMPACT | 1360 | tokens.rs:142 (is_compact_viewport :188-191) |
| UI | PANEL_LEFT_DEFAULT | 260 (min 180, max 45% viewport via PANEL_LEFT_MAX_FRACTION) | tokens.rs + panels.rs/algebra.rs |
| UI | PANEL_LEFT_MIN | 180 | tokens.rs |
| UI | PANEL_LEFT_MAX_FRACTION | 0.45 (clamp + panel_left_max_width) | tokens.rs |
| UI | DRAWER_RIGHT_DEFAULT | 344 (min 292, max 440) | tokens.rs |
| UI | DRAWER_RIGHT_MIN | 292 | tokens.rs |
| UI | DRAWER_RIGHT_MAX | 440 | tokens.rs |
| UI | RAIL_WIDTH | 68 | tokens.rs + ui.rs |
| UI | TOP_BAR_HEIGHT | 48 | tokens.rs |
| UI | SPLASH_LOGO_SIZE | 128 | tokens.rs |
| UI | ASSISTANT_PANEL width | 300..520 (default 400); bottom-sheet si viewport < 740 | assistant.rs:99-105 + `assistant_uses_bottom_sheet` :4408 |
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
| 2 | `test` | `cargo test --workspace --all-targets --all-features --locked` | matrix 1.92 + stable |
| 3 | `gpu-compute` | `cargo test -p grafito-render --test gpu_compute --locked` **required** | `WGPU_BACKEND=vulkan`, `GRAFITO_REQUIRE_GPU_TESTS=1`, `mesa-vulkan-drivers` + `libvulkan1`, no longer `WGPU_BACKEND=gl headless` ni SKIP; falla si GPU no disponible |
| 4 | `examples` | `cargo check --workspace --examples --locked` | stable |
| 5 | `benches` | `cargo check --workspace --benches --locked` | stable (separado de examples desde 14-job split) |
| 6 | `docs` | `cargo doc --workspace --no-deps --locked` con `RUSTDOCFLAGS=-D warnings` | stable |
| 7 | `lint` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | matrix 1.92 + stable, components clippy |
| 8 | `release-build` | `cargo build --workspace --release --locked` | PR \|\| push main (`if: pull_request \|\| push refs/heads/main`), ubuntu-22.04 |
| 9 | `fmt` | `cargo fmt --all -- --check` | stable, rustfmt |
| 10 | `all-targets` | `cargo check --workspace --all-targets --all-features --locked` | stable |
| 11 | `cross-platform-smoke` | `cargo test -p grafito-app --test app_smoke --locked` | matrix os: ubuntu/windows/macos, fail-fast false |
| 12 | `supply-chain` | `cargo audit 0.22.2` + `cargo deny 0.20.2` + `verify_advisory_exceptions.py` | cache cargo-tools |
| 13 | `workflow-lint` | `actionlint 1.7.7` + `shellcheck` + `bash -n` + `bash packaging/tests/packaging-fixtures.sh` + Debian version mapping (`1.2.20~beta < 1.2.20`) | valida packaging fixtures como gate |
| 14 | `coverage` | `cargo llvm-cov --workspace --all-targets --all-features --locked` con gate `--fail-under-lines 71`, semanal/manual (`schedule` lunes 03:00 / `workflow_dispatch`, verificado en `ci.yml:368-372`), SIN fallback `cargo test` (F5: duplicaba el job `test`; GPU required con `GRAFITO_REQUIRE_GPU_TESTS=1`, ya no SKIP) | stable + llvm-tools-preview, artefacto lcov 14 días |
| 15 | `bench-regression` | `cargo bench --workspace --benches --locked` (criterion, baseline `main` INFORMATIVO sin gate >10% hasta baseline estable; F5 agrega benches SSE-truncado + frames nativos integral/morph con numero base impreso). **Solo `schedule` + `workflow_dispatch` (`ci.yml:450`): NO es gate de PR; sin comparador `critcmp` por diseño** (flaky entre runners; ver Notas) | stable, artefacto target/criterion 7 días |
| 16 | `mutation` | `cargo mutants 24.11.1 --workspace --timeout 60 --in-place` (semanal a proposito: 60min + muta el arbol, no apto como gate de PR). **Solo `schedule` lunes 03:00 + `workflow_dispatch` (`ci.yml:499`): NO es gate de PR** | sólo `schedule` semanal / `workflow_dispatch`, 60 min |
| 17 | `package-debian` | `desktop-file-validate` + `packaging/build-deb.sh` + `dpkg-deb --info/--ctrl-tarfile` ownership `root:root`, permisos, `lintian --fail-on error`, `dpkg --install` + `/usr/bin/grafito --help` + purge | ubuntu-22.04, Needs `dpkg-dev lintian desktop-file-utils` |

Notas:
- Ola 4 honesto: `coverage`, `bench-regression` y `mutation` son `schedule` /
  `workflow_dispatch`, NUNCA gates de PR (verificado `ci.yml:372,450,499`).
  `bench-regression` no tiene comparador `critcmp`: comparar >10% de verdad
  exigiría baseline estable + descarga del artefacto de `main` y es flaky entre
  runners; hasta tenerlo el job es informativo (compila, corre harness `--test`,
  imprime números base, sube artefacto) sin gate.
- `gpu-compute` ahora **requerido** con `WGPU_BACKEND=vulkan` (antes `gl` headless SKIP); `GRAFITO_REQUIRE_GPU_TESTS=1` hace fail-closed si el adapter no esta disponible.
- Packaging fixtures (`packaging/tests/packaging-fixtures.sh`) es gate en `workflow-lint`: verifica iconos `16..512` + scalable `hicolor/scalable/apps/grafito.svg`, `grafito-icon.svg`, abort si falta asset, y `desktop Icon=grafito`, mas plugins `usr/share/grafito/plugins` (`j-space`), `postrm` parse, MSRV 1.92 docs, MSVC static CRT, e icon asset existencia; `assets/mora.png/.svg` existen y se embeben via `include_bytes!` (verificado en `app.rs:4870` test `<32 KiB`).
- Baseline 2026-08-20: 7/8 PASS (gpu_compute SKIP headless, release-build SKIP 45m). Desde 14-job split: gpu-compute ya no SKIP, package-debian y workflow-lint son blocking. F14 2026-09-13: `ci.yml` real 608 líneas, gate coverage 71% (no 75), `release-build` corre también en PR, `mutation` limitado por `cargo-mutants.toml` a 5 crates (`core/geometry/command/render/app`).
- Perf Ola 5/6 (2026-09-17): mediciones, decisiones y negativos en `docs/profiling.md` (walk vs flat, mimalloc **revertido**, `desired_maximum_frame_latency: 1`, GPU async, RAM); PGO reproducible con `scripts/pgo.sh` (`--interactive`, `--install`, fail-closed si no hay `llvm-profdata`); spike egui 0.32 diferido con conteo real de rupturas en `docs/adr-0004-egui-032-spike.md`; `bench-regression` compila además `expr_bench`/`ode_bench`/`cas_matrices_bench`/`native_rgba` (sigue informativo, sin `critcmp`).

## 10. Snapshot histórico v1.2.35-beta — Paridad parcial GeoGebra: subset verificado, resto stub honesto (2026-08-26; la versión vigente es 1.1.0)

**Pedagogía multi-nivel (primaria→ingeniería)**
- `grafito-pedagogy::Curriculum` 45 LOs: UTN AM1 8, AM2 7, Álgebra 7, Prob 6, Secundaria 12, Primaria 5 (level_min, requires DAG, tags, topological_order Kahn)
- `UdlProfile {Concrete,Graphic,Symbolic,Formal}` + `level_value()` (Primary 2, Secondary 8, AM1 12...)
- `SocraticFsm` Review→HeuristicQ→AwaitStudent→Rectify→Summarize (Telling<5%), `ScaffoldEngine` ya usa `history`
- `ExerciseGenerator::generate_with_seed` wyhash paramétrico (a,b,c) + `ValidatorKind` + `FeedbackEngine` 10 misconceptions (Sign/Distributive/ChainRule/Fraction/Domain/Notation/Exponent/Algebra/Concept/None)

**Perfil adaptativo**
- `BKT` bayesiano + `Scheduler` Leitner `next_interval=86400*2^(level-1)*(2-mastery)` → `BranchState {next_review_epoch, box_level, bkt_p_known}` + `branches_due()` + `recommend_next_with_scheduler()`
- `WorkingMemory` sesión (steps_tried, misconception_counts) sincronizado `assistant.rs:520` con `StudentProfile`

**Asistente OpenCode Go socrático**
- `PedagogyDispatcher` 6 tools puras (`scaffold`, `generate_exercise`, `assess_answer`, `get_curriculum`, `suggest_next`, `generate_animation`) + 3 base = 9 `all_safe_tool_schemas()` OpenAI-compat (`agent.rs` 680L, 14 tests)
- `TeachingTopic` 14 variantes, `teaching_ui::whiteboard_elements_for_hint` 14 mappings (fracción, vector, matriz, prob, serie, trig, cónica...), `anim_native` templates pedagógicos, `AssistantExerciseCard` inline `grafito-exercise`

**UI Scandinavian sin laberinto**
- `toolbar.rs` `PRIMARY 5` `SECONDARY 8` `UNIVERSITY 18` (F3a suma Transform; `ALL_GROUPS` 15 diverge — ver §8) + `toolbar_groups_for_level_value(u32)` + `udl.rs` helper sin depender de pedagogy; `filter_groups_by_level`
- `AssistantPanelState` `max_composer 88..260` quiet, tokens 64%/44%/10%/5% documentados, `whiteboard:WhiteboardDoc` persistente en `Document` (serde, cota 500 elementos), `spreadsheet` `=A1+B1` stripping, `AppConfig::onboarding_completed`

**Gates:** `cargo fmt 0` `clippy -D warnings 0` `test --workspace ~1500 verdes`

## 11. Riesgos residuales

- BTreeMap determinismo total (cerebro-audit 2026-09-10): `objects`, `next_label_number`, `spreadsheet_coordinate_points`, `variables` (`VarMap`), `variable_meta` (`VarMetaMap`), `live_sequences`, `variables_assumptions` + params de constraints (`Constraint.params`, `Document` params fns) + `Exercise.params` son BTreeMap; ~210 firmas `&HashMap<String,f64>` migradas en core/geometry/command/render/complex/pedagogy/app (+ tests/benches/examples). Quedan `HashMap` solo fuera del dominio variables: índice solver `var_index`, mapas tipados `DD`/`Complex64`, intervalos simbólicos, escalares de planilla, cachés UI.
- `Transformed` Jacobian det pendiente
- `fill_compute` aún `None` (ahorra 128 MiB, habilitar lazy si `ImplicitCurve != Eq`)
- App God Object `app.rs 9275L` + `assistant.rs 14429L` (medido 2026-09-17): `controllers.rs` reales con tests y `GrafitoApp` delega por puente `from_parts`/`into_parts` (wiring fino P2); `AssistantTurnState` vive en `assistant_jobs.rs` y deriva del runtime, aún sin consumidor productivo (P2)

## 12. Próximos pasos

1. `ValidatedMatrix` (la migración `Document {variables, variable_meta, live_sequences, variables_assumptions}` a `BTreeMap` ya se completó 2026-09-10)
2. `WhiteboardDoc` export SVG + `GeoObject::Whiteboard` independiente
3. `fill_compute` lazy + `DomainColoring` en export
4. Classroom P2P opt-in (feature flag)

## 13. Tabla doc→código (BUILD 2026-09-04 — todo número con origen verificado)

| Afirmación en docs | Origen verificado en código |
|---|---|
| RequestBudget 8192 / 2048 / 8 / 60s | `crates/grafito-assistant-types/src/lib.rs:198-209` |
| AttachmentLimits 512 KiB / 1 MiB / 1-2 MiP / 2 adjuntos | `crates/grafito-assistant-types/src/lib.rs:245-255` |
| 639 comandos (`command!(`), 599 visibles + 15 UI = 614 en paleta | `crates/grafito-command/src/command_registry.rs` (R3.1: Rename visible; 3D-A2: +Vista3D; P0.2: +41 álgebra; P0.3: +7 complejo; P0.4: +3 demostración; P0.5: +8 optimización+gráficos; P1: +82 listas/estadística/probabilidad; P1b: +1 List persistible; P2: +25 medidas/geometría; P3b: +11 scripting + Execute real; P4: +105 CAS/listas/geometría/stats/display; P5: +18 cierre nominal (Slope/SetValue/GroebnerLexDeg/SD/SampleSD/SampleVariance/SetSeed/Seed/Turtle*)) |
| 15 acciones UI + fuzzy + footer es | `crates/grafito-ui/src/command_palette.rs` |
| 18 grupos toolbar (PRIMARY 5, SECONDARY 8, UNIVERSITY 18; ALL_GROUPS 15 diverge — ver §8) | `crates/grafito-ui/src/toolbar.rs:263-284,298-315`, tests `:1865-1868` |
| 87 herramientas (`Tool`) | `crates/grafito-ui/src/lib.rs` `pub enum Tool` (contado F5: 87 variantes) |
| 10 perspectivas (Ctrl+Shift+1..9,0) | `crates/grafito-app/src/lib.rs:90-111`, `shortcuts.rs:169-183` |
| 18 crates workspace | `crates/` (ls: +classroom R5, +ggb F9) |
| 17 jobs CI | `.github/workflows/ci.yml` (608 líneas; gate coverage 71%) |
| Spark vía Responses API (`POST {base}/responses`) | `crates/grafito-assistant/src/lib.rs:50-56`, `:905-907`, `:938-951` |
| Ruteo Go por tabla `go_model_protocol` (docs Go 2026-09-13): spark/gpt/grok→Responses, minimax/qwen3.6-3.8→Messages, resto→Chat | `crates/grafito-assistant/src/lib.rs` (tabla + test `go_model_routing_table_matches_go_docs_endpoints`); agente usa la misma tabla |
| Catálogo Go vigente incl. `deepseek-v4.1-flash` + botón Actualizar + texto libre | `crates/grafito-ui/src/assistant.rs` (`OPENCODE_MODELS`, `RefreshModels`, `model_draft`, `sanitize_custom_model_id`); auto-refresh al cambiar proveedor |
| `max_tokens` Chat con headroom ×2 para razonamiento (piso 1024) | `crates/grafito-assistant/src/lib.rs:completion_token_limit_for_chars` + `loop_engine.rs:completion_token_budget`; test `chat_token_limit_doubles_chars_with_reasoning_headroom` |
| Fallback agente con pregunta guardada + toast de truncado + body de sesión | `assistant_jobs.rs` (`AssistantAgentJob.question`, `outcome.truncated`); `agent.rs:agent_http_status_error` (cuerpo cap 200) |
| F20: `reasoning.summary=auto` siempre en Responses + budget ×4 (verificado contra el endpoint real: sin summary el item llega con `summary:[]` y un "2+2" consume ~2045 reasoning tokens → `incomplete`) | `crates/grafito-assistant/src/lib.rs` (`build_responses_payload`, `responses_token_limit_for_chars`, test `responses_reasoning_summary_always_requested_with_quadrupled_budget`) |
| F20: etapas sin jerga (`Pensando…`, sin KiB) + tarjeta de espera oculta con contenido en vivo | `crates/grafito-ui/src/assistant.rs` (`RemoteStage::label`, `pending_card_visible`, `provisional_turn_has_visible_content`) |
| F20: composer sólo-iconos + teclado con foco y toggle persistente + composer sin recorte | `crates/grafito-ui/src/icons.rs` (`Send/Brain/Copy/Paperclip`), `assistant.rs` (`composer_icon_toggle`, BASE 100, sheet con piso usable), `app/src/keyboard.rs` (inserción según `composer_focused`, `keyboard_visible_explicit`) |
| Modelo default `deepseek-v4-flash` | `crates/grafito-app/src/utils.rs:59-61` |
| Fallback sesión spark→deepseek | `crates/grafito-app/src/assistant.rs:2470-2485`, `:2595-2613` |
| Ctrl+T tema | `crates/grafito-app/src/shortcuts.rs:195` + menú `ui.rs:261` |
| Ctrl+P/E lápiz/borrador, F8/F9 esfera/cubo | `crates/grafito-app/src/shortcuts.rs:69-74,204,209` |
| Onboarding 420px, 3 bullets, 3 botones | `crates/grafito-app/src/app.rs:7910`, gating `:2358`, `utils.rs:49` |
| Rail 68px, drawer 292..440, panel izq 180+45% | `crates/grafito-ui/src/tokens.rs:151-164,207-210`; `app/src/ui.rs:549-552,727-731`; `app/src/panels.rs:1201-1206,2125-2132` |
| 21 tools assistant (3 base + 8 pedag + 8 math + 2 harness1) / 9 en agent hoja | `crates/grafito-assistant/src/agent.rs:2250-2259` (pedag 8), `:2721-2766` (harness1 2 + base 3), math 8 por conteo `ToolSchema::new`; `crates/grafito-agent/src/tools.rs:3060-3081` (3+6=9) |
| AnimDuration 0.1..=60s, Resolution 64..=4096 | `crates/grafito-anim/src/protocol.rs:263`, tests `:3321-3371` |
| Long-form 1500 frames (GIF 64), chunks 64 MiB streaming, timeline 60 s | `crates/grafito-anim/src/protocol.rs:341,345,349,364-389,438,931` |
| AudioTrack offset 0..=60000, gain 0.0..=2.0 | `crates/grafito-anim/src/protocol.rs:468,470,503-543` |
| Captions SRT/ASS ≤256 KiB; voiceover ≤40 palabras/paso; short 110-130 | `crates/grafito-anim/src/captions.rs:33`; `guion.rs:64,66,68` |
| run_command ≤2000 chars + allowlist/undo (solo no destructivos) | `crates/grafito-assistant-types/src/lib.rs:17,988-997`; `assistant/src/agent.rs:2381-2432` |
| solid_measure_3d + generate_short_script (4 beats) | `crates/grafito-assistant/src/agent.rs:1828-1907,2234-2237,2705-2720` |
| Voiceover Piper sidecar honesto + captions sidecar/quemadas | `crates/grafito-app/src/voice.rs:39-56,102-134`; `grafito-ui/src/assistant.rs:997-998,1016-1017` |
| Write por trazo + Rectangle/Ellipse/Arc | `crates/grafito-anim/src/player.rs:23,469,482`; `scene.rs:533-542` |
| LaTeX offline SVG/raster (tex, STIX) + draw_math | `crates/grafito-tex/src/lib.rs:1,157,247,352`; `grafito-ui/src/assistant.rs:10649-10820` |
| 13 plantillas nativas (11 protocolo + subspace + fractal, sync 13↔13↔13) + `media.title` 14 ES/EN/PT/IT/FR/DE | `crates/grafito-anim/src/protocol.rs` (`CANONICAL_TEMPLATES` 13) + `crates/grafito-app/src/anim_native.rs` (`NATIVE_TEMPLATES` 13) + `crates/grafito-ui/src/i18n.rs` (`media.title.*` 14: +`subspace`/`fractal`) |
| 18 members (tex) + root; 19 dirs en crates/ | `Cargo.toml` members + `ls crates/` (release-tests no listado) |
| F18: `StreamDelta` (Reasoning/Text/Status) + `usage` real por proveedor + reasoning plegable por turno | `crates/grafito-assistant/src/lib.rs` (`StreamDelta`, `stream_progress_sender`, `parse_token_usage`, `post_json_with_reasoning_fallback`, `request_anthropic_completion_streaming`); `crates/grafito-app/src/assistant.rs` (`drain_remote_stream_preview`); `crates/grafito-ui/src/assistant.rs` (`draw_reasoning_disclosure`, `draw_turn_metrics`, `composer_mode_chip`) |
| F18: búsqueda web sin claves (DDG HTML + Instant Answer) | `crates/grafito-assistant/src/web.rs` (23 tests) + tool opt-in `web_search` (`agent.rs`) |

## 14. Paridad GeoGebra 2026 — frente F10-C (BUILD 2026-09-05, rama f10-plan-total)

> Plan de cierre ejecutado 2026-09-16 (P0–P4, ver ADR-0003): 338 → **639 comandos**
> (599 visibles + 15 UI = 614 en paleta), i18n ES/EN/PT/IT/FR/DE (193 claves),
> export `.ggb` + PDF multipágina + P2P iroh tras flag. GeoGebra lista ~502:
> cobertura nominal ≈100% con el resto declarado abajo como stub honesto.

Cerebro puro en `crates/grafito-core/src/symbolic/` (`csv.rs`, `solids.rs`,
`exchange.rs`, `mod.rs` con `groebner_gate`); piel fina en
`crates/grafito-app/src/render_3d.rs` (`OrthoProjection`,
`project_point_ortho`, `solid_measure_text`); helps honestos en
`crates/grafito-command/src/command_registry.rs` (Groebner 2×2, Net L).
Sin tocar geometría exacta, A11Y ni perf; sin `unwrap` (gates §9).

| Categoría | Grafito hoy (archivo) | GeoGebra | Esfuerzo |
|---|---|---|---|
| Capas | `symbolic/exchange.rs` (`LayerTable` 0..=255 + visibilidad) | capas con orden | S cerrado (API; wiring panel P2) |
| Bar/Pie | `command/src/commands.rs` (`BarChart`/`PieChart` reales desde lista o `DataTable[.xs\|.ys]` vía `parse_chart_data_arg`; `exchange.rs` conserva `bar_chart_stub`/`pie_chart_stub` solo como validación sin documento) | BarChart/PieChart | S cerrado (R3.5: rango de planilla) |
| Tabla viva lectura | `symbolic/exchange.rs` (`datatable_rows`/`cell`/`to_csv` sobre `DataTableObj`) | spreadsheet viva | S cerrado (edición P2) |
| Volumen/área 3D | `symbolic/solids.rs` (esfera/cubo/cilindro/cono/toro/tetra/pirámide/prisma exactos; cuádrica → `None` + `solid_measure_status`) | Volume/Area 3D | S cerrado |
| Vistas ortográficas | `symbolic/solids.rs` (`OrthoView` alzado/planta/perfil) + `render_3d.rs` (`OrthoProjection`, píxeles egui) | vistas 3D | S cerrado (cableado cámara P2) |
| Groebner | `symbolic/mod.rs` (`groebner_gate`: 2×2 lineal exacto, >2×2 `Err` → Eliminate); Buchberger real acotado (`geometry/cas.rs:2149`) | CAS Groebner | S cerrado |
| PDF | `app/src/export.rs` (`pdf_page_ops` vía `printpdf 0.12`: rectas/círculos/polígonos/texto Helvetica, 1 página por hoja con contenido, tope `MAX_PDF_PAGES` 64; `document_to_pdf` queda referencia histórica) | export PDF | S cerrado |
| CSV RFC 4180 | `symbolic/csv.rs` (`to_csv` CRLF + `parse_csv` con `""`, cotas 20k filas/10M) | import/export CSV | S cerrado (wiring UI P2) |
| Clipboard SVG/PNG | `app/src/export.rs` (SVG real punto/círculo/polígono/texto; PNG vía `clipboard_png_bytes` + `arboard` "Copiar PNG", headless honesto) | copiar SVG/PNG | S cerrado |
| Gruntz / Risch / marching-tetra / Net / iroh / CRDT | Reales y cableados: Risch-Norman (`geometry/integral.rs`), Gruntz (`geometry/cas.rs:599`; consumido por `Limit` desde F14), marching-tetra (`implicit_surface_compute.rs`), Net (`commands.rs:15598`), Buchberger (`cas.rs:2149`), P2P real `IrohTransport` tras flag `aula-iroh` (`classroom/src/iroh_transport.rs`, ping/pong verificado); stub honesto solo cifrado-sesión/outbox-persistente (`classroom/stubs.rs`) | CAS y P2P | S cerrado (wiring); UI de sala P2P = P3c diferido |

### 14.1 Frente C3 — voz, guion corto, trazo, LaTeX, run_command (sync 2026-09-11)

Puro y acotado, sin tocar geometría exacta ni A11Y; sin `unwrap` (gates §9).

| Pieza | Grafito hoy (archivo) | Nota |
|---|---|---|
| Voiceover Piper | `app/src/voice.rs` (sidecar `piper` en disco; sin binario → `PiperMissing` honesto, jamás silencio; UI `VozMode::Piper` + hint `assistant.rs:997-998`) | MP4 vía ffmpeg-sidecar o mudo honesto |
| Captions | `anim/src/captions.rs` (SRT/ASS ≤256 KiB) + sidecar `.srt` / quemadas vía ffmpeg (`voice.rs:102-134`) | Sin ffmpeg queda el sidecar |
| Short script | `anim/src/guion.rs:933` (`short_script`, 4 beats, 110-130 palabras) + tool `generate_short_script` (`assistant/src/agent.rs:1828-1907`) | Determinista byte-idéntico |
| Write + figuras | `anim/src/player.rs` (`WriteAnim` por trazo) + `scene.rs:533-542` (`Rectangle`/`Ellipse`/`Arc`) | Fallback opacidad honesta sin trazo |
| LaTeX offline | `grafito-tex` (pulldown-latex→formulary→SVG/tiny-skia, STIX embebida, 8 KiB) + `draw_math` (`ui/src/assistant.rs:10649-10820`) | Subset + aviso si falta fuente |
| run_command | bridge puro ≤2000 chars (`assistant-types:988-997`, `agent.rs:2381-2399`); app valida allowlist y aplica con undo, solo no destructivos | Propuesta, nunca ejecuta solo |
| solid_measure_3d | `assistant/src/agent.rs:2705-2720` sobre `symbolic::solids` exactos | Error honesto si no computable |
| Long-form streaming | chunks 64 MiB (`protocol.rs:349,364-389`), 1500 frames, timeline 60 s | Sin `Vec` total en RAM |
