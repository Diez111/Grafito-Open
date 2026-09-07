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

## F6 — Sync BUILD docs↔código 2026-09-04 — DONE
- [x] F6.1 Presupuestos con file:line (8192/2048/8/60s/512KiB-1MiB, assistant-types/src/lib.rs:198-255) + Responses Spark (assistant/src/lib.rs:50-56,905-951) + modelos (default deepseek utils.rs:59-61, fallback assistant.rs:2470-2613, qwen/kimi sólo hint :2936)
- [x] F6.2 Comandos 238 (`command!(`, registry:228), 199 paleta + 14 UI = 213, 26 categorías visibles; grupos toolbar 17 (5/8/17, toolbar.rs:263-284); 73 tools; 10 perspectivas; 16 crates; CI 17 jobs (coverage 75%, bench >10%, mutation semanal)
- [x] F6.3 app.rs: Ctrl+T tema (:4259-4267) + Ctrl+P/E (:4268-4278) + F8/F9 (:4135-4144) — cero fantasmas; onboarding gating `!onboarding_completed` (:1763, :4922-5033, utils.rs:46-48), botón [No mostrar]
- [x] F6.4 Responsive: rail 60px Medium/Wide (ui.rs:549-552), drawer clamp 292..440 (tokens.rs:207-210, panels.rs:2125-2132), panel izq 180+45% (panels.rs:1201-1206); paleta footer "N de M" es (command_palette.rs:394-403)
- [x] F6.5 README.md/README.en.md Controles corregidos (Shift+L/K/J = X/Y/ambos, F8/F9, Ctrl+T/P/E); tabla doc→código §13 en architecture.md

## F9 — 20 agentes 2026-09-05 — DONE (rama f9-completion)
- [x] F9.1 W1 perf + W2 correctness + W3 paridad + W4 interop/i18n/calidad (20 agentes, ownership disjunto)
- [x] F9.2 Cierres lead: Perpendicular/input+dispatcher, Rastro (registry+dispatch+docs), Parallel/Arc/Sector + polygon_n, trace stack Document+render+tests
- [x] F9.3 Docs sync real: 232 comandos únicos, 193 visibles + 14 UI = 207 paleta, 76 Tools, 18 crates
- [x] F9.4 Gates: fmt/clippy -D warnings 0, suites lib+tests verdes (skip 1 Vulkan lavapipe pre-existente)

## F10 — Plan total 2026-09-05 — EN CURSO (rama f10-plan-total, pusheada a origin)
- [x] F10.0 Merge f9-completion→main (blindaje) + rama f10 + deps qrcode/printpdf (MSRV ok)
- [x] F10.W1: GGB-WIRE + CAS-WIRE (Steps) + I18N-WIRE (locale vivo) + ANIM-REVIVE + PRISM-RENDER
- [x] F10.W2 parcial: CAS-rational (Hermite/Fu) + dispatcher arms + ROBUST-SPADE + recovery modal
- [ ] F10.W2 resto: DECIDER + LINECONIC (abortados/perdidos por reset, rehacer abajo)
- [ ] F10.W3: STATEM-AULA + AULA-UI-QR + TELEMETRY + JUDGE + MASTERY
- [ ] F10.W4: LAYERS + TABLE-REG + ACTIONS + PDF-CSV + TOOLTIPS-SLIDERS
- [ ] F10.W5: motor resto (Gruntz/Risch/EDO) + 3D resto (Net/volumen/ortho/F(x,y,z)) + P2P iroh + CRDT
- [ ] F10.W6: CI honesto + perf (rayon/SIMD/R-tree) + egui upgrade + E3 keyring + i18n resto

### LO QUE FALTA (verificado claims-vs-código 2026-09-05)
**Perdido por resets, rehacer:**
- [ ] DECIDER: Asymptotic Decider casos 5/10 marching squares + enum BreakKind {Pole,Jump,RemovableHole,DomainEdge} (implicit_curve.rs + function_sampling.rs)
- [ ] LINECONIC: geom_eps(scale) real + intersección recta-cónica exacta + cónica-cónica stub honesto (intersections.rs + lines.rs)
- [ ] A11Y resto: keyboard/orden foco en toolbar/paneles, live-region lector, Esc descarta persistentes (assistant.rs/ui.rs/pou.rs fuera de tokens/)
- [ ] E3/G18: keyring/reqwest opcionales tras assistant-net (revertido; requiere gates cfg en credentials + request_remote*)

**Nunca empezado (fases F2-F6 del plan):**
- [ ] CAS: Gruntz límites simbólicos (L), Risch completo (L, solo Risch-Norman parcial), EDO simbólicas 1er orden (M), Laurent/residuos (M), Groebner real (L, hoy stub 2×2)
- [ ] 3D: Net real (M), intersección plano-poliedro (M), volumen/área 3D (S), vistas ortográficas (S), F(x,y,z)=0 marching cubes (L), picking exacto resto, icosa/dodecaedro, revolución lathe
- [ ] Aula/P2P: statem Idle→Lobby→Live→Closed (M), ClassroomTransport + Loopback (M), unirse-con-código + roster + manos (M), QR real qrcode crate (S), iroh P2P (L), CRDT pizarra UUID+LWW (M), dashboard←StudentProfile real (M)
- [ ] IA: telemetría por turno (S), juez LLM telling calibrado (M), contrato revise>block + over-blocking ≤5% (M), FSM durable serde (M), mastery por BKT posterior (S), scheduler FSRS-lite + interleaving (M), calibración EM N≥200 (M), Elo on-the-fly (M), costos visibles + cascada (S-M), sesiones cifradas (M-L), offline-first cola (L), errores vibecoder 2-3 botones (M), OCR local (L)
- [ ] Paridad: capas (S), tabla viva + regresión ~ (M), action objects Button/Checkbox/InputBox + GGBScript (M), PDF real printpdf (M), Bar/Pie (S), probability panel (M), spreadsheet view (M), MathML export (M), custom tools .ggt (M), streamlines RK4 (L), TikZ axis pgfplots (S), usvg import (M), CSV RFC4180 (S), clipboard SVG/PNG (S-M), HTML export (M)
- [ ] Profesionalismo: sliders Play+prompt auto (M), trace hover readout (M), tooltips ricos + MRU paleta (S), skeleton+% progreso (S), keypad matemático (M), preview LaTeX vivo (M), onboarding tour (S), sonificación/Braille (L), projectorMode (M)
- [ ] Perf medido: rayon build_geometry (M), caché world-space (L), R-tree culling (M), alloc reuse (S-M), bind groups (M), SIMD batch eval (L), wgpu-profiler (S), egui 0.29→0.31+ (M-L), splash progreso real (S), cold start instrumentado (S)
- [ ] CI honesto: bench-regression real con critcmp (M, quitar || true), mutation gate 80% (M), fuzz anim_wire contra producción (M), fuzz smoke en PR (M), doc-tests ejecutables (S-M), GPU fail-closed local (S), coverage sin fallback ruidoso (S)
- [ ] i18n resto: onboarding+cheat+toasts+menús (~280 claves, M), format_number cableado (M), resto 7335 strings por oleadas, fluent diferido (>500 claves)
- [ ] Varios: whiteboard validación por hoja en validate_document (S), examen lockdown policy (decisión), pick 3D exacto resto, journal/ledger Plans.md F9 (S), CHANGELOG 1.2.35 (M), WORKSPACE Next (S), dropear 3 stashes (tras backup branch), .jspace sync
