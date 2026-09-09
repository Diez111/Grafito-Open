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
- 2026-09-08: opencode-mem NO fue descargado por la IA (venía del 2026-09-05, 614M de los cuales 613M es el modelo local nomic). Hallazgo: auto-capture APAGADO de fábrica (exige opencodeProvider, 183 prompts logueados pero 0 memorias auto y 0 perfil). Activado 2026-09-08 con provider opencode-go + Spark 1.3 (usa tu auth, $0 extra) + embeddingUseTaskPrefixes. Verificado: store→recall cross-sesión ×2 ("Grafito-Open", preferencia commits). Web UI en :4747.
- 2026-09-09: auto-capture de opencode-mem APAGADO DEL TODO (autoCaptureEnabled false + profile interval a 1M). Causa real del cartel "Respuesta lista / opencode-mem capture": no era toast del plugin sino la APP avisando cada sesión de captura en fondo. Sin sesiones de fondo no hay más carteles. Memoria sigue a pedido vía tool `memory`. Requiere reiniciar opencode-desktop.
- 2026-09-07: opencode remoto verificado: `server` en opencode.json (0.0.0.0:4096 + mDNS opencode.local), `/` sirve web UI (PWA mobile-ready), sin password corre abierto con warning / con OPENCODE_SERVER_PASSWORD exige Basic auth. Script `scripts/opencode-remote.sh` (genera clave si no hay). Falta solo: `sudo ufw allow 4096/tcp` (ufw activo; sudo pide clave, no automatizable desde aquí).
- 2026-09-08: opencode remoto EN VIVO: `opencode serve :4096` corriendo, tailnet IP 100.80.55.7 (pc.tail790faa.ts.net), celu redmi-14c en la tailnet. Health por tailnet con Basic auth OK; sin auth 401. Clave en /tmp/opencode/remote-pass.txt (owner-only). `tailscale serve` (https) pendiente de aprobación admin. Al reiniciar la PC: re-correr el serve.

## Preferencias usuario
- Todo el catálogo disponible, la IA elige 1-3 skills por tarea.
- Memoria híbrida (local-first + cloud team solo no-PII).
- MCP mínimo lectura; full bajo demanda.

## Backends
- Default local: `opencode-mem` plugin INSTALADO 2026-09-05 (config `~/.config/opencode/opencode-mem.jsonc`, verificado E2E `opencode run` HARNESS-OK) + `.jspace/WORKSPACE.md` ledger.
- Grafo: `kuzu-memory` disponible vía `uvx kuzu-memory` (45 pkgs, CLI `remember/enhance/learn`; sin integración opencode nativa — solo CLI por ahora).
- Serena `.serena/memories/*.md` para memoria código + LSP (bajo demanda).
- Runtimes del box: bun 1.4.2 (`~/.bun/bin`), uv 0.12.10 (`~/.local/bin`). Sin cargo/rustc/node todavía.
