# Offload pesado a Google Colab (vía `colab-mcp`)

> Cálculos que exceden el box (CPU/RAM/tiempo) se derivan a la VM Pro del
> usuario. El server Rust jamás habla con Google: empaqueta, el agente
> ejecuta vía `colab-mcp`, y la vuelta se verifica en local antes de
> registrarse. Sin `job_id` + verificación, no pasó nada.

## 0. Cómo encaja (leer antes de usar)

```
Muse Spark (opencode, este box)
 ├─ grafito-mcp (33+2 tools): export_colab_job → script .py autocontenido
 ├─ colab-mcp (proxy local, googlecolab/colab-mcp, Apache-2.0):
 │    open_colab_browser_connection → abre TU Chrome con tu cuenta Pro,
 │    vos pareás la pestaña (60 s), y re-publica las tools del notebook
 └─ import_colab_result: re-verifica en local y registra en lab_colab.jsonl
```

`colab-mcp` corre en tu máquina (`uvx`, ya instalado: uv 0.12.10) y manda el
código a TU sesión de Colab en el browser — la GPU Pro la pone tu cuenta,
no este repo. Tu cliente opencode ya maneja `notifications/tools/list_changed`
(PR anomalyco/opencode#5913, mergeado 2025-12-24): al parear, las tools del
notebook aparecen solas sin reiniciar la sesión.

## 1. Conexión de tu cuenta Pro (una vez)

1. En opencode, llamá `open_colab_browser_connection` (server `colab-mcp`).
   Se abre Chrome en una URL de Colab con el token de pareo.
2. En esa pestaña: logueate con tu cuenta Pro si hace falta, elegí el
   entorno (CPU/GPU — con Pro podés pedir GPU: A100/L4 según disponibilidad)
   y aceptá la conexión. Tenés 60 s; si expira, repetí el paso 1.
3. Verificá que llegaron las tools del notebook (`tools/list` las muestra).
   Recién ahí mandes trabajo.

Primera vez, `uvx` descarga el server de GitHub (necesita red una sola vez;
después queda en caché local).

## 2. Protocolo de offload (siempre igual)

1. `export_colab_job(kind, params)` → `{job_id, script}`. El script solo usa
   lo preinstalado en Colab (numpy; `pip install python-sat` solo en
   `sat_sweep`, corre en la VM, no acá).
2. Pegá/ejecutá el script con las tools del notebook pareado. El script
   imprime UNA línea JSON a stdout.
3. `import_colab_result(job_id, result)` con ese JSON. Niveles:
   - `full-local`: re-generado y re-medido acá, idéntico (incl. hash).
   - `model-checked`: modelo SAT chequeado cláusula por cláusula acá.
   - `cross`: veredicto sympy (segunda opinión, no prueba).
   - `unverified`: dato archivado, NO evidencia. `mismatch`: refutación.

## 3. Cuándo derivar (umbrales honestos)

Medí en local primero (barato); derivá solo con delta:

| Caso | Local | Derivar cuando |
|---|---|---|
| `unit_sweep` numpy | índice espacial O(n) hasta 100k pts | n > 100k, o barridos de >256 seeds, o querés GPU/cupy |
| `sat_sweep` batch | kissat/cadical hasta 24 h | batches de varios CNFs, o sin solver instalado acá |
| `cas_crosscheck` | CAS propio + `verify_step` | segunda opinión independiente (sympy) |
| Lean+mathlib | sin toolchain acá | builds pesados con caché de VM |

Si el motor local dice "excede", partí el problema antes de derivar: la
nube acelera, no hace magia con O(n²)/O(n³).

## 4. Seguridad y PII (no negociable)

- A la nube sale SOLO matemática: puntos, CNFs, polinomios, params.
  `export_colab_job` rechaza emails, rutas de home y claves antes de
  empaquetar (guardia PII pineado por test).
- PII siempre local (regla del proyecto). Jamás pegues ledger interno,
  paths, documentos ni credenciales en el notebook.
- El notebook es tu sesión personal: lo que corras ahí queda en tu Drive
  si lo guardás. Revisá antes de persistir.

## 5. Cuotas y límites (verificado a medias)

- Verificado: pareo con timeout 60 s; server por `uvx`; cliente con
  `list_changed` (PR mergeado arriba).
- NO verificado en este box (marcalo `[DESCONOCIDO]` si lo citás): GB
  exactos de RAM por tier Pro, minutos de GPU por mes, reciclaje por
  inactividad. Asumí lo peor: VMs efímeras — cada job debe ser
  re-ejecutable desde su `job_id` + `colab://jobs/{id}` sin estado previo.

## 6. Troubleshooting

- `open_colab_browser_connection → false`: exhalaste los 60 s o el browser
  no abrió (headless). Abrí la URL a mano y repetí.
- Las tools del notebook no aparecen: pedí `tools/list` de nuevo; si el
  cliente es viejo (sin PR #5913), reiniciá la sesión.
- `pip install python-sat` falla en la VM: reintentá (red de la VM) o
  bajá a `sat_sweep` con menos CNFs.
- Resultado gigante: los scripts imprimen UNA línea JSON compacta; si tu
  sweep devuelve MBs, agregá agregación en el script antes de traerlo.
