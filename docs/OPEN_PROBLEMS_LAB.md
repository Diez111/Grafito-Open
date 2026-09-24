# Lab de problemas abiertos — protocolo de uso (harness + loop de 1 problema)

> Motor: `crates/grafito-geometry/src/search.rs`. Servidor MCP:
> `crates/grafito-mcp` (stdio, binario `grafito-mcp`, 42 tools + 9 recursos;
> conteo pineado en `crates/grafito-mcp/src/protocol.rs:448`).
> Comandos: `UnitPairs`, `DistinctDistances`, `UnitGraphEdges`,
> `ChromaticCheck`, `HalvingEdges`, `EmptyTriangle`, `Topp39Scan` (categoría
> Discreta). Tools MCP: 7 lab (`search_topp39`, `export_dimacs`,
> `verify_search_run`, `topp39_best_of`, `sat_check`, `check_bounds`,
> `execute_command`) + 21 proxedas del asistente (análisis, pedagógicas y
> base, paridad automática vía `SafeGrafitoDispatcher`). Excluidas a
> propósito: `web_search` (red) y el harness-2 viejo (reemplazado).
> Prompt del agente: `prompts/grafito-lab-agent.md`.

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

## Servidor MCP (`grafito-mcp`)

```bash
cargo run -p grafito-mcp --offline
```

Protocolo: MCP 2024-11-05 por stdio (una línea JSON-RPC por mensaje).
`initialize` → `tools/list` (33) → `tools/call` / `resources/read`.
Recursos: `grafito://ledger`, `grafito://run/{id}`, `.../cnf/{k}`,
`.../result`, `grafito://bounds/known`, `grafito://catalog/commands`
(650 comandos, 25 categorías). El server es el único escritor del
ledger (`$XDG_DATA_HOME/grafito/lab_ledger.jsonl` o `GRAFITO_LAB_LEDGER`) y
solo guarda lo verificado. Sin `ledger_append` a propósito.

Cobertura total: `execute_command` corre de verdad los 650 comandos sobre
un doc efímero (1 paso o hasta 32 secuenciales con inventario final de
etiquetas; I/O de archivos bloqueado con guía); las 21 del asistente
(`evaluate_expr`, `diff`, `integrate`, `limit`, `solve_*`, `verify_step`,
`groebner_gate`, pedagógicas, `run_command` como propuesta pura) van por
proxy sin duplicar lógica. Límite honesto: el MCP es stateless, no ve el
documento abierto en la ventana de Grafito (cada llamada parte de un doc
vacío salvo sus propios `steps`).

Topes altos configurables (sin presupuesto artificial, con resistencia):
100k puntos (índice espacial O(n)), 2M aristas, 500k vars DIMACS, 50 MiB CNF,
100k seeds, SAT hasta 24 h (`timeout_ms: 0` = sin timeout explícito).
Env: `GRAFITO_LAB_MAX_POINTS/EDGES/DIMACS_VARS/CNF_BYTES/SEEDS/LEDGER`.
Lo O(n²)/O(n³)/O(n⁴) imposible sigue honesto: distintas topa en 10k,
halving/triángulo piden muestreo.

## Verificación externa (TOPP 57)

1. `export_dimacs(points, k)` genera el CNF (SHA-256 `cnf_hash`, guardado).
2. `sat_check(cnf_hash, solver, timeout_ms)` corre `kissat`/`cadical` sidecar;
   si falta el binario da error honesto con guía (estilo `FfmpegMissing`).
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

## Bucle RSI (dream loop)
Detalle completo: `docs/RSI_ROADMAP.md`.
Tools nuevas (propuesta): `lean_check`, `lean_submit`,
`policy_suggest`, `replay_score`.
Recursos: `lean://proofs/{id}`, `policy://archive`.
El dream loop solo muta la política; nunca el cerebro ni el ledger.
La candidata incluye la actual como fallback (no-regresión).
Sin `verify_search_run` OK no hay evidencia; sin `lean_check` OK no hay `[PRUEBA]`.
