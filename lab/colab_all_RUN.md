# Pack Colab en paralelo al G3 (listo para tu pareo, 1 vez por VM)

El G3 sigue moliendo en el box (xargs -P10, ~5h, 0 FORCED). Esto corre en Colab
sin tocarlo. Todo es solo-matemática (puntos, CNFs), nada personal sale.

## 1) SAT con CPUs prestadas (Glucose3, NO GPU/TPU)
- Jobs viejos pendientes: `9c8940ea…/68b0ac88…/762b748a…` (509/510/874 k4)
- Job nuevo chico demo: `138e84c4…` (triángulo k4, `timeout_s=30`)
- G40x2 spindle (79v/165e, dQQ=1.0 exacto, k4+k5 SAT local): CNF chico, pedir si querés y lo exporto con su hash

Pasos por VM: `pip install -q python-sat` → pegar script → copiar la línea JSON →
`import_colab_result` en local (modelos SAT se re-chequean cláusula por cláusula).

## 2) T4 GPU — solo NN (torch), jamás SAT
- `lab/colG_pack.py --demo` (3/3 locales OK) y `lab/nn_color.py --demo`
- En T4: `!python nn_color.py --demo` y escala 874/553 k5 (CPU 42/34 s → segundos)
- Límite medido: SGD ingenuo traba en 509 k5 (43→31 vs kissat 25 ms); T4 acelera,
  no resuelve. Replicar loss+annealing del paper = semanas (JAX-port para TPU).

## 3) Numpy / sympy (verificación cruzada, no prueba)
- `8f9b37bf…` unit_sweep seeded n=500 seeds 1-4 (regen exacta bit a bit + points_hash)
- `38d4ea2b…` cas_crosscheck identidad trigonométrica (segunda opinión sympy)
- Vuelta: `import_colab_result` → regen local si entra en topes, si no `unverified`

## Herramienta nueva de esta ola
- `lab/g40x2_spindle.py`: construye G40×2 compartiendo P, rota θ=2·asin(1/2d),
  cierre dQQ=1.0 exacto, corre kissat k4/k5 en ms.
  [EVIDENCIA] G40x2 (n=79, E=165): k4 SAT + k5 SAT → [DESCARTADO] como testigo.
