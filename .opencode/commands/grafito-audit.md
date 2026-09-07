---
description: Audita Grafito con skill jspace y dispersa agentes por dominio.
agent: build
---

Usa `skill({name:"j-space"})` y luego audita:
1. Lee `.jspace/WORKSPACE.md`, `Plans.md`, `Tasks.md`.
2. Dispersa 10 agentes paralelos por dominio (panic, concurrencia, memoria, geometría, persistencia, UI, supply, tests, perf, docs) con gates cruzados.
3. Actualiza Verified solo con evidencia de `cargo` gates. Open/Next con dueños.
