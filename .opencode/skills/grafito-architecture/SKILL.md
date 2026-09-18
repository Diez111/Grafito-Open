# Grafito — Architecture on demand

> Referencia completa: `docs/architecture.md` (fuente de verdad, no se inyecta).
> Este skill es el recordatorio operativo para no cargar 10k tokens por sesión.

## DAG (18 members + root)

```
geometry ─┐
complex  ─┼─> core ──> command ──> app
render   ─┘     │                 ▲
anim ───────────┼─> assistant ────┘
whiteboard ─────┘
ui ─────────────> app (Piel)
OTROS: agent (tools hoja), assistant-types (contratos), classroom (P2P), pedagogy (curriculum),
profile (BKT/Leitner), plugins (registry), ggb (paridad), tex (LaTeX offline)
```

- Cerebro puro (sin egui/wgpu): core, geometry, command, complex, whiteboard, profile,
  pedagogy, plugins, assistant, assistant-types, agent.
- Puente: anim (IPC JSON v1 stdio; nativo 13 plantillas + ffmpeg sidecar).
- Piel: ui (tokens/theme) + app (app.rs ~9.4kL, assistant.rs ~14.4kL, panels.rs ~9.5kL).

## Presupuestos (verificados 2026-09-17)

| Dominio | Constante | Valor |
|---|---|---|
| Doc | MAX_OBJECT_COUNT / MAX_EXPR_LENGTH / MAX_DOCUMENT_SIZE | 5000 / 2000 / 10M |
| Matriz | MAX_MATRIX_DIMENSION / ELEMENTS | 1000 / 1M |
| Undo | MAX_UNDO / MAX_UNDO_BYTES | 50 / 50 MiB (VecDeque) |
| Anim | line_cap / diagnostics / frames | 64KB / 64 líneas / 1500 long-form |
| Assistant | RequestBudget entrada/salida/pasos/timeout | 8192 / 2048 / 8 / 60s |
| Assistant | Adjuntos | 512 KiB, 1 MiB total, 2 adjuntos, 2 MiP |
| UI | Panel asistente / rail / drawer | 300..520 / 68 / 292..440 |
| GPU | domain_coloring_compute | 250k cells/dispatch |
| Tex | TEX_INPUT_MAX_BYTES | 8 KiB |

## Gates (MSRV 1.92; `cargo clippy -D warnings = 0`)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
```

## Deuda conocida (no repetirla como si fuera bug)

- `controllers.rs` reales con tests; wiring fino de `GrafitoApp` por puente (`from_parts`/`into_parts`) P2.
- `AssistantTurnState` en `assistant_jobs.rs` derivado del runtime; sin consumidor productivo aún (P2).
- `ViewMode/Perspective/CanvasMode` redundancia (P1); god objects por partir (`commands.rs`, `panels.rs`, `assistant/lib.rs`).
- `fill_compute` aún `None` (habilita lazy si `ImplicitCurve != Eq`).
- CI: `coverage` 71% y `mutation`/`bench-regression` son schedule, no gates de PR.

## Contexto del asistente de Grafito (lo que sí importa)

- Gate de contexto: el TEXTO nunca se descarta si cambia el documento (aviso informativo);
  las propuestas se re-verifican contra el estado actual y el Apply sigue fail-closed
  (`PlanBasis` en `assistant_plan.rs::validate_basis`).
- Telemetría local opt-in de tokens: `GRAFITO_USAGE_LOG=1` → JSONL en `$XDG_DATA_HOME/grafito/assistant_usage.jsonl`.
- Prompts: system ~1.7k tokens (cacheable por proveedor); contexto de objetos truncado por
  presupuesto con nota honesta (`bounded_context_prompt`).
