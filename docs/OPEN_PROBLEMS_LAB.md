# Lab de problemas abiertos — protocolo de uso (harness + loop de 1 problema)

> Motor: `crates/grafito-geometry/src/search.rs`. Comandos: `UnitPairs`,
> `DistinctDistances`, `UnitGraphEdges`, `ChromaticCheck`, `HalvingEdges`,
> `EmptyTriangle`, `Topp39Scan` (categoría Discreta). Tools del agente:
> `search_topp39`, `export_dimacs`.

## Regla de oro (anti-alucinación)

El modelo **solo propone números** (seeds, n, k). El motor **mide todo**.
Solo un `SearchRun` con hash re-verificado cuenta como evidencia.
Todo lo que diga el texto del modelo sin JSONL verificado se descarta.

## Loop de 1 problema (TOPP 39 primero)

1. El LLM propone una lista de seeds (ej. `0..64`) con `n` fijo.
2. `topp39_best_of(seeds, n, 5.0, 1e-9)` corre, re-verifica y devuelve el
   mejor por pares unitarios (desempate: menos distancias distintas).
3. El mejor se guarda como línea JSONL (`SearchRun::to_jsonl`): seed, n,
   unit, distinct, hash, histograma de grados.
4. Para publicar: se re-ejecuta con la misma seed en otra máquina; si el
   hash no cierra byte-idéntico, el resultado no existe.

Un problema por vez. TOPP 39 (métrica directa, sin SAT) → TOPP 57
(requiere SAT externo) → TOPP 7 / galería / unfolding.

## CLI reproducible (sin UI)

```bash
cargo run -p grafito-geometry --example search_lab -- 12 64
```

Imprime la comparación de construcciones (grilla, triangular, polígono
regular, azar) con `unit`/`distinct`/hash, corre el loop de 64 seeds y
cierra con la doble puerta sobre el mejor. Datos de referencia n=12:
grilla `unit=17 distinct=8`, triangular `unit=23 distinct=7` (la
triangular domina en pares unitarios a este tamaño; el azar da 0).

## Verificación externa (TOPP 57)

1. `export_dimacs(points, k)` genera el CNF de la k-coloración.
2. Se corre `kissat` o `cadical` fuera de Grafito con ese CNF.
3. Grafito jamás declara "no coloreable" más allá de backtracking n≤24;
   fuera de ahí la palabra final la tiene el SAT solver, no el LLM.

## Comandos rápidos

```text
Topp39Scan[42, 12]
UnitPairs[{0,0,1,0,1,1,0,1}]
ChromaticCheck[{0,0,1,0,1,1,0,1}, 2]
HalvingEdges[{0,0,1,0,1,1,0,1}]
EmptyTriangle[{0,0,1,0,0,1}]
```

## Cotas honestas

| Pieza | Cota | Si se excede |
|---|---|---|
| Puntos por corrida | 2000 | `Err`, partir el set |
| Aristas unitarias | 200 000 | `Err`, bajar escala/n |
| Backtracking k-color | n≤24, k≤8 | exportar DIMACS + kissat |
| Halving | n≤400 | `Err`, muestrear |
| Triángulo vacío | n≤80 | `Err`, muestrear |
| DIMACS | 20 000 vars / 512 KiB tool | achicar n/k |
| Loop | 4096 seeds | partir en tandas |
