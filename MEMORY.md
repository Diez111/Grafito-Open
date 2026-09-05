# MEMORY.md — promoción curada (híbrida local-first)

> Sesión caliente en `/tmp` + promoción curada aquí + perfil global en `~/.opencode-mem/`.
> PII siempre local. Solo hechos no-PII a SaaS team (`memory.store`/Mem0 cloud).
> Nunca append-only sin dedup/decay. Esta file la escribe la IA en `session.idle`.

## Proyecto (cross-sesión)
- Stack: Rust 1.92, 16 crates, eframe 0.29 + egui rayon, wgpu 22.
- Tokens nórdicos: Snow #F7F7F5 / Charcoal #2C2F38 / Fjord #4A6E8A, Inter, grid 8px.
- Gates: `cargo fmt --check && cargo clippy -- -D warnings && cargo test --locked`.

## Decisiones (fecha + por qué)
- 2026-09-05: 528 skills locales en `.opencode/skills/` (46 repos depth-1, 67MB, 528/528 frontmatter OK). Router `skills-catalog` primero; la IA elige 1-3 por tarea. `.opencode/` gitignored por diseño; equipo reinstala con `scripts/install-skills.sh`.
- 2026-09-05: Memoria híbrida local-first (PII local; SaaS solo no-PII team). Default `opencode-mem` + ledger `.jspace/WORKSPACE.md`; grafo `kuzu`/`cognee` bajo demanda.
- 2026-09-05: MCP mínimos 7 en opencode.json (filesystem scope `.`, git, fetch, memory, sequential-thinking, time, context7). Full (GitHub/Figman/Playwright/DBHub) por agente y bajo demanda.
- 2026-09-05: UI = consolidar egui 0.29 + tokens nórdicos (Snow/Charcoal/Fjord, Inter, 8px, AA). Slint solo spike si menús nativos/tray lo exigen.
- 2026-09-05: Este box YA tiene toolchain: cargo/rustc 1.92 (MSRV exacta), bun 1.4.2, uv 0.12.10. Node sigue ausente (bun lo cubre para MCP). Gates Rust corren aquí.

## Preferencias usuario
- Todo el catálogo disponible, la IA elige 1-3 skills por tarea.
- Memoria híbrida (local-first + cloud team solo no-PII).
- MCP mínimo lectura; full bajo demanda.

## Backends
- Default local: `opencode-mem` plugin INSTALADO 2026-09-05 (config `~/.config/opencode/opencode-mem.jsonc`, verificado E2E `opencode run` HARNESS-OK) + `.jspace/WORKSPACE.md` ledger.
- Grafo: `kuzu-memory` disponible vía `uvx kuzu-memory` (45 pkgs, CLI `remember/enhance/learn`; sin integración opencode nativa — solo CLI por ahora).
- Serena `.serena/memories/*.md` para memoria código + LSP (bajo demanda).
- Runtimes del box: bun 1.4.2 (`~/.bun/bin`), uv 0.12.10 (`~/.local/bin`). Sin cargo/rustc/node todavía.
