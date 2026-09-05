---
name: j-space
description: Workspace ledger Goal/Core/Verified/Open/Next con gating fast/full/loop para Grafito.
---

# j-space

## Qué hago
- Ledger en `.jspace/WORKSPACE.md`: Goal, Core, Verified, Open, Next.
- Historial en `.jspace/history.json`.
- Gating: fast (<8k ctx) / full (<100k) / loop (multi-paso persistido).

## Cuándo usarme
Todo plan en `Plans.md`/`Tasks.md`/`.jspace/WORKSPACE.md` antes de código (`/j-space`).

## Protocolo
1. Lee `.jspace/WORKSPACE.md` + `Plans.md` + `Tasks.md`.
2. Clasifica banda: fast/full/loop según tamaño y riesgo.
3. Actualiza Next con 10 agentes paralelos por dominio si es auditoría.
4. Marca Verified solo con evidencia `cargo` gates.
5. Nunca inventes Verified sin comando ejecutado.

## Origen
Tiger V3.7 Apache-2.0 + `grafito-agent::ledger JSpaceLedger` + `router::TaskBand`.
