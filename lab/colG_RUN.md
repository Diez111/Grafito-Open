# colG_RUN — instrucciones EXACTAS (humano + Colab T4)

Pack del agente G: `colG_pack.py` (solo torch + stdlib, sin numpy ni nada más).
Necesitás subir 2 archivos al Colab: `colG_pack.py` y `509_parts.vtx.json`
(este último está en `/tmp/opencode/509_parts.vtx.json` del box).

## 1. Abrir Colab con GPU T4 (3 clicks)

1. https://colab.research.google.com → cuaderno nuevo.
2. Menú **Runtime** (Entorno de ejecución) → **Change runtime type**
   (Cambiar tipo de entorno) → **Hardware accelerator: T4 GPU** → **Save**.
3. Verificá la GPU en una celda:
   ```
   !nvidia-smi -L
   ```
   Esperado: una línea con `Tesla T4`.

## 2. Subir archivos (2 clicks)

1. Panel izquierdo → icono carpeta (Files) → **Upload** (Subir).
2. Subí `colG_pack.py` y `509_parts.vtx.json` (quedan en `/content/`).

## 3. Chequeo de torch (1 celda, ~10 s)

```
!python -c "import torch; print(torch.__version__, torch.cuda.is_available())"
```

Esperado: versión (p. ej. `2.x`) y `True` (hay CUDA).
Si da error de import: `!pip install torch` y reintentá.

## 4. Demos de validación (1 celda, ~30 s en T4)

```
!python /content/colG_pack.py --demo
```

Output esperado (una sola línea JSON en stdout; el detalle va a stderr):

- `triangle_k2`: `loss_best ≈ 0.333` (piso 1/3, lo óptimo con 2 colores),
  `violated = 1`, `pass = true`.
- `triangle_k3`: `loss_best ≈ 0.0000x` (→ 0), `violated = 0`, `pass = true`.
- `squareC4_k2`: `loss_best ≈ 0.0000x` (→ 0), `violated = 0`, `pass = true`.
- `"device": "cuda"`.

Referencia local CPU (box, `colG_pack.py --demo`):
`triangle_k2 loss 0.333335/1 viol`, `triangle_k3 loss 8e-06/0 viol`,
`squareC4_k2 loss 8e-06/0 viol`, los 3 `PASS`.

Si algún demo dice `pass = false` o el proceso sale con código ≠ 0:
NO sigas — avisá al agente G (el pack está roto).

## 5. Escala sobre el 509 con k=5 (1 celda, ~5–10 min en T4)

```
!python /content/colG_pack.py --points /content/509_parts.vtx.json --k 5 \
    --restarts 32 --steps 20000 --entropy 0.05 --entropy-schedule cosine \
    --out /content/colG_509k5.jsonl --colors-out /content/colG_509k5.colors.json
```

Qué hace: 32 restarts (seeds 7..38), 20000 pasos Adam c/u,
entropía inicial 0.05 con decaimiento coseno → 0, aristas numéricas
(|d−1| < 1e-9, GPU-ok). Guarda un JSONL (una línea por restart:
`{restart, seed, loss_best, violated, time_s}`) y el mejor coloreo.

Qué esperar en vivo (stderr): una línea por restart,
`[restart R] loss_best=... violated=... time=...s`.
Al final, una línea JSON en stdout con `n = 509`, `edges = 2442`,
`best_restart`, `loss_best`, `violated` (el objetivo es `violated = 0`).

Tiempo estimado en T4: **5–10 min** (medido en CPU débil del box:
200 pasos sobre 509 nodos/2442 aristas = 0.1 s de train, o sea
~0.5 ms/paso → 640 k pasos ≈ 5–6 min en CPU; T4 anda parecido o mejor
porque los tensores son chicos y manda el overhead de kernel).
Si hay apuro: `--restarts 8 --steps 5000` (~1–2 min) como prueba humo;
el resultado de referencia (motor válido `nn_color.py`) es
**509 k5 con 0 violaciones**.

## 6. Verificación del resultado (1 celda, instantáneo)

```
!wc -l /content/colG_509k5.jsonl && sort -t: -k4 -n /content/colG_509k5.jsonl | head -3
```

Esperado: `32` líneas; las mejores al tope con `violated` mínimo
(idealmente 0). El conteo de violaciones ya está validado
arista por arista dentro del programa (doble puerta con `assert`).

## 7. Traer de vuelta

Descargá de Files: `colG_509k5.jsonl` (+ `colG_509k5.colors.json`
si querés el coloreo) y pasáselos al agente G.

## Lectura honesta del resultado

- `violated = 0` = se ENCONTRÓ un 5-coloreo (evidencia de búsqueda, útil).
- `violated > 0` = NO se encontró (piso > 0, análogo a UNSAT, pero
  NO es certificado de nada). En ese caso igual sirve el JSONL
  (mejor restart + loss) para decidir el próximo paso.
