# progress.md — Auditoria E2E Completa 2026-08-20

## GOAL
Auditoría TOTAL de inicio a fin + mejoras + nuevas funciones, con foco en generador de animaciones, pensando en cada detalle, trabajo por capas (Cerebro → Piel), statem, type-safety, separación UI.

## Slash Commands activos
- /j-space: Plans.md v2 + Tasks.md v2 + docs/architecture.md antes de código
- /statem: AnimJobState, DocumentLifecycle, AssistantRuntime, Interval/ExprEval
- /rust-design: newtype Resolution/AnimDuration/AnimJobId, Result/Option, clippy -D warnings =0
- /rust-ui: UI solo renderiza &Estado, sin I/O
- /vibecoder-guide: micro-pasos, lenguaje criollo, menú ante error

## Baseline gates (2026-08-20, toolchain 1.88)

| Puerta | Comando | Resultado |
|--------|---------|-----------|
| fmt | cargo +1.88 fmt --check | PASS (0) |
| clippy | cargo +1.88 clippy --workspace --all-targets -- -D warnings | PASS (0, 17s) |
| check | cargo +1.88 check --workspace --locked | PASS |
| examples | cargo +1.88 check --workspace --examples --locked | PASS |
| benches | cargo +1.88 check --workspace --benches --locked | PASS |
| test | cargo +1.88 test --workspace --locked | PASS (387 geo lib + 5 anim + doc-tests 7+1, 0 FAILED tras revert safe_sinh) |
| doc | RUSTDOCFLAGS="-D warnings" cargo +1.88 doc --workspace --no-deps --locked | PASS |
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
- **docs/architecture.md**: creado (DAG, statems, type-safety, presupuestos, CI 8 jobs, riesgos)
- **Plans.md v2 / Tasks.md v2**: trabajo por capas documentado

## Verificación de errores de compilador (vibecoder-guide)

**Error 1**: clippy manual_range_contains en AnimDuration y interval → **Menú**: A) usar (a..=b).contains(&x) B) allow(lint) C) revertir. **Elegido A**: fix a contains, clippy pasa.

**Error 2**: clippy needless_if en anim_ui Exportar GIF vacío → **Menú**: A) quitar if B) añadir TODO comment C) allow. **Elegido A**: añadir TODO logico.

**Error 3**: tests FAILED sinh(1e10).is_infinite() tras clamp → **Menú**: A) revert clamp (inf es esperado, safe_sample maneja) B) actualizar tests a expect finito C) clamp condicional. **Elegido A**: revert a a.sinh(), tests 387 PASS. Lección negocio: "safe" no significa esconder inf, sino detectarlo aguas abajo.

**Error 4**: duplicated_attributes allow(dead_code) → **Menú**: A) quitar duplicado en file B) quitar en lib C) allow duplicado. **Elegido A**: quitar #!allow en file, dejar en mod.

**Error 5**: anim_native delimiter extra → **Menú**: A) git checkout + reescribir limpio B) sed fix C) allow. **Elegido A**: checkout + cat heredoc limpio, check pasa.

## Riesgos residuales (OPEN)

- RUSTSEC-2026-0194/0195 quick-xml/zbus expiran 2026-09-30 (deny.toml)
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
