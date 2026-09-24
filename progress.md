# progress.md — Auditoria E2E Completa 2026-08-20

## GOAL
Auditoría TOTAL de inicio a fin + mejoras + nuevas funciones, con foco en generador de animaciones, pensando en cada detalle, trabajo por capas (Cerebro → Piel), statem, type-safety, separación UI.

## Slash Commands activos
- /j-space: Plans.md v2 + Tasks.md v2 + docs/architecture.md antes de código
- /statem: AnimJobState, DocumentLifecycle, AssistantRuntime, Interval/ExprEval
- /rust-design: newtype Resolution/AnimDuration/AnimJobId, Result/Option, clippy -D warnings =0
- /rust-ui: UI solo renderiza &Estado, sin I/O
- /vibecoder-guide: micro-pasos, lenguaje criollo, menú ante error

## Baseline gates (2026-08-20, toolchain 1.92)

| Puerta | Comando | Resultado |
|--------|---------|-----------|
| fmt | cargo +1.92 fmt --check | PASS (0) |
| clippy | cargo +1.92 clippy --workspace --all-targets -- -D warnings | PASS (0, 17s) |
| check | cargo +1.92 check --workspace --locked | PASS |
| examples | cargo +1.92 check --workspace --examples --locked | PASS |
| benches | cargo +1.92 check --workspace --benches --locked | PASS |
| test | cargo +1.92 test --workspace --locked | PASS (387 geo lib + 5 anim + doc-tests 7+1, 0 FAILED tras revert safe_sinh) |
| doc | RUSTDOCFLAGS="-D warnings" cargo +1.92 doc --workspace --no-deps --locked | PASS |
| packaging | bash packaging/tests/packaging-fixtures.sh | PASS (icon 16x16 warn no bloquea) |
| gpu | cargo test -p grafito-render --test gpu_compute | SKIP headless (WGPU_BACKEND=gl) |

## Fixes aplicados (por capas)

### F1 Cerebro — Núcleo lógico
- **object.rs:2605** hash determinista: sorted vars antes de hashear (P0, cache miss espurio)
- **validation + Transformed**: try_new valida prepare_function_ast("z") y rechaza "0" singular, documenta MAX_TRANSFORM_DEPTH 64
- **expr.rs**: COMPILED_EXPR_CACHE LRU 128 (evita DoS), midpoint lo+(hi-lo)/2
- **interval.rs**: safe_sample valida finite y n∈[2,100k], midpoint sin overflow
- **matrices.rs**: zeros/identity debug_assert si excede MAX (1000, 1M)
- **geometry newtypes**: auditados (Radius/Angle deuda P2)
- **core**: hash BTreeMap deuda P0 documentada, CoreError coverage P1

### F2 Anim — Puente (corazón pedido)
- **protocol.rs**: newtypes Resolution (64..4096), AnimDuration (0.1..30s), AnimParams (validate+into_request), canvas 64..8192 (warn >4096), ExportFormat, WireMessage v1
- **engine.rs**: spawn valida NUL/workdir/bin, wait_ready hello+pong deadline 8s poll 250ms, submit can_submit, recv_event filtra job_id, line_cap 64KB drain OOM, shutdown cooperativo 8s, **cancel()** nuevo (Running→Cancelled), **Drop no-bloqueante** (kill+try_wait), diagnostics cap 64, validate_media_path, run_job cancel poll 200ms deadline absoluta (5 tests OK)
- **python __main__.py**: sandbox AST DENIED_NODES (Attribute/Subscript/Lambda) + "__" bloqueado, safe_path relative_to symlink-safe, placeholder hex estable (bytes.fromhex), 4 templates reales (derivative-slope, integral-area, taylor-series, conformal-map), parse_canvas 64..4096, progress 30→60→100, manim fallback con stderr log, py_compile OK
- **anim_native.rs**: 5 templates nativos (derivative, pitagoras, integral, taylor, conformal) + dispatcher render_anim_by_template + tests 5 nuevos (307L)
- **anim_ui.rs**: creado y arreglado: textura id único por frame (fix P0), max_height 84, bar_width 420 clamp, wrap, sin I/O, build_anim_params con Resolution/Duration, tests 3, mod integrado en lib.rs con allow(dead_code)

### F3 Piel — UI
- **grafito-ui**: tokens única fuente, assistant panel composer fix sin ScrollArea envolvente (88..260), wrapping, clip verificado
- **grafito-app**: AssistantRuntime auditado (I/O en thread, Budget), app.rs God Object/Fun deuda P1 documentada, MAX_UNDO 50 deuda P2, ViewMode redundancia P1

