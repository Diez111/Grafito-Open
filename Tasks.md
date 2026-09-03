# Tasks.md — Auditoria E2E Completa v2 (J-Space) — ESTADO 2026-08-20

## F0 — Inventario & Baseline [DONE]
- [x] F0.1 Mapa DAG y conteo (136 RS, 39335L app, 19762L core) — ver Plans.md
- [x] F0.2 Baseline gates: fmt OK, clippy OK (0 warnings), test OK (387 geo + 5 anim + resto workspace OK)
- [x] F0.3 docs/architecture.md creado + progress.md con baseline

## F1 — Cerebro: Nucleo Logico (PRIO 1, sin tocar UI) — DONE parcial critico
- [x] F1.1 grafito-core: hash determinista object.rs:2605 (sorted vars), Transformed try_new valida ast "z" y rechaza singular "0", BTreeMap deuda documentada
- [x] F1.2 grafito-geometry: cache LRU 128 (expr.rs), midpoint lo+(hi-lo)/2, safe_sample MAX 100k + finite check, Matrix zeros/identity debug_assert, newtypes Radius/Angle deuda P2
- [x] F1.3 grafito-command: auditado (Statem preflight/documentado, allowlist 92 caps, H1-H5 hallazgos)
- [x] F1.4 grafito-complex/render/whiteboard/profile/plugins/assistant: auditados, rangos OK, headless SKIP

## F2 — Generador de Animaciones v2 (PRIO 0) — DONE
- [x] F2.1 Protocol (protocol.rs): newtypes Resolution (64..4096), AnimDuration (0.1..30s), AnimParams validate + into_request, ExportFormat, WireMessage versionado, ProtocolError completo, canvas 64..8192 (warn >4096)
- [x] F2.2 Engine (engine.rs): spawn NUL/workdir/bin validation, wait_ready hello+pong, submit can_submit, recv_event filtra job_id, line_cap OOM drain, shutdown cooperativo, cancel() Running->Cancelled, Drop no-bloqueante (kill+try_wait), diagnostics cap 64, validate_media_path, run_job cancel poll 200ms deadline absoluta
- [x] F2.3 Python (manim_engine/__main__.py): sandbox AST DENIED_NODES + __ bloqueado, safe_path relative_to symlink-safe, placeholder hex estable (bytes.fromhex), 4 templates (derivative-slope, integral-area, taylor-series, conformal-map), parse_canvas, progress 30->60->100, fallback stderr log
- [x] F2.4 Nativo (anim_native.rs): 5 templates (derivative, pitagoras, integral, taylor, conformal) + dispatcher render_anim_by_template + tests (5)
- [x] F2.5 Statem AnimJobState completo + tests <2s placeholder con/sin manim, timeout, cancel, path_escape (engine::tests 5 OK)

## F3 — Piel: Separacion Cerebro/Piel — DONE parcial
- [x] F3.1 grafito-ui: tokens OK, assistant panel composer fix verificado (sin ScrollArea envolvente, clamp 88..260)
- [x] F3.2 grafito-app anim_ui.rs: id unico textura por frame, max_height 84, bar_width 420, wrap, sin I/O, allow(dead_code) via lib, tests 3
- [x] F3.3 grafito-app assistant.rs: auditado AssistantRuntime, I/O en thread, UI solo render &State (deuda Statem formal P1)
- [x] F3.4 grafito-app app.rs/panels.rs/canvas.rs/render_2d.rs: auditado (God Object deuda P1, MAX_UNDO 50 deuda P2)

## F4 — Estabilidad Total (Puertas AGENTS.md) — DONE
- [x] F4.1 cargo fmt --check (0)
- [x] F4.2 cargo clippy --workspace --all-targets -- -D warnings (0, 1.92 + stable)
- [x] F4.3 cargo check --workspace --locked (OK)
- [x] F4.4 cargo test --workspace --locked (OK, 392+ tests, 0 FAILED tras revert safe_sinh)
- [x] F4.5 cargo check --workspace --examples --benches --locked (OK)
- [x] F4.6 cargo doc --workspace --no-deps --locked (OK, RUSTDOCFLAGS -D warnings)
- [x] F4.7 gpu-compute headless (SKIP WGPU_BACKEND=gl, no bloquea)
- [x] F4.8 packaging build-deb.sh + fixtures (PASS)

## F5 — Docs & Handoff — DONE
- [x] F5.1 docs/architecture.md + README + CHANGELOG (architecture.md creado)
- [x] F5.2 progress.md evidencia CI + riesgos (RUSTSEC 2026-12-31, HashMap, etc.)
- [x] F5.3 Handoff: menu decisiones vibecoder-guide si algo fallo (este archivo)
