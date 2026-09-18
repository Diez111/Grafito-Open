# MEMORY.md — promoción curada (híbrida local-first)

> Sesión caliente en `/tmp` + promoción curada aquí + perfil global en `~/.opencode-mem/`.
> PII siempre local. Solo hechos no-PII a SaaS team (`memory.store`/Mem0 cloud).
> Nunca append-only sin dedup/decay. Esta file la escribe la IA en `session.idle`.
> Historial largo: `docs/ledger-archive-2026-09.md`.

## Proyecto (cross-sesión)
- Stack: Rust 1.92, 18 members de workspace (19 dirs en crates/), eframe 0.29 + egui rayon, wgpu 22.
- Tokens nórdicos: Snow #F7F7F5 / Charcoal #2C2F38 / Fjord #4A6E8A, Inter, grid 8px.
- Gates: `cargo fmt --check && cargo clippy -- -D warnings && cargo test --locked`.

## Decisiones (fecha + por qué)
- 2026-09-18 (Olas 0–2, plan de mejoras): Groebner real por nombre (`cas_groebner_ordered`, Buchberger con grlex/lex/grevlex), smoke de ejecución de los 643 specs (9 no implementados pinneados + 8 subset honestos), i18n 323 claves ES/EN/PT/IT/FR/DE, hoja 8×14, protocolo de construcción navegable, `ZTest2`/`FTest` reales y cotas Gröbner 12/6/384; scripting `OnLoad`/`OnUpdate` automático (allowlist benigna pinneada), análisis simbólico por CAS, factor general (Kronecker, grado ≤6), álgebra lineal exacta (`exact_linalg.rs`), cuádricas volumen/área, `SetSpinSpeed` + herramienta Texto (88.ª). Button quedó real (Tool::Button → botón de acción con guion, test ola16); Tool::Image sigue fuera por diseño honesto (requiere modelo persistente de textura cross GPU/CPU). Release instalado /usr/bin/grafito sha256 00708be8 (reemplaza 21e35899; copia ~/.local/bin borrada). El working tree sigue sin commitear. Evidencia: workspace 5151/0 (152 suites), fmt/clippy 0.
- 2026-09-17: el gate del asistente dejó de descartar respuestas por contexto rancio (el texto se publica siempre; propuestas re-verificadas contra el doc actual; Apply sigue fail-closed por `PlanBasis`). Clarificación `ask_user` usa el `call_id` real del wire (`AgentEvent::Clarification`). Telemetría de tokens opt-in: `GRAFITO_USAGE_LOG=1` → JSONL en `$XDG_DATA_HOME/grafito/assistant_usage.jsonl`. Contexto del prompt truncado por presupuesto (`bounded_context_prompt`) para que un documento denso no rompa la consulta.
- 2026-09-17 (perf Plan v3, Olas 0–5): loop plano 35.3 µs vs 122.7 (trivial 5000); mimalloc **revertido** (perdió 13/13 plantillas); `desired_maximum_frame_latency: 1`; PGO `scripts/pgo.sh`; ADR-0004 egui 0.32 diferido (312 rupturas). Evidencia: suite 5071/0, GPU 30/30.
- 2026-09-17 (post-perf): 2 crashes de escena 2000 con fix + test (vertex budget 6M, `GracefulTextureCache`, `cache_nonce`); idle RSS 80 MB, escena 2000 740 MB; release sin `glow` (44.8 MB). Evidencia: suite 5090/0, sha256 ef0b863f, E2E 2000.
- 2026-09-05: 528 skills locales en `.opencode/skills/` (67MB, gitignored). Router `skills-catalog` primero; la IA elige 1-3 por tarea. Equipo reinstala con `scripts/install-skills.sh`.
- 2026-09-05: Memoria híbrida local-first (PII local; SaaS solo no-PII team). Default `opencode-mem` + ledger `.jspace/WORKSPACE.md`; grafo `kuzu`/`cognee` bajo demanda.
- 2026-09-05: MCP mínimos en opencode.json (git, fetch, memory, sequential-thinking, context7). `filesystem` MCP desactivado 2026-09-17 (redundante con read/write/glob/grep; ahorra ~14 tools por request). Full (GitHub/Figman/Playwright/DBHub) por agente y bajo demanda.
- 2026-09-05: UI = egui 0.29 + tokens nórdicos (Slint solo spike si menús nativos/tray lo exigen).
- 2026-09-08: opencode-mem auto-capture APAGADO (causaba cartel de captura en fondo). Memoria a pedido vía tool `memory`. Requiere reiniciar opencode-desktop.
- 2026-09-09: remote opencode verificado (`server` 0.0.0.0:4096 + mDNS; con `OPENCODE_SERVER_PASSWORD` exige Basic auth). Script `scripts/opencode-remote.sh`. Falta `sudo ufw allow 4096/tcp`.

## Preferencias usuario
- Todo el catálogo disponible, la IA elige 1-3 skills por tarea.
- Memoria híbrida (local-first + cloud team solo no-PII).
- MCP mínimo lectura; full bajo demanda.
- Commits: español rioplatense, sin emojis, autor siempre Lautaro Agustin Diez.

## Backends
- Default local: `opencode-mem` plugin (config `~/.config/opencode/opencode-mem.jsonc`) + `.jspace/WORKSPACE.md`.
- Grafo: `kuzu-memory` vía `uvx` (CLI). Serena `.serena/memories/*.md` bajo demanda.
- Runtimes del box: cargo/rustc 1.92, bun 1.4.2 (`~/.bun/bin`), uv 0.12.10 (`~/.local/bin`). Node ausente (bun cubre MCP).
