# Proof toolchain (HN lab, 2026-09-21)

Verificación de UNSAT por certificados (no por palabra del solver).

## Fuentes (upstream, compilar user-space, sin sudo)

- drat-trim: https://github.com/marijnheule/drat-trim (un .c)
  `gcc -O2 drat-trim.c -o drat-trim`
- CaDiCaL: https://github.com/arminbiere/cadical
  `git clone --depth 1 ... && ./configure && make -j16`
  Emite DRAT: `cadical --binary=false CNF PROOF` (exit 20 = UNSAT).

## Resultados verificados (este box)

| grafo | k4 UNSAT | proof check (drat-trim) | core (vértices) |
|---|---|---|---|
| 553 | CaDiCaL 1 s (symbreak) | s VERIFIED 0.75 s | 553 (mínimo) |
| 874 | CaDiCaL 10 s (symbreak) | s VERIFIED 8.2 s | 874 (mínimo) |
| 509 | CaDiCaL ~1 s | vía core re-solve UNSAT 8.5 s | 509 (mínimo) |
| 510 | CaDiCaL ~1 s | core generado | 510 (mínimo) |
| 529 | CaDiCaL ~1 s | core generado | 529 (mínimo) |
| 1405 | kissat 1581 s (symbreak) | CaDiCaL proof en curso | — |

Conclusión: 553/874/509/510/529 son mínimos por núcleo (ningún vértice
sobra en el core) — converge con minimización hipergráfica (553: 2143
tests, 0 removidos). CNFs con symbreak (triángulo fijo): ver symI_cnf.py.
