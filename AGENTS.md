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

> MSRV Rust 1.88 — verificado en CI `ci.yml` matrix `['1.88', stable]` + `cargo metadata --locked` (ver `docs/architecture.md:9`).

## Skills

Invocar vía `skill` tool:
- `jspace` — workspace ledger Goal/Core/Verified/Open/Next, fast/full/loop gating
- `rust-performance` — puffin, rayon tessellation, wgpu compute, lyon, spade, rstar
- `rust-ui-ux` — tokens TYPE_XS..XXL, Scandinavian progressive disclosure (5/8/17 groups)
- `token-optimizer` — LLMLingua-2, compaction, prompt-cache, small_model routing
- `context-engineering` — opencode compaction, prune, reserved, instructions globs

## Agents especializados

- `cerebro-audit` — auditor cerebro sin UI
- `piel-ui` — optimización egui/wgpu
- `perf-profiler` — medición puffin antes de optimizar
