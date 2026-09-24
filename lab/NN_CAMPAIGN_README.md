# Caza χ≥6 Hadwiger-Nelson — README maestro de campaña (2026-09-22)

Objetivo: grafo unit-distance con número cromático ≥ 6, verificado.
El plano está en 5–7 (de Grey 2018 / Isbell 1950); este repo ataca el 6.

Leyenda: `[CONJETURA]` hipótesis · `[EVIDENCIA]` run verificado ·
`[PRUEBA]` certificado re-ejecutado acá · `[DESCARTADO]` refutado por dato.
Ledger: `~/.local/share/grafito/lab_ledger.jsonl` (110 entradas, ~40 kinds:
topp39×29, mono-mine×15, cnf-build×9, rot-merge, proofs, tools, colab, tpu…).
Hito solo con `lab/witnesses/hn/CHI6_WITNESS.json` (no existe: no hay hito).

## 0. Lo que está corriendo AHORA

- **CORRECCIÓN G1 (2026-09-22): G1 es 4-COLOREABLE** [PRUEBA] (kissat k4 SAT
  12.8 s, coloreo verificado 0/3985). Nunca fue base 5-cromática: su mining
  k5 y el condmine-k5 eran de nivel equivocado (cerrados como vacuos).
  Par AB del autor recuperado por diff CNF: vértices (0,5) a √3.
  ABsame-k4 sigue sin decidirse local (kissat+nativo >280 s; CaDiCaL local
  muerto por SIGTERM): claim del autor, NO verificado acá.
- **Mining G1+AB (base verdadera) en fondo**: `parmine.sh` -P14, 273.430
  pares k5 — primer mining bien nivelado de la campaña.
- **Nativo `-march=native`: DESCARTADO** (0.083 vs 0.084 s en G1k5; >280 s
  ambos en ABsame: sin diferencia medible; se queda stock).
  Siguiente: `kissat` k5 directo a lo denso que salga.

## 1. Historia completa (qué se probó y qué dio)

1. **Bases 5-cromáticas re-certificadas** [PRUEBA] con kissat/CaDiCaL local:
   509, 510, 874, 553 (vértice-crítico entero), 517, 529, de Grey 1581,
   T703/T1405/T1299 (+1405 k4 UNSAT en 1581 s con symbreak, donde los autores
   abortaron). Detalle en `lab/witnesses/hn/README.md` (con `cnf_hash`).
2. **Spindle-mono** [DESCARTADO] en esos bases: 711k+ pares, 0 forzados.
3. **Spindle condicional G40** (Exoo–Ismailescu): reproducido (T3 UNSAT 0.026 s);
   G40×2 (79v, dQQ=1.0) da k5 SAT → [DESCARTADO] como testigo.
4. **Multi-copy/merge ciego**: 56 configs, todo k5 SAT. Núcleos densos y
   virtual-edges: 0. Rotaciones/merge y minimización: nada.
5. **Criba neuronal calibrada** (este frente, ver §2): k5→0 en 874 y G3(2131);
   k4→pisos 12+ y 35 (consistentes con 5-cromáticos, sin probar nada).

## 2. Frente neuronal + TPU v5e-1 (ver `lab/NN_CAMPAIGN_README.md` era este archivo; ahora unificado acá)

- `lab/nn_jax.py` (logits libres, Adam, entropía coseno J4, TabuCol, SLIM),
  `lab/nn_siren.py` (SIREN espacial, el que rinde), `lab/newcand.py` (C1–C4),
  `lab/g40x2_spindle.py`, `lab/exoL_gadget.py`, `lab/colG_pack.py`, `lab/nn_color.py`.
- Curva 874 k5: `39/40 → 12 (config J4) → 11 (512 restarts) → 9 (Tabu+SLIM)
  → 0 (SIREN 16×8000, 31 s TPU; [PRUEBA] 0/4461 local)`.
- G3: k5=0 [EVIDENCIA] 2ª vía; k4 piso 35 (indicio, misma firma 0.28% que 874).
- Notebooks `lab/colab_upload/`: 01 SAT · 02 sympy/numpy · 03 k4 pesado ·
  04 JAX/TPU (+x64 p/ T4 pendiente) · 05 mega · 06 SIREN (dio el 0) ·
  07 G3 · 08 criba C1–C4 (pin jax 0.7.2, instrumentado).
- Colab: 15 job-scripts; imports verificados (SAT model-checked; numpy 4/4
  hashes OK con mismatch honesto vs regen Rust en seed 3).
- Bugs cazados: Adam sin cuadrar, kernel-args, sys.exit, jax 0.11.2, float32/64.

## 3. Candidatos nuevos (criba 08/09/10)

08: C3 509×3 SÍ vale (13284 cruces → 0, denso y no trivial). C1/C2/C4 eran
DISJUNTAS (0 cruces: sus 0 valen como chequeo de pipeline, no como evidencia;
mea culpa registrado).
09: dobles con matching garantizado + miles de cruces: C5 874×2
(1748v/12616e/3694x), C6 509×2 (1018v/7428e/2544x), C7 G3×2 (4262v/34179e/9119x).
10: `lab/evocomp.py` evolutivo DISCRETO (twist continuo libre no pega: documentado;
genoma base×base/rot15°/tvec-unidad). Top cross 3948–5219, 8 uniones diversas.
Regla: SIREN-TPU 0 = descartado DE VERDAD; piso alto = `kissat` k5 en CPU
(`export_dimacs`+`sat_check` + test de pares virtuales con assumptions).

## 4. Abierto, en orden

1. Criba 08 de C1–C4 (pendiente geo del usuario).
2. T4-x64 del 04 (hipótesis precisión, 2 min).
3. Resistente de 08 → `kissat` k5 (rango chico y seguro).
4. k4 exacto del G3 = semanas de solver (solo si el indicio lo justifica).
5. Reanudar caza full = borrar `.jspace/STOP` + `scripts/grafito-loop.sh`.