### Infra
- **docs/architecture.md**: creado (DAG, statems, type-safety, presupuestos, CI 14 jobs, riesgos)
- **Plans.md v2 / Tasks.md v2**: trabajo por capas documentado

## Verificación de errores de compilador (vibecoder-guide)

**Error 1**: clippy manual_range_contains en AnimDuration y interval → **Menú**: A) usar (a..=b).contains(&x) B) allow(lint) C) revertir. **Elegido A**: fix a contains, clippy pasa.

**Error 2**: clippy needless_if en anim_ui Exportar GIF vacío → **Menú**: A) quitar if B) añadir TODO comment C) allow. **Elegido A**: añadir TODO logico.

**Error 3**: tests FAILED sinh(1e10).is_infinite() tras clamp → **Menú**: A) revert clamp (inf es esperado, safe_sample maneja) B) actualizar tests a expect finito C) clamp condicional. **Elegido A**: revert a a.sinh(), tests 387 PASS. Lección negocio: "safe" no significa esconder inf, sino detectarlo aguas abajo.

**Error 4**: duplicated_attributes allow(dead_code) → **Menú**: A) quitar duplicado en file B) quitar en lib C) allow duplicado. **Elegido A**: quitar #!allow en file, dejar en mod.

**Error 5**: anim_native delimiter extra → **Menú**: A) git checkout + reescribir limpio B) sed fix C) allow. **Elegido A**: checkout + cat heredoc limpio, check pasa.

## Riesgos residuales (OPEN)

- RUSTSEC-2026-0194/0195 quick-xml/zbus expiran 2026-12-31 (deny.toml)
- HashMap no determinista en Document (migrar a BTreeMap, P0)
- Transformed solo check "0" singular (falta Jacobian det, P1)
- AssistantRuntime sin Statem formal (distribuido, P1)
- App God Object 75 campos + update 820L (P1, split en tick/handle/draw)
- MAX_UNDO Vec<Document> clone O(n) shift (P2, VecDeque + delta)

## Evidencia

- cargo test --workspace: 5 anim + 387 geometry lib + doc-tests 7+1, 0 FAILED
- cargo clippy -D warnings: 0
- cargo fmt --check: 0
- packaging-fixtures: PASS
- python -m py_compile: OK

## NEXT

- F1: ValidatedDocument + BTreeMap + CoreError 100% + ValidatedMatrix + cargo test -p grafito-core
- F2: cancel() público + Cancelling estado + transiciones consume Self + templates progress real por frames + mp4
- F3: mover I/O assistant a background thread formal, ThinkingOrb Statem, fix ViewMode
- F4: packaging build-deb.sh release 45m + gpu WGPU_BACKEND=gl

## Sync BUILD 2026-09-04 (docs↔código, ownership exclusivo)

- **Números verificados por lectura directa** (no copiados): RequestBudget 8192/2048/8/60000ms (`assistant-types/src/lib.rs:198-209`), AttachmentLimits 512KiB/1MiB/1-2MiP/2 (`:245-255`), 238 `command!(` (registry `:228`, 199 visibles + 14 UI = 213 en paleta), 26 categorías visibles (30 raw con tildes duplicadas), 17 grupos toolbar 5/8/17 (`toolbar.rs:263-284`, tests `:1317-1319`), 73 `Tool`, 10 perspectivas (`lib.rs:90-111`), 16 crates, 17 jobs CI (`ci.yml:24-496`: +coverage 75%, +bench-regression >10%, +mutation semanal vs 14 documentados).
- **Respuestas/Spark**: `uses_responses_api` = contains "muse-spark" (`assistant/src/lib.rs:54-56`), `responses_endpoint` (`:905-907`), `remote_protocol` (`:938-951`); default `deepseek-v4-flash` (`utils.rs:59-61`); fallback sesión spark→deepseek (`assistant.rs:2470-2485`, `:2595-2613`); qwen3.8-max/kimi-k3 sólo hint (`:2936`, sin tests).
- **Fantasmas eliminados**: Ctrl+P/E y F8/F9 tenían etiqueta (toolbar) sin handler → handlers nuevos en `app.rs:4135-4144` + `:4268-4278`; Ctrl+T tema nuevo (`:4259-4267` + menú `ui.rs:209`). Verificación completa en architecture.md §13.
- **Responsive**: rail 60px sólo Medium/Wide ≥1360 (colapsado <780 incluido); Inspector con max 440 (`panels.rs:2125-2132`, clamp `tokens.rs:207-210`); CAS muerto alineado a tokens; paleta footer "N de M" en español + test `paleta_expone_registro_mas_acciones_ui`.
- Gates pendientes de corrida: fmt + clippy `-p grafito-app -p grafito-ui --all-targets --locked -- -D warnings` + tests lib ambos (ver §13).

