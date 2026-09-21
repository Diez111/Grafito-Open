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

## Oleada agente D: 553 + 517 (2026-09-20, CERRADA)

Resto del CNP-SAT: 553 (el del paper Heule) y 517, conteos exactos
(2722/2579 aristas, 0 mismatch numérico), importados (`553_heule.vtx`,
`517_heule.vtx`, SHA256SUMS 10/10 OK).
- [PRUEBA] 553 k=4 UNSAT (55.4 s) + 517 k=4 UNSAT (82.5 s), recheck ciego
  del 553 OK → 4ª y 5ª familia 5-cromática confirmadas.
- [EVIDENCIA] ambos k=5 SAT (0.04 s).
- [EVIDENCIA] mono-mine 50 000 pares (25 000 c/u, d≥0.5) → 0 FORCED.
  [DESCARTADO] también sobre 553/517.
Gran total campaña: 417 929 pares, cero forzados.
Recon: no existen variantes 553 en CNP-SAT (solo 553.vtx/edge, byte-idéntico
al minado) — corrección al reporte web previo.

## Oleada núcleos densos (2026-09-20, CERRADA)

180 núcleos densos (bolas geométricas r1.5 + grafo-radio 2 sobre top-30
grados de 874/553/510) + 6 uniones de núcleos: los 186 k4 SAT instantáneo
(≤0.1 s), 0 UNSAT. [DESCARTADO] récord <509 por esta vía. Insight: la
5-cromaticidad de estos grafos es GLOBAL (el 874 completo tarda 184 s en
UNSAT; cualquier núcleo denso cae en 0.0 s) — no vive en vecindades densas.
Script `/tmp/opencode/denE_hunt.py`, ledger kind `dense-cores`.

## Rama virtual edges + RSI (2026-09-20, CERRADA)

Nueva medición (no hecha antes): pares forzado-DIFERENTES en 553 a las
distancias de forcing de HeliCorgi. 2404 no-aristas (663 a d~=2, 1741 a
d~=1/√3), test de igualdad forzada (`mine_virtual.py`, 10 cláusulas,
controles edge→VIRTUAL y same-color→SAT OK) → **0 VIRTUAL**.
Hueco del cap 25k cerrado: 578 pares mismo-color a esas distancias,
testados forced-SAME → 0 FORCED. Ledger kinds `virtual-mine` + `mono-mine`
(553_d2-d577). Conclusión: el forcing existencial que reporta HeliCorgi en
510 no deja rastro FIJO en 553 a esas distancias (ni same ni different).
RSI usado de verdad: `policy_suggest` (49 runs → triangular domina) +
`replay_score` (triangular-500 1412 vs grid-500 955, determinista) → el
ganador optimiza DENSIDAD, no forcing (lattice triangular es 3-coloreable);
no sirve a HN y se documenta en vez de insistir.

## Rama neuronal estilo 2024 (2026-09-20, instrumento listo, límite medido)

Tool nueva `lab/nn_color.py` (torch, CPU/T4, imprime 1 JSON): coloreo
probabilístico + loss de pares monocromáticos + SGD (núcleo del paper
2404.05509). Validada en triángulo (k2 piso 1/3 +1 violación = espejo UNSAT;
k3 →0 +0 = espejo SAT). En 509 real: k4 trabado en 119 violaciones (espejo
UNSAT correcto) pero k5 trabado en 43→31 (kissat lo colorea en 25 ms).
Conclusión medida: el SGD ingenuo NO alcanza donde SAT sobra; el programa
real (replicar loss custom + annealing del paper, escalar en T4) es proyecto
de semanas, no de esta noche. TPU: inútil para SAT y para este script
(torch no corre en TPU; JAX-port como follow-up). Ledger kinds
`nn-instrument` + `nn-search`. Para correr en Colab (requiere TU pareo):
Runtime → T4 GPU → subir `lab/nn_color.py` → `!python nn_color.py --demo`.

## Oleada 529 (2026-09-20, CERRADA)

