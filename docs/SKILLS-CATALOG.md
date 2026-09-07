# SKILLS CATALOG — referencia externa (lo local ya está instalado)

> 528 skills YA en `.opencode/skills/` — inventario en `docs/SKILLS-INSTALLED.md`, router en skill `skills-catalog`.
> Este archivo es solo para lo que NO es un SKILL.md instalable: backends de memoria, plugins/orquestadores, MCP servers, crates/Rust, registries + tabla de routing.

## 0. Protocolo IA

1. Lee `docs/SKILLS-INSTALLED.md` (top picks + 528 nombres) y carga 1-3 con el tool skill.
2. Si falta algo, instala: `bash scripts/install-skills.sh --group <rust|ui|memory|orchestration|mcp>` o `npx skills add <owner/repo> --skill <nombre> -a opencode`.
3. Solo usa las URLs de abajo para lo NO instalable (backends, plugins npm, MCP).

## 1. Top picks Grafito (todos locales salvo nota)

- UI escandinava egui: `design-tokens` (filosofía Scandinavian) + `ui-visual-composition` + `minimalist-ui`/`high-end-visual-design` + gate `web-design-guidelines` + `wcag-audit` + `ux-writing` + `macos`/`web` platform.
- Rust: `rust-best-practices` (apollo, mejor review) + `rust-skills` (265 reglas) + `rust-engineer`/`rust-systems` + `rust-architect` (ADRs) + `webgpu-skill` (cazala) + `axum-skills` (solo si servidor).
- NO instalados aún (bajo demanda): `actionbook/rust-skills` (`/sync-crate-skills` desde Cargo.toml), `huiali/rust-skills`, `full-stack-skills/{rust,tauri}-skills`, `Zuytan/rustrade ui-design` (copiar `.agent/skills/ui-design`), `optimizing-rust-for-wasm` (otherland), `rofrol/awesome-wgpu` + `learn-wgpu` (índices, no skills).

## 2. Memoria: backends (no son SKILL.md, se instalan como plugin/MCP)

- Default local: `tickernelz/opencode-mem` (`plugin:["opencode-mem"]`, SQLite+FTS5+USearch, WebUI :4747). https://github.com/tickernelz/opencode-mem
- Anti-amnesia: `daniloaguiarbr/opencode-auto-memory` (dual-write Serena + MEMORY.md). https://github.com/daniloaguiarbr/opencode-auto-memory
- Puente Claude: `thedotmack/claude-mem` (`npx claude-mem install --ide opencode`). https://github.com/thedotmack/claude-mem
- Grafo local: `bobmatnyc/kuzu-memory` (`uvx kuzu-memory`, <3ms) / `topoteretes/cognee` (`uvx cognee-mcp`, `.cognee_system`).
- Código: `oraios/serena` (`uvx ... serena start-mcp-server`, `.serena/memories/*.md` + LSP) / `DeusData/codebase-memory-mcp` (14 tools grafo código) / `corporatepiyush/mcp-memory` (Rust 1 binario, tree-sitter).
- Híbrido team (solo no-PII): `memory.store` (`https://memory.store/mcp`, sucesor Julep) / Mem0 (`https://mcp.mem0.ai/mcp`, o `tensakulabs/mem0-mcp` self-host Qdrant+Neo4j+Ollama) / `supermemory` (`npx supermemory local`).
- Compresión: `alexgreensh/token-optimizer` (local) + `Opencode-DCP/opencode-dynamic-context-pruning` + `avabbbb/headroom-opencode` (proxy :8787).

## 3. Orquestación: plugins (elegir 1)

- Crítico multi-sesión: `code-yeongyu/oh-my-openagent` (Sisyphus+Prometheus+Metis, `/start-work`). https://github.com/code-yeongyu/oh-my-openagent
- 2-3 slices: `hueyexe/opencode-ensemble` (worktrees, dashboard :4747). Ligero: `moinulmoin/opencode-arise`.
- Research largo: `kdcokenny/opencode-background-agents` (`delegate` persistente) / `AutomatorAlex/opencode-background-tasks` (`bg_task` + reconcile) / `spoons-and-mirrors/pocket-universe`.
- Gobernado: `DVNghiem/FlowDeck` (`~/.fd-plan/`). Calidad: `tdd-workflow` + `reviewer`/`debugger` + `git-release` + `github MCP`.

## 4. MCP: activos vs bajo demanda + registries

- Activos (7, opencode.json): `filesystem` (scope `.`), `git`, `fetch`, `memory`, `sequential-thinking`, `time`, `context7` remote.
- Bajo demanda: `github/github-mcp-server` (Go oficial, ojo tokens: limitar toolsets) + `rust-analyzer-mcp` + `figma` (`mcp.figma.com/mcp`) + `storybook addon-mcp` + `microsoft/playwright-mcp` + `bytebase/dbhub` (1.4k tokens, NO `server-postgres` archivado con bypass) + `exa-labs/exa-mcp-server` (solo en subagente, devolver destilado).
- Descubrir con criterio: `registry.modelcontextprotocol.io` (oficial) → `glama.ai/mcp` (grades A-F, 66% servers con issues críticos) → `smithery.ai` (verified + install). Exigir: mantenimiento <6 meses, permisos mínimos, tools pocas.

## 5. Routing por proyecto (la IA decide)

- ¿1 fichero exploratorio? → sin orquestador, `fetch`/`context7` si necesita docs.
- ¿Feature/bugfix/refactor? → `tdd-workflow` + `reviewer`/`debugger`.
- ¿UI escandinava egui? → §1 UI (tokens + gate + a11y + writing).
- ¿Multi-fichero independiente? → `Ensemble` o `ARISE` + worktrees.
- ¿Research largo en paralelo? → `background-agents` o `pocket-universe`.
- ¿Crítico multi-sesión? → OMO `Prometheus+Metis → /start-work`.
- ¿Release/E2E/DB? → `git-release`+github / `playwright`+`webapp-testing` / `dbhub`+`database-optimizer`.
- ¿Memoria? → `opencode-mem` siempre; + `cognee` si docs crecen; + `kuzu` si grafo; + SaaS solo no-PII.