## F9 2026-09-05 — 20 agentes + cierres lead (rama f9-completion)
- **W1 perf**: caché geometría por objeto, staging incremental variables, álgebra virtualizada, culling rstar, scheduler reactivo.
- **W2 correctness**: geom_eps(scale), Asymptotic Decider, Prism/Quadric 3D visibles, Delaunay spade, validación relativa.
- **W3 paridad MVP**: sliders canvas (widget), trace flag+trail, Si/&&/||/!, lienzo completo, transformaciones universales.
- **W4 interop/i18n/calidad**: ggb F0-F3 (~85% aula), CSV/drag-drop/SVG-clipboard, i18n 122 claves, telling+banco, trails render.
- **Cierres lead**: Perpendicular honesta (punto+recta>mediatriz), Rastro end-to-end (232 cmds/193 visibles/207 paleta), Parallel/Arc/Sector (76 tools), polygon_n paramétrico, merge-conflicts resueltos, docs sync 232/193/207/76/18 crates.
- **Lección**: reset --hard externo en loop borra trabajo sin commitear → commitear por oleada en rama f9-completion. Auditoría claims-vs-código obligatoria antes de cada oleada.

## Ola 0 2026-09-23 — Frente fantasma + comandos nuevos (gap MathHook/MathCore)

- **Línea de base (re-verificada hoy, cero números copiados)**: 662 comandos (`grep -c "command!("` = 662; blindaje `registry_counts_match_documented_architecture` 1/1 ok), 625 visibles + 15 UI = **640 en paleta**, 25 categorías (`command_registry.rs:10185-10191`; `VALID_CATEGORIES` `:8920-8946`); tools asistente **28** (3 base + 8 pedag + 13 math + 2 harness1 + 2 harness2; asserts `agent.rs:6157,:6901-6903`) y 9 en agent hoja (`agent/src/tools.rs:3073`); MCP `tools/list` **42** / proxied **26** (`protocol.rs:447-448`, `stdio_contract.rs:61-62`, `bridge.rs:252-258`); `cas_steps.rs` 3080 L / `ode.rs` 5194 L (`wc -l`).
- **Gates Ola 0** (4 agentes en paralelo, `.rs` fuera de mi alcance): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo check --workspace --locked` — reportados en verde por la sesión; en esta pasada de docs se re-corrió solo el blindaje del registry (1/1 ok), fmt/clippy NO se re-corrieron acá.
- **Tests corridos hoy (estado caliente del árbol sucio)**: `cargo test -p grafito-geometry --test api_contract` 1/1 (convención textbook de `sylvester_resultant` — `solve.rs:1006-1024`); `cargo test -p grafito-geometry --lib principal_part` 4/4 (los 2 bugs reportados de `laurent_principal_part` ya son tests de regresión en verde, `cas.rs:3128-3151`); `cargo test -p grafito-command --test registry_fantasma_y_nuevos` 12/13 con `resultant_registrado_y_ejecuta` en ROJO (el pin del test quedó en la convención vieja `= -2`; el motor devuelve textbook `2*y^2 - 1`).
- **Regla fija para olas 1-6**: UNA sola representación de expresión — todo algoritmo nuevo sobre `Expr`/bytecode reusando `evaluate_cached` → `compile_flat_ops` → `run_opcodes_flat` (`expr.rs:1019,2256,1976`); nunca portar `Expression` hash-consed (MathHook) ni `Expr` boxeado (MathCore): multi-repo real, reimplementación in-house.
- **Negativos documentados** (`docs/profiling.md` §2.11-2.13): cache de expresiones compiladas 128 vs 512/1024 sin ganancia (W=64: 10.22/9.97/10.23 µs, Δ<3%), `panic="abort"` descartado (rompe `catch_unwind` en producción `bridge.rs:186`/`assistant_jobs.rs:1300`), `Interval::new` ahora redondea outward de verdad (`prec=0` = comportamiento previo).

## Auditoría core/anim 2026-09-24 — FIX 1-6 (grafito-core/grafito-anim + docs)

- **Alcance**: solo `crates/grafito-core/**`, `crates/grafito-anim/**`, `docs/**` (property exclusiva; el resto del workspace estaba siendo editado por otros agentes en paralelo).
- **FIX 1 (ALTA)** — repro previo: `tests_polygon_lazos_autointersectados` en rojo para Lissajous(3,2) (400 pts, δ=π/2 — paridad con el comando `Lissajous`, `commands.rs:12407-12445`) y lemniscata de Bernoulli: "Polygon is degenerate or colinear" pese a ser curvas válidas; rosa de 3 pétalos y lazo de trébol SAFABAN de pedo (su área neta no da exactamente 0: el rechazo dependía de la parametrización). Causa raíz: `validation.rs` usaba el shoelace CON SIGNO (área algebraica neta): en una curva auto-intersecada las regiones se cancelan y el área neta da ~0. Fix: suma de áreas ABSOLUTAS de triángulos abanicados desde el vértice 0 (`area2_abs`, invariante ante traslación y auto-intersección; da 0 sii todos los puntos son colineales) — `validation.rs:971-1001`. Degenerado genuino (colineales, también en orden no monótono) sigue rechazándose.
- **FIX 2 (MEDIA)** — repro previo: `srt_frena_al_cruzar_el_tope…` medía `SalidaMuyGrande { bytes: 472893 }` (SRT) y `ass_frena…karaoke…` `309696` (ASS): la cota de 256 KiB se chequeaba DESPUÉS de construir toda la salida. Peor caso de wire alcanzable vía `Deserialize`: 2000 segmentos × 500 palabras × 200 chars ≈ 200 MB transitorios. Fix: `empuja_acotado`/`empuja_karaoke` (`captions.rs:350-377`) chequean ANTES de cada push (el acumulado jamás supera el tope; el karaoke escribe palabra por palabra). Tests: el `Err` llega con el proyectado ≤ tope + 1 KiB + `empuja_acotado_nunca_supera_el_tope` (contador de bytes acumulados). Salida byte-idéntica para payloads chicos (tests preexistentes en verde).
- **FIX 3 (MEDIA)** — repro previo: `short_script_usa_las_13_canonicas` en rojo ("crecimiento exponencial" → `universal` en vez de `euler`) y `short_script_varia_los_beats_por_familia` en rojo (hooks idénticos: "Che, mirá esto con atención: parece magia…"). Fix: allowlist = las 13 `CANONICAL_TEMPLATES` + copy (texto + voz + pizarra) por familia de concepto (6 familias: cálculo, complejo, dinámica, series, forma, universal) con la MISMA estructura de presupuestos (4 actos / 6 pasos / 48 frames / 25.7 s, voz 111-121 palabras en 110..=130) — `guion.rs:931-1157`. Golden byte-a-byte: `short_script_golden_hash` pinea hash FNV-1a 64 del `GuionTexto` serializado para los 13 conceptos (`guion.rs:1808`).
- **FIX 4 (MEDIA)** — repro previo: `peor_caso_compuesto_err_honesto_en_vez_de_cien_mb` en rojo — `try_play` devolvía `Ok` con 96 frames compuestos (16 capas × 4096 pts ≈ 100 MB; con 32 capas el techo era ~192 MB) clonando la escena base CADA frame; el único "presupuesto" (`frames × 16 × 512` en `try_play`) nunca llegaba a 64 MiB con ≤96 frames. Fix: presupuesto propio `PLAYER_MAX_TOTAL_BYTES` 64 MiB lógicos estimados (`estimate_placed_bytes`/`empuja_frame`, `player.rs:307-372`) enforceado durante el compuesto: `try_play` → `Err(PresupuestoExcedido)` honesto; `play` (modo total) corta con lo ya jugado dentro del tope. **Fondo compartido NO hecho (decisión documentada)**: un `Arc<[PlacedMobject]>` por frame no ahorra las copias — cada frame es fondo+animado y el slice plano se materializa igual; el ahorro real exige representación de dos partes (fondo `Arc` + extras) o raster incremental, y ambos cambian el contrato `PlayedFrame.objects` con la Piel (`grafito-app/src/assistant.rs:7407` consume `&cuadro.objects` como `&[PlacedMobject]`), fuera del alcance de esta sesión.
- **FIX 5 (BAJA)** — (a) protocolo v1 duro `== 1` (`UnsupportedVersion{min:1,max:1}`): ahora rango real `1..=2` (`protocol.rs:35-42,1213-1222`); un hello v2 con campos nuevos se tolera degradando a v1 con diagnóstico (repro: test engine con stub v2 caía en timeout/"version_mismatch"), y v99 → `Error` tipado "versión de protocolo incompatible" (antes la línea se descartaba y el síntoma era un timeout mudo). (b) `Ok(_) => {}` en `wait_ready` descartaba mensajes inesperados sin traza: ahora `anota_diagnostico` (`engine.rs:451`, `:520-528`, `:572-580`; repro: `diagnostics()` venía `[]`). (c) `math_expr`: `MATH_EXPR_MAX_CHARS` (chars) → `MATH_EXPR_MAX_BYTES` (bytes) para unificar la unidad con `grafito-pedagogy::verify_math_expr` (`teaching.rs:284-294`): una expr multibyte de 200 chars = 400 bytes pasaba el pre-gate para caer en el gate real (repro capturado con el contador de chars restaurado temporalmente). El lado pedagogy NO se tocó (otro dueño); las otras diferencias (`;`/`\n` y parser real `prepare_function_ast`) son por diseño (forma vs CAS-gate).
- **FIX 6 (ALTA)** — `architecture.md` §8/§13 sincronizados con las dos olas de `command_registry.rs`: **733 comandos** (650 → 662 con fantasma+nuevos +12 → 733 con fantasma-2 +71: Matrices +20, AM2 +13, Crear +7, AM1 +7, CAS +7, 3D +7, Construir +3, Dinámica +3, Estadística +2, Análisis +1, Complejos +1; `Image` NO registrado — stub sin modelo persistente — y blindado en `command_registry.rs:11053-11066`), **696 visibles + 15 UI = 711 en paleta** (625 → 696), 25 categorías sin cambio (`command_registry.rs:11420-11474`, blindaje `registry_counts_match_documented_architecture` 1/1). Documentado como **cambio de semántica**: `sylvester_resultant` a convención textbook — `Resultant[x²+1, x-1, x]` ahora `2` (antes `-2`; `solve.rs:1005-1024` + corrección `:1050-1054`). También tools assistant 28 / MCP 42 / `MAX_INTERVAL_PREC_DIGITS` 15 / `MAX_COMPILED_EXPR_CACHE` 128 ya estaban; drift corregido: i18n "322 claves" → 329 (`i18n.rs:102`), `OPEN_PROBLEMS_LAB.md` "35 tools" → 42 (`grafito-mcp/src/protocol.rs:448`); "5/8/18" verificado correcto (PRIMARY 5 / SECONDARY 8 / UNIVERSITY 18 vs ALL_GROUPS 15 clásico intencional — `toolbar.rs:301-302,2134`, la explicación de §8 sigue siendo la correcta); `grafito-core/src/lib.rs:6` ya decía "58 tipos" y `GeoObject` tiene 58 variantes (contadas, no repetido).
- **Tests agregados (todos rojo-hoy antes del fix, salvo los golden)**: core `tests_polygon_lazos_autointersectados` (6); anim `srt_frena_al_cruzar_el_tope_sin_materializar_la_salida_entera`, `ass_frena_al_cruzar_el_tope_con_karaoke_maximo`, `empuja_acotado_nunca_supera_el_tope`, `short_script_usa_las_13_canonicas`, `short_script_varia_los_beats_por_familia`, `short_script_golden_hash`, `math_expr_se_mide_en_bytes_como_el_gate_de_pedagogia`, `peor_caso_compuesto_err_honesto_en_vez_de_cien_mb`, `handshake_tolerante_a_v2_con_traza_de_mensajes_inesperados`, `handshake_rechaza_version_fuera_de_rango_con_error_tipado`.
- **Gates** (2026-09-24): `cargo fmt -p grafito-core -p grafito-anim -- --check` 0; `cargo clippy -p grafito-core -p grafito-anim --all-targets -- -D warnings` 0; `cargo test -p grafito-core` 383 lib + 58 + 9 + 7 + 3 + 14 + 2 + 5 + 2 + 5 / 0 fallos; `cargo test -p grafito-anim` 175 / 0; `cargo test -p grafito-command --lib registry_counts_match_documented_architecture` 1/1. **NO corre hoy por rotura ajena**: los gates `--locked` (`Cargo.lock` desincronizado por `grafito-whiteboard/Cargo.toml` de otro agente) y `cargo test --workspace` (`grafito-ui` lib test con 11 errores de otro agente: `draw_turn_player` sin arg `locale`, `grafito-ui/src/assistant.rs:17980`).
- **Conteos antes/después**: comandos 662 → 733; visibles 625 → 696; paleta (+15 UI) 640 → 711; categorías 25 → 25; plantillas en `short_script` 5+universal → 13 (6 familias de copy); protocolo anim `== 1` → `1..=2`; `math_expr` chars → bytes; cota captions post-hoc → preventiva; presupuesto player inerte (frames×8 KiB) → 64 MiB reales; i18n (doc) 322 → 329; Lab MCP tools (doc) 35 → 42.
