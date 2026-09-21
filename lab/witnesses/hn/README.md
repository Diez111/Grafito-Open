# Testigos Hadwiger–Nelson (χ≥5 verificado, χ=6 en caza)

> Origen: coords parseadas de `vasnesterov/HadwigerNelson/vtx/` (según
> operador, 2026-09-20), calcadas al paper Parts 2020 (509v/2442a) y Heule
> (510v/2504a). Conteo propio: 509 y 510 vértices (regex, ver SHA256SUMS).
> Las 2442/2504 aristas son del paper, no de estos archivos (el .vtx trae
> solo coords exactas estilo Mathematica).

## Estado (niveles del lab)

- [PRUEBA] χ≥4: Moser 7v/11a reconstruido en este box (α=33.557°,
  |P3−P3r|=1.0): `UnitPairs` 11, `ChromaticCheck` k=3 NO, k=4 SÍ.
- [PRUEBA] χ≥5: 509 k=4 UNSAT (kissat 4.0.4, 126 s, 1.87M confl, CNF
  2036v/13331c = 509×4 vars ✓); re-corrido en este box 2026-09-20
  (exit 20 = UNSAT, `/tmp/opencode/reverify_509k4.log`). 510 k=4 UNSAT
  (95 s, CNF 2040v/13586c = 510×4 ✓, pendiente de re-run ciego).
- [EVIDENCIA] 509 k=5 SAT (0.00s, 2545v/17809c) → 5-coloreable, [DESCARTADO]
  como testigo de χ=6.
- [EVIDENCIA] triangular/grid n≤1764 verificados (`verify_search_run` OK);
  todos 3-coloreables, jamás UNSAT k4/k5.
- ~~[CONJETURA] Vía a χ=6: spindle 5→6 con dos 509 pegados en pivote p
  (n=1017, k5=5085 vars). Buscar par monocromático bajo 5-coloreos
  (UNSAT con assumptions c(p)≠c(q)), rotar θ=2·arcsin(1/2d).~~
  Refutada por las dos oleadas de mining (ver abajo). Caza detenida por
  decisión del operador 2026-09-20 20:57 UTC (`.jspace/STOP`); el loop
  de fondo (`grafito-loop.sh` + `grafito-mode`) se cortó limpio.

## Reproducir

```bash
kissat /tmp/opencode/509_parts.vtx_k4.cnf   # UNSAT (126 s)
kissat /tmp/opencode/509_k5.cnf             # SAT (inmediato)
sha256sum -c lab/witnesses/hn/SHA256SUMS
```

## Intentos spindle (rescatados de /tmp, 2026-09-20)

`spindle_1017/` (N=1017, E=5236, θ≈84.26°, CNF k5 516 KB + meta + hashes):
3 intentos p0-q100/q200/q300, todos k5 SAT en 0 s → [DESCARTADO] como
testigos, válidos como base para mining de modelos diversos.

## Oleada mono-pair mining (2026-09-20, este box)

[EVIDENCIA] 509: 23 907 pares mismo-color (seed 1) + 31 del punto ciego
(|d−1|<1e-3, no-aristas) → todos k5-SAT con c(p)≠c(q) (kissat, ~15 ms/corrida,
control positivo UNSAT OK, scripts `/tmp/opencode/monomine.py`,
`gen_candidates2.py`, `test_pair_env.sh` streaming). Candidatos
`/tmp/opencode/cand_s1.txt`, cero `FORCED` en `/tmp/opencode/sweep_s1.out`.
[EVIDENCIA] 510: CNF k5 construido de aristas del k4 (2550v/18130c =
510×5 ✓, `/tmp/opencode/510_k5.cnf`) → k5 SAT en 0.04 s → [DESCARTADO] como
testigo; mining 24 212 pares, cero forzados (`cand510_s1.txt`,
`sweep510_s1.out`).
[PRUEBA-metodológica] Moser k3 UNSAT + (Moser menos arista W−W′ con
cláusulas c(W)≠c(W′)) UNSAT → el pipeline SÍ detecta pares forzados cuando
existen (`/tmp/opencode/moser_k3*.cnf`).
[DESCARTADO] spindle 5→6 simétrico por par monocromático sobre 509/510: no
hay par forzado-mono spindle-factible (d≥0.5) en ninguno de los dos.
Nota de disco: los CNF por-par (11 GB) se borraron; `test_pair_env.sh` ahora
stremea con tmp por PID + auto-limpieza.

