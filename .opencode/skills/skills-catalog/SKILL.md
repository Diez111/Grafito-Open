---
name: skills-catalog
description: Router maestro de 528 skills locales. Úsalo primero para elegir qué skill cargar según proyecto.
---

# Skills Catalog Router

> 528 skills YA instaladas en `.opencode/skills/` (46 repos, 67MB, todas validadas).
> Ver inventario: `docs/SKILLS-INSTALLED.md`. Referencia externa: `docs/SKILLS-CATALOG.md`.
> La IA elige 1-3 por tarea y las carga con el tool skill. Nada de instalar: ya están en disco.

## Protocolo IA (obligatorio)

1. Lee `docs/SKILLS-INSTALLED.md` (top picks + lista completa 528).
2. Elige 1-3 skills según tipo de tarea (ver top picks).
3. Carga con el tool skill por nombre exacto (nombre = dirname).
4. Si ninguna encaja, amplía con `bash scripts/install-skills.sh --group <rust|ui|memory|orchestration|mcp>` o `npx skills add <owner/repo> --skill <nombre> -a opencode`.
```
4. Luego `skill({name:"<nombre>"})` para cargar la instalada.
5. Si ninguna encaja, usa mega-repos como bolsa:
- `salmanneomtech/Opencode-Skills` 634+
- `ffsshhttiikk/opencode-agents-skills` cientos atómicos
- `VoltAgent/awesome-agent-skills` 1000+ curado
- `FrancoStino/opencode-skills-collection` 1595+ bundle + SkillPointer
- Loader persistente: `joshuadavidthomas/opencode-agent-skills` (`use_skill`, sobrevive a compaction).

## Skills locales siempre disponibles (7 recuperadas)

- `j-space` — ledger Goal/Core/Verified/Open/Next, gating fast/full/loop.
- `rust-performance` — puffin, rayon, wgpu 22, lyon, spade, rstar.
- `rust-ui-ux` — tokens TYPE/SPACE/RADIUS, Scandinavian 5/8/17 groups.
- `token-optimizer` — LLMLingua-2, compaction, prompt-cache, routing.
- `context-engineering` — precedence 8 niveles, discovery, prune.
- `grafito-architecture` — 16 crates, presupuestos,Debt BTreeMap/Statem.
- `profiling` — puffin/criterion/wgpu-profiler, mide antes de optimizar.

## Regla de elección rápida

- ¿UI escandinava egui? → `design-tokens(Scandinavian)` + `ui-visual-composition` + `minimalist-ui` + gate `web-design-guidelines` + `wcag-audit` + `ux-writing`.
- ¿Rust workspace? → `rust-best-practices` + `rust-skills` + `rust-engineer`.
- ¿Memoria? → `opencode-mem` siempre; + `cognee` si docs crecen; + `kuzu` si grafo; + `memory.store` SaaS solo no-PII.
- ¿Multi-fichero? → `Ensemble` o `ARISE`. ¿Crítico multi-sesión? → `OMO Prometheus+Metis`.
- ¿E2E/DB/Release? → `playwright` / `postgres-mcp` / `git-release` + `github MCP`.
