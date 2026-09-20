# RSI Roadmap — Grafito Lab

> Alcance: solo diseño. Nada de esto corre en prod todavía. El cerebro
> sigue siendo el único que mide; el modelo solo propone.

## 0. Idea en una línea

Muse Spark propone, `grafito-mcp` ejecuta y verifica, el ledger hace
de replay simulator, y el dream loop solo toca la política. Si no hay
`run_id` verificado, no pasó nada.

## 1. Arquitectura del bucle

```
      +----------------+
      | Muse Spark     |  policy: propone seeds, params, variantes
      | (generador)    |  nunca mide, nunca escribe el ledger
      +-------+--------+
              | tools/call por stdio JSON-RPC
              v
      +-------+--------------------------------------+
      | grafito-mcp: 33 tools (lab + proxy + Lean +  |
      |   policy + gpu_probe)                        |
      +---+--------------+--------------+-----------+
          |              |              |
          v              v              v
   +------+------+ +----+-----+ +------+------+
   | cerebro CPU | | kernel   | | GPU vivo    |
   | (search,   | | Lean     | | wgpu 22:    |
   |  CAS, SAT  | | sidecar  | | gpu_probe   |
   |  gate)     | | (prueba) | | verificado  |
   +------+------+ +----+-----+ +-------------+
          |              |              |
          +-------+------+--------------+
                  v
      +-----------+-----------+
      | ledger JSONL append   |  replay simulator: solo el server
      | (solo verified:true)  |  escribe, sin ledger_append
      +-----------+-----------+
                  |
                  v
      +-----------+-----------+
      | dream loop            |  policy_suggest -> replay_score
      | (solo muta política)  |  -> archive -> re-deploy
      +-----------------------+
```

Reglas duras:

- El dream loop jamás edita el cerebro ni el ledger. Solo produce una
  política candidata versionada.
- La candidata siempre incluye a la actual como fallback (no-regresión).
- Todo veredicto fuerte (UNSAT, prueba) sale del motor o del sidecar,
  nunca del texto del modelo.

## 2. Fases 1–3 con milestones verificables

**Fase 1 — Verificación externa (base actual + Lean sidecar).**
Milestones:

1. `sat_check` con `kissat`/`cadical` instalado resuelve un CNF chico
   exportado por `export_dimacs` y el `cnf_hash` coincide.
2. `lean_check(expr_o_prueba)` responde `{ok, errores}` y un caso
   positivo/negativo pinneado queda en test.
3. `verify_search_run` sigue siendo la única puerta a `EVIDENCIA`.

**Fase 2 — Política versionada + replay offline.**
Milestones:

1. `policy_suggest(contexto)` devuelve candidata `{policy_id, diff}` sin
   tocar nada online.
2. `replay_score(policy_id)` rejuega N runs del ledger y reporta
   `{mejora, regresiones}` determinista (mismo hash con misma seed).
3. `policy://archive` lista candidatas con padre y score.

**Fase 3 — Dream loop cerrado.**
Milestones:

1. Loop `suggest -> score -> archive -> re-deploy` corre 10 ciclos sin
   intervención y sin una sola escritura al ledger fuera del server.
2. La política desplegada nunca rinde peor que la anterior en el set
   de replay (gate fail-closed).
3. Reporte de ola cita `policy_id` + `run_id` + hash, reproducible en
   otra máquina.

## 3. LiTS EXTERNO contra grafito-mcp

LiTS vive afuera del repo. No se agrega dependencia ni crate. Habla con
el binario `grafito-mcp` por stdio (una línea JSON-RPC por mensaje) y
envuelve `tools/call` en una `BaseTool`. Ejemplo mínimo (18 líneas):

```python
import json, subprocess
proc = subprocess.Popen(["cargo", "run", "-p", "grafito-mcp", "--offline"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
_id = 0
def call(tool, args):
    global _id; _id += 1
    req = {"jsonrpc": "2.0", "id": _id, "method": "tools/call",
           "params": {"name": tool, "arguments": args}}
    proc.stdin.write(json.dumps(req)+"\n"); proc.stdin.flush()
    return json.loads(proc.stdout.readline())["result"]
class BaseTool:
    def __init__(self, name): self.name = name
    def run(self, **kw): return call(self.name, kw)
search = BaseTool("search_topp39")
verify = BaseTool("verify_search_run")
r = search.run(seed=42, n=12)
print(r["run_id"], r["hash"])
assert verify.run(run_id=r["run_id"])["ok"]
```

## 4. Receta LADDER-mitad (sin RL)

No hay entrenamiento por refuerzo. Es generate-and-filter:

1. Generá K variantes con el modelo (seeds, familias, expresiones).
2. Verificá cada una con el cerebro: `diff` / `integrate` /
   `execute_command` / `verify_search_run`.
3. Quedate solo con las que pasan el gate; el resto se descarta.
4. Guardá las ganadoras como ejemplos curados (ledger + archive).
5. Repetís. La "mejora" es mejor dataset de prompts/política, no
   gradientes.

Si la variante no ejecuta o no verifica, no existe. Fin.

## 5. Riesgos + mitigaciones

| Riesgo | Mitigación |
|---|---|
| Modelo inventa teoremas | Solo vale `run_id` + `verify_search_run` OK; texto sin hash se descarta |
| Dream loop rompe lo que anda | Candidata incluye la actual; deploy solo si `replay_score` no regresa |
| Lean sidecar ausente/roto | Error honesto estilo `FfmpegMissing` con guía; sin prueba no hay `[PRUEBA]` |
| SAT impagable (n grande) | Topes y muestreo honestos; `TIMEOUT` es dato, no victoria |
| Fuga prompt-inyección desde docs | Bloque doc/web delimitado como dato no confiable; el server no hace red |
| Ledger falsificado | Solo el server escribe, solo `verified:true`, sin `ledger_append` |

## 6. Métricas por fase

- Fase 1: % runs con `verify OK`, SAT `SAT/UNSAT/TIMEOUT` con `cnf_hash`,
  Lean `ok:true` con prueba mínima.
- Fase 2: `delta_vs_best` por ciclo, # regresiones en replay (= 0 para
  desplegar), tamaño del archive.
- Fase 3: mejora acumulada en 10 ciclos, # ciclos sin avance antes de
  rotar familia, tiempo mediano `suggest->deploy`.

## 7. Qué NO se verificó

- `kernel-rsi`: nombrado en borradores, sin código ni test en este repo.
- `OpenRSI` (x2 menciones): sin fuente ni artefacto verificable acá.
- `OpenSIR`: sin fuente ni artefacto verificable acá.
- `axiom-explorer`: sin fuente ni artefacto verificable acá.
- `GATS`: la cita `arXiv:2601.xxxxx` es inválida (placeholder, no
  existe como referencia) — no la uses como evidencia.
- Precios de Muse Spark: no verificados en este box; no afirmes costos
  ni tiers sin chequear la fuente del proveedor.

## 8. Veredicto wgpu (2026-09-20): se queda en 22

wgpu 29 exigiría eframe/egui muy por encima de 0.32, migración ya
diferida con evidencia en `docs/adr-0004-egui-032-spike.md` (312 errores
solo en `grafito-ui` para 0.32→wgpu 25; `deny.toml` prohíbe dual-version).
El único consumidor de wgpu 29 (khal) está incompleto y pide nightly; el
compute que el lab necesita ya corre en wgpu 22 (`gpu_probe` verificado
RADV+llvmpipe, `match:true`). Subir = romper la app sin delta medido.
