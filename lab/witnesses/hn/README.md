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
- [CONJETURA] Vía a χ=6: spindle 5→6 con dos 509 pegados en pivote p
  (n=1017, k5=5085 vars). Buscar par monocromático bajo 5-coloreos
  (UNSAT con assumptions c(p)≠c(q)), rotar θ=2·arcsin(1/2d).

## Reproducir

```bash
kissat /tmp/opencode/509_parts.vtx_k4.cnf   # UNSAT (126 s)
kissat /tmp/opencode/509_k5.cnf             # SAT (inmediato)
sha256sum -c lab/witnesses/hn/SHA256SUMS
```

## Regla

Solo `run_id` con `verified:true` o UNSAT/SAT re-ejecutado acá cuenta.
El texto del operador (incluido este README) es contexto, no evidencia.