Web→CNP-SAT (Heule 2019): 529v/2670e exactos (0 mismatch), importado
(`529_heule.vtx`, SHA 12/12 OK).
- [PRUEBA] k4 UNSAT (85 s) + k5 SAT (0.04 s) → 6ª familia 5-cromática.
- [EVIDENCIA] mono-mine 25 879 pares → 0 FORCED. [DESCARTADO] vía spindle.
Gran total campaña: 443 808 pares, cero forzados.

## Oleada T650/zach7036 (2026-09-20, VERIFICACIÓN INDEPENDIENTE EXITOSA)

Repo 0-stars con gadget T703 (terminales a d=2 forzados mismo color en todo
4-coloreo) + composición 1299v. Verificación propia:
- [EVIDENCIA] CNF congelado `t703_endpoints_different.cnf` (sha OK, 2812v):
  **kissat UNSAT en 29 s** = 4ª familia solver independiente (ellos: CaDiCaL
  ×2, Glucose). El claim de forcing pasa de [CONJETURA] a [EVIDENCIA].
- Geometría: .edge sha OK; 3861/3861 aristas numéricas idénticas (dif=0);
  d(terminales)=2.0 exacto; rotación cos=7/8 lleva terminales a 1.0 exacto
  (analítico: 2·2·sin(θ/2)=1). .vtx difiere en sha upstream (solo formato:
  sin CR, contenido validado numéricamente). Importado (`t703_zach.*`).
- [EVIDENCIA] T703 k5 SAT (0.06 s); mono-mine 44 738 pares → 0 FORCED:
  ni siquiera un gadget CON forcing probado a nivel 4 tiene par forzado a
  nivel 5. [DESCARTADO] vía spindle también acá.
- [EVIDENCIA] REPRODUCCIÓN TOTAL del 1405 (`t1405_graph.json` oficial):
  1405/1405 puntos matcheados a la construcción propia (2×T703 + rot 7/8);
  aristas propias tol 1e-9 IDÉNTICAS a las suyas (dif simétrica 0, 7723);
  el near-miss a 6e-7 queda excluido (era ruido float); su 5-coloreo
  explícito VALIDADO (0 violaciones). k4 directo con symbreak en curso.
  Ledger kind `t650-repro`.
- [PRUEBA] k4 DIRECTO sobre el 1405 completo con symbreak (triángulo fijo):
  **UNSAT en 1581 s** (`cnf_sha=8065f45d0df7c5b8b4668e3950b7d1dee3ff9157201e4aa5d29e3d1f7ae9b2df`,
  5620v). Ellos abortaron a los 630 s sin symbreak; el symbreak (50× en 553)
  volvió factible lo infactible. Prueba directa de no-4-coloreabilidad sin
  pasar por el argumento composicional.
- Minería nivel-5 sobre el 1405: 180 533 candidatos → 0 FORCED.
  [DESCARTADO] vía spindle también en la construcción gadget-compuesta.
Gran total campaña: 669 079 pares, cero forzados.

## Oleada fan-out ×4 herramientas + T1299 (2026-09-21, CERRADA)

- F (kit Parts Type-J): `lab/gadF_kit.py` construye Moser (k3 UNSAT/k4 SAT)
  y ciclo-J 13v/23e 4-cromático con forcing no-trivial. Maquinaria OK.
- G (pack Colab NN): `lab/colG_pack.py` + `lab/colG_RUN.md`, demos 3/3
  locales. LISTO para T4 (falta pareo humano).
- H (T650/1299): CONFIRMA total — forcing UNSAT ×2 re-verificado kissat
  (46 s + 115 s), 1299v/6757e recount exacto, 5-coloreo 0 violaciones,
  mining 20 000 → 0 FORCED. DRAT/LRAT sin checker en el box.
- I (symbreak+shrink2): `lab/symI_cnf.py` + `lab/symI_shrink2.py`; 60×
  re-verificado (58.17 s→0.97 s); shrink 623 checks, invariante OK.