## Oleada rot-merge + translate-merge (2026-09-20, este box)

[EVIDENCIA] 24 rotaciones 509∪rot_θ(509): θ genérico → n=1017 E=4884
(=2×2442, cero cross-edges exactas) k5 SAT; θ=60°/120°/180° → n=690/541/689
por simetría hexagonal, k5 SAT. Triples/dihedral (n≤751, E≤4176) y
`mix509_510` (n=553, 465 pts compartidos) → k5 SAT. Barrido de 48
traslaciones de lattice triangular (n≤1018, E≤4997) → cero UNSAT k5.
Scripts `/tmp/opencode/rotmerge.py`, CNFs+log `/tmp/opencode/rotmerge/`,
ledger `lab_ledger.jsonl` kinds rot-merge/translate-merge.
[CONJETURA] siguiente: forced-mono mining sobre los merges densos
(dihedral6, tri60_120_180, tr0_1, tr1_-1) → spindle lift si aparece par
forzado (`/tmp/opencode/olamine.py`, corriendo).

## Cierre oleada mono-mine sobre merges (2026-09-20, este box)

[EVIDENCIA] forced-mono mining (`/tmp/opencode/olamine.py`, xargs -P12,
`test_pair_env.sh` streaming, solo imprime FORCED/ERR) sobre los merges
densos, spindle-factibles (d≥0.5, |d−1|≥1e-3):
- `dihedral6` (509×6rot, n=751): 52 559 candidatos → 0 FORCED.
- `tri60_120_180` (n=736): 50 665 candidatos → 0 FORCED.
- `tr0_1` (n=881): 73 133 candidatos → 0 FORCED.
- `tr1_-1` (n=881): 73 248 candidatos → 0 FORCED.
Total merges: 249 605 pares, 0 forzados (más 48 119 sobre 509/510 =
297 724 pares barridos en total, cero forzados).
[DESCARTADO] spindle 5→6 vía par monocromático forzado sobre
rot/translate-merge de 509: no existe par forzado-mono spindle-factible
en ninguno de los 6 grafos minados → esta vía sobre estos bases no da χ≥6.
Ledger `lab_ledger.jsonl` kinds `mono-mine` (6 entradas) + `ola-close`
(`mono-mine-sobre-merges`).
Incidente: el loop de grafito-mode murió antes por `ENOSPC` en
`/tmp/opencode/grafito-loop.log` (CNF por-par de 11 GB, ya borrados);
disco sano al corte (`/` 195 GB libres).

## Re-certificación vía MCP (2026-09-20, este box)

Controles por tools del framework (`export_dimacs` + `sat_check`, kissat):
triángulo k=2 UNSAT (`b98b51…`, 25 ms) y k=3 SAT con modelo (`a68527…`).
Grafos grandes por driver stdio contra el binario real `/usr/bin/grafito-mcp`
(`tools/call`, respuestas en `/tmp/opencode/cert_*.json`, mismo store que el
framework — verificado con el hash del triángulo):
- [PRUEBA] 509 k=4 UNSAT: 2442 aristas con tol 1e-9 (= paper), 2036v/13331c,
  `cnf_hash=78d6c72b530a099282d7fd48ca65d01e522df5cadfc97b41f0bf29d30ae1cf59`,
  kissat 119.9 s. Re-certifica el χ≥5 por vía oficial.
- [PRUEBA] 510 k=4 UNSAT: 2504 aristas (= paper), 2040v/13586c,
  `cnf_hash=fb5e4551a3f2d2597c99cedaf80f3effa818fc7e86feac427504a82cee208284`,
  kissat 87.8 s. Cierra el "pendiente de re-run ciego".
