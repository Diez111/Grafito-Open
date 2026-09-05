---
name: grafito-architecture
description: Arquitectura 16 crates Grafito, presupuestos, gates y deuda conocida.
---

# grafito-architecture

```
grafito-geometry ──┐
grafito-complex ──┼─> grafito-core ──> grafito-command ──> grafito-app
grafito-render ───┘          │                    ▲
grafito-anim ────────────────┼─> grafito-assistant┘
grafito-whiteboard ──────────┘
grafito-ui ───────────────> grafito-app (Piel)
```

- Cerebro puro (sin egui/wgpu): core, geometry, command, whiteboard, profile, pedagogy, plugins.
- Puente: anim IPC JSON v1 stdio a Python/manim.
- Piel: ui (tokens) + app (app.rs ~5kL, assistant.rs ~4.7kL God Object).

## Presupuestos
Doc MAX_OBJECT_COUNT 5000, Expr MAX_EXPR_LENGTH 2000, Anim line_cap 64KB, Undo 50/50MiB VecDeque, panel 340..460, assistant-types 8192/2048/8/60s/512KiB.

## Deuda
`HashMap→BTreeMap` determinismo, `ValidatedDocument` parcial, `app.rs` extraer `controllers.rs`, `assistant.rs` Statem formal, `fill_compute` lazy (ahorra 128MiB).

## Gates + MSRV 1.92
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
```
