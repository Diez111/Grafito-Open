---
description: Lanza la caza χ=6 (spindle 5→6) en grafito-mode autónomo.
agent: grafito-mode
---

Leé `lab/witnesses/hn/README.md` y `.opencode/agents/grafito-mode.md`, luego
atacá la [CONJETURA] spindle: dos 509 (`lab/witnesses/hn/509_parts.vtx`)
pegados en pivote p (n=1017, k5=5085 vars).

Fase 1 (local, hoy): parseá el .vtx a coords numéricas, exportá DIMACS k=5
vía el pipeline (`export_dimacs` o script propio pineado contra
`UnitPairs`), corrí `sat_check` con kissat y buscá pares monocromáticos
bajo 5-coloreos (UNSAT con assumptions c(p)≠c(q) para candidatos p,q).
Fase 2 (Colab): baridos `sat_sweep` con timeouts largos para los pares
prometedores; import con verificación de modelo en local.
Fase 3 (hito): si algún k5 da UNSAT, re-corré ciego (otra seed, otro
solver si hay), archivá CNF+modelo+hashes en `lab/witnesses/hn/` y
reportá. Sin UNSAT re-verificado no hay hito: hay siguiente pivote.

Baseline que NO hay que re-probar salvo cambio de toolchain: Moser χ=4,
509/510 k4 UNSAT, 509 k5 SAT, triangular/grid 3-coloreables (ver README).