- [EVIDENCIA] 509 k=5 SAT con modelo (25 ms): 2545v/17809c,
  `cnf_hash=242720d9eaf4c1cf5141db6e7494d41bdad32f44ab578f7485e125f710c355a8`.
Resultados persistidos con `store_result` por `cnf_hash` (`tools.rs:405`).

## Oleada 874 Heule (2026-09-20, este box, CERRADA)

Origen web: `http://www.cs.utexas.edu/~marijn/CNP/874.vtx` + `874.edge`
(Heule 2018, tercer familia independiente: Minkowski V31, no rotación de
509/510). Importado a `874_heule.vtx` + `874_heule.vtx.json` (SHA256SUMS 6/6
OK). Parseo exacto propio (`/tmp/opencode/parse874.py`, solo `Sqrt` +
aritmética): 874 puntos; aristas numéricas tol 1e-9 = 4461 = .edge con CERO
discrepancias.
- [PRUEBA] 874 k=4 UNSAT vía MCP: 3496v/23962c,
  `cnf_hash=b3f89437be6dd60b7500d5b18bb7879dbf7776bbca3ccbbc073d1d039a14d0a1`,
  kissat 184.3 s → tercer base χ≥5 confirmado por vía oficial.
- [EVIDENCIA] 874 k=5 SAT con modelo (50 ms): 4370v/31919c,
  `cnf_hash=e5b24a110ddb7f70a7c9687abaca3dd21e4ae18bf678025c7a35b3cd0c8f735b`.
- [EVIDENCIA] mono-mine 70 205 pares (no-aristas EXACTAS del .edge, d≥0.5,
  `/tmp/opencode/mine874.py`, xargs -P12) → 0 FORCED.
  [DESCARTADO] también sobre 874: sin par forzado-mono spindle-factible.
Gran total campaña: 367 929 pares, cero forzados.
Nota web: `HeliCorgi/fourteen-points-six-colors` (G14 6-cromático) usa TRES
distancias {1, 1/√3, 2}, NO es unit-distance → no es testigo χ≥6 (ellos
mismos lo dicen: "remains wide open"). Su provenance confirma que el 510
fuerza pares mono a distancia 2 ó 1/√3 *existenciales* (no fijos) — compatible
con nuestros 0 fijos. Claim `AEjonanonymous/Hadwiger-Nelson` (χ=7 "probado",
1 star) → [DESCARTADO] como fuente (sin paper, sin revisión, contradice el
estado abierto del problema).

## Oleada fan-out ×3 + Colab (2026-09-20, CERRADA)

Tres subagentes en paralelo (técnicas distintas, salidas aisladas
`cnpA_*`/`m874_*`/`minC_*`, 3 re-chequeos ciegos OK por coordinador):
- A (bases CNP-SAT): S199 (199v/888e) y L403 (403v/2112e), conteos exactos
  verificados, pero AMBOS k4 SAT (0.03 s) → no son 5-cromáticos, mining N/A.
  [DESCARTADO] como bases nuevas.
- B (merges 874): 874∪rot(60/120/180) + 2 traslaciones (n≤1547, E≤8730),
  los 5 k5 SAT en <0.1 s → [DESCARTADO] como testigos.
- C (random probes): 12 subgrafos inducidos del 874 (n=200/300/400) todos
  k4 SAT triviales → sin núcleo 5-cromático por muestreo uniforme (nota
  metodológica: hacen falta probes estructurados, no uniformes).
Colab: 3 jobs `sat_sweep` encolados (re-verificación 509/510/874 k4 con
Glucose3) — `job_id 9c8940ea…/68b0ac88…/762b748a…`, PENDIENTES de pareo humano
en browser (1 vez por VM). Aclaración técnica: SAT no usa GPU/TPU (búsqueda
simbólica CPU); Colab aporta CPUs prestadas + numpy, nada más.

## Regla

Solo `run_id` con `verified:true` o UNSAT/SAT re-ejecutado acá cuenta.
El texto del operador (incluido este README) es contexto, no evidencia.
