# AGENTS.md — Grafito

> Cargado vía `instructions` en `Grafito-Open/opencode.json` + global.

## Arquitectura (ver `docs/architecture.md`)

```
grafito-geometry  ──┐
grafito-complex   ──┼─> grafito-core ──> grafito-command ──> grafito-app
grafito-render    ──┘          │                    ▲
grafito-anim      ─────────────┼─> grafito-assistant┘
grafito-whiteboard ────────────┘
grafito-ui        ─────────────> grafito-app (Piel)
```

- **Cerebro puro** (sin egui/wgpu): `grafito-core`, `grafito-geometry`, `grafito-command`, `grafito-whiteboard`, `grafito-profile`, `grafito-pedagogy`, `grafito-plugins`
- **Puente**: `grafito-anim` IPC JSON v1 stdio a Python/manim
- **Piel**: `grafito-ui` (tokens, theme) + `grafito-app` (app.rs 4.8kL, assistant.rs 4.7kL)

## Principios

- `/j-space`: todo plan en `Plans.md`/`Tasks.md`/`.jspace/WORKSPACE.md` antes de código
- `/statem`: enums `Estado` + transiciones tipadas; estados inválidos no compilan
- `/rust-design`: newtypes (`AnimJobId`, `Resolution`), `Result/Option` en bordes, `unwrap_used = deny`, `clippy -D warnings = 0`
- `/rust-ui`: `fn render(&Estado) -> Frame`; cero I/O/spawn en `Ui::`
- Tokens: `compaction.prune:true`, `reserved:12000`, `small_model` para title; evitar contexts >100k

## Presupuestos (ver `docs/architecture.md:8`)

| Dominio | Constante | Valor |
|---------|-----------|-------|
| Doc | MAX_OBJECT_COUNT | 5000 |
| Expr | MAX_EXPR_LENGTH | 2000 |
| Anim | line_cap | 64KB |
| Undo | MAX_UNDO / MAX_UNDO_BYTES | 50 / 50 MiB VecDeque |
| UI | ASSISTANT_PANEL | 340..460 width |

## Gates locales

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
```

> MSRV Rust 1.92 — verificado en CI `ci.yml` matrix `['1.92', stable]` + `cargo metadata --locked` (ver `docs/architecture.md:9`).

## Cómo responder (calidad)

- Idioma: español rioplatense, conciso, sin adornos. Código y comandos en inglés original.
- Evidencia antes de afirmar: toda claim sobre el repo cita `archivo:línea` o comando ejecutado. Si no lo verificaste, dilo.
- Pregunta antes de suponer: tarea ambigua o destructiva → tool `question` primero, ejecuta después.
- Formato: respuesta corta + archivos tocados + cómo verificar (comando). Sin resúmenes de lo obvio.
- Ejecuta, no recites: `cargo` gates, `opencode debug config`, lecturas reales antes de concluir.
- Incertidumbre explícita: si algo no se verificó en este box, dilo antes que inventar.

## Skills — TODAS, la IA elige según proyecto

> Router: `skill({name:"skills-catalog"})` → lee `docs/SKILLS-INSTALLED.md` (528 locales) → carga 1-3 con el tool skill.
> Las 528 YA están en disco (`.opencode/skills/`, gitignored). Si falta algo: `bash scripts/install-skills.sh --group <rust|ui|memory|orchestration|mcp>`.

Locales (vía `skill` tool):
- `skills-catalog` — router maestro 200+ (USAR PRIMERO)
- `j-space` — workspace ledger Goal/Core/Verified/Open/Next, fast/full/loop gating
- `rust-performance` — puffin, rayon tessellation, wgpu compute, lyon, spade, rstar
- `rust-ui-ux` — tokens TYPE_XS..XXL, Scandinavian progressive disclosure (5/8/17 groups)
- `token-optimizer` — LLMLingua-2, compaction, prompt-cache, small_model routing
- `context-engineering` — opencode compaction, prune, reserved, instructions globs
- `grafito-architecture` — 16 crates, presupuestos, deuda BTreeMap/Statem
- `profiling` — puffin/criterion/wgpu-profiler, mide antes de optimizar

## Memoria híbrida (local-first + cloud team)

- Sesión caliente `/tmp` + promoción curada a `MEMORY.md` + `.jspace/WORKSPACE.md` en `session.idle`.
- Default local: `opencode-mem` plugin + MCP `memory` oficial. Grafo: `kuzu-memory`/`cognee-mcp` bajo demanda.
- PII siempre local. SaaS (`memory.store`/Mem0) solo hechos no-PII team.

## MCP (verificados `opencode mcp list` 6/6 connected)

Runtimes instalados user-local (sin sudo): bun 1.4.2 (`~/.bun/bin`), uv 0.12.10 (`~/.local/bin`). Comandos con paths absolutos (no dependen del PATH del shell) + `timeout: 30000`. `time` dado de baja (no hay paquete instalable; redundante con `date`); `fetch` va por `uvx mcp-server-fetch` (el `@modelcontextprotocol/server-fetch` da 404 en npm).

`filesystem` (scope `.`), `git`, `fetch`, `memory`, `sequential-thinking`, `context7` remote.
Full (GitHub/Figman/Playwright/DBHub/Qdrant) solo por agente y bajo demanda — ver catálogo §4.

## Agents especializados

Routing (sin solape):
- ¿1 tarea rápida? → `build` directo (default).
- ¿Plan barato arch/budgets/DAG? → `plan` (solo analiza, no ejecuta).
- ¿Varias subtareas independientes? → `orchestrator` (fan-out 5-8 workers + merge). Los 4 commands usan `build` a propósito (tareas atómicas).
- ¿Auditar código? → `reviewer` (PASS/FAIL, no toca nada) o `cerebro-audit` (audita Y fija, cerebro sin UI).
- ¿UI/perf? → `piel-ui` / `perf-profiler` (mide antes de optimizar).
- ¿Memoria? → `memory-keeper` (único escritor de MEMORY.md/.jspace).
- ¿Release? → `release` (changelog + tag, pregunta antes de publicar).
- ¿Tokens? → `token-saver` (aconseja routing; no ejecuta).

Roles:
- `cerebro-audit` — auditor cerebro sin UI (fija)
- `piel-ui` — optimización egui/wgpu
- `perf-profiler` — medición puffin antes de optimizar
- `orchestrator` — fan-out + merge, nunca hace el trabajo directo
- `reviewer` — staff review read-only
- `token-saver` — routing y presupuestos (solo aconseja)
- `memory-keeper` — curaduría memoria con presupuesto
- `release` — changelog + tag con aprobación