Ledger kinds `tools-tested` + `t650-1299-verify`.

## Oleada fan-out ×5 total (2026-09-21, CERRADA)

- J1 (Type-J constructivo): `lab/jcyJ_kit.py` — mono-pair 8/3 + escalados
  η/ρ con forcing k3 probado; ciclo-J 25v χ=4 (k3 UNSAT, k4 SAT). Obstáculo
  documentado: compartir terminal con ángulo η crea arista emergente.
- J2 (1299 full):unció bug de harness (índices 1-based vs 0-based; minings
  propios 0-based NO afectados). Re-mine TRUE-20k corregido: 0 FORCED.
  Virtual 686: 0 VIRTUAL. Espacio 1299 (38 620) cubierto entero → 0.
- J3 (shrink): 1299 k4 TIMEOUT 900 s (shrink inviable ahí); 553 localmente
  minimal (2 seeds ×623 tests, 0 removidos, invariante OK).
- J4 (NN scale): 874 k5 y 553 k5 → 0 violaciones (42 s/34 s CPU).
- J5 (Mixon 1577): k4 SAT en ambas bases de índices → descartado.
Ledger kinds respectivos + `harness-bugfix`.
Gran total campaña: 711 367 pares (669 079 + 2 982 virtuales-553 + 39 306
del 1299), cero forzados.

## Oleada multi-copy composer (2026-09-21, CERRADA)

56 configuraciones (estrellas/rings/cadenas/hubs de 3-8 copias de 509/553/
510, hasta 4852v/31ke, cierres en anillo geométricamente exactos) → todas
k5 SAT en 114 s totales. [DESCARTADO] salto cromático emergente por unión
de copias: ni los anillos cerrados frustran el 5-coloreo. Nota: m>2 copias
no comparten arista completa en el plano (fan sobre vértice).
Ledger kind `multi-compose`.

## Oleada de Grey 1581 (2026-09-21, FAMILIA CONFIRMADA)

S transcripto 1:1 del paper (39 puntos) → Sa=397 → G=1581, aristas 7877
(desvío 0%). k5 SAT (0.1 s) + mining 15 000 → 0 FORCED. k4: kissat TIMEOUT
2×, **CaDiCaL UNSAT** → 7ª familia 5-cromática confirmada localmente.
Ledger kind `cnf-build` (deGrey-1581).

## Oleada Haugland 2131 (2026-09-21, EN CURSO)

Construcción exacta por etapas (21/1042/740+3985/1066+6264/2131+12530,
k5 SAT 0.39 s). Forcing G1 con CaDiCaL en fondo. Full-mining G3 (420 555)
+ G1 (50 149) en fondo. Ledger kind `cnf-build` (Haugland-G3-2131).

## Oleada pipeline Heule/Parts (2026-09-21, CERRADA)
(escala proofs: CaDiCaL 56 min sin terminar en 1405, proof 5.2 GB creciendo
→ trim inviable en box; vale kissat UNSAT 26 min + 5-coloreo. Ledger kind
`proof-scale`.)

- L1 (hipergrafo): 553 VÉRTICE-CRÍTICO — los 553 vértices críticos
  (sacar cualquiera mata la 5-cromaticidad), 2143 tests, 0 removidos.
- L2 (siembra): V31 31v/60e validado (= paper); 11 candidatos L∪ρS/η
  todos k4 SAT → sin familia nueva.
- L3 (Exoo): corrección importante — el 8/3 incondicional es de HARDER
  (745v→G367), no de Exoo. G40 (`lab/exoL_gadget.py`): forcing
  CONDICIONAL verificado (P=Q solo si no-mono √(11/3)).
- Toolchain proofs: drat-trim compilado OK; CaDiCaL en compilación
  (primer intento falló por workdir, relanzado).
Ledger kinds `hyper-min` + `sow` + `gadget-exoo`.

## Regla

Solo `run_id` con `verified:true` o UNSAT/SAT re-ejecutado acá cuenta.
El texto del operador (incluido este README) es contexto, no evidencia.
