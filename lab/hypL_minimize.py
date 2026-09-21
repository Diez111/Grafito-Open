#!/usr/bin/env python3
"""hypL_minimize.py — agente L1: minimización estilo Parts 2020a §5.5 (hipergrafo
de criticidad) sobre el 553 (k=4, symbreak triángulo SIEMPRE).

Protocolo:
  Fase 1 (hiperaristas grado 1): greedy por vértice ascendente; G-v SAT => v
    CRÍTICO (se queda); G-v UNSAT => v removible (se saca en el acto).
  Fase 2 (8-4-2-1): lotes de 8 sobre el núcleo; si G-lote UNSAT van los 8
    juntos; si SAT se subdivide 4-2-1 recursivo.
  Fase fina: pasadas vértice-por-vértice hasta punto fijo (o tope de tiempo).
  Validación final del invariante UNSAT + reporte JSON.

Solo /tmp/opencode con prefijo hypL_. Un solo kissat a la vez (secuencial).
"""

import json
import os
import subprocess
import sys
import time

sys.path.insert(0, "/tmp/opencode")
from symI_cnf import build_cnf, to_dimacs  # patrón /tmp/opencode/symI_cnf.py

KISSAT = "/home/diez/.local/bin/kissat"
EDGE = "/tmp/opencode/cnpD_553.edge"
K = 4
TIMEOUT = 30
CNF_TMP = "/tmp/opencode/hypL_work.cnf"
LOG = "/tmp/opencode/hypL_progress.log"
RESULT = "/tmp/opencode/hypL_result.json"
BUDGET_S = 3400  # tope interno < 60 min para cerrar con validación+reporte

T0 = time.time()
TESTS = [0]
T_SOLVE = [0.0]


def log(msg):
    line = f"[{time.time() - T0:8.1f}s] {msg}"
    print(line, flush=True)
    with open(LOG, "a") as f:
        f.write(line + "\n")


def load_base():
    n = 0
    edges = []
    with open(EDGE) as f:
        for line in f:
            p = line.split()
            if not p:
                continue
            if p[0] == "p":
                n = int(p[2])
            elif p[0] == "e":
                a, b = int(p[1]) - 1, int(p[2]) - 1
                if a != b:
                    edges.append((a, b))
    return n, edges


N0, ALLEDGES = load_base()
log(f"base {EDGE}: n={N0} m={len(ALLEDGES)}")


def test_removal(active, remove):
    """Teste k-coloración de G[active-remove] con symbreak. Devuelve (veredicto, segundos)."""
    keep_set = active - remove
    keep = sorted(keep_set)
    idx = {v: i for i, v in enumerate(keep)}
    e2 = []
    for a, b in ALLEDGES:
        if a in keep_set and b in keep_set:
            ia, ib = idx[a], idx[b]
            e2.append((ia, ib) if ia < ib else (ib, ia))
    nvars, clauses = build_cnf(len(keep), e2, K, symbreak=True)
    with open(CNF_TMP, "w") as f:
        f.write(to_dimacs(nvars, clauses))
    t = time.time()
    TESTS[0] += 1
    try:
        p = subprocess.run(
            [KISSAT, CNF_TMP], capture_output=True, text=True, timeout=TIMEOUT
        )
        dt = time.time() - t
        T_SOLVE[0] += dt
        out = p.stdout
        if "\ns UNSATISFIABLE" in out:
            return "UNSAT", dt
        if "\ns SATISFIABLE" in out:
            return "SAT", dt
        return f"ERROR(rc={p.returncode})", dt
    except subprocess.TimeoutExpired:
        dt = time.time() - t
        T_SOLVE[0] += dt
        return "TIMEOUT", dt


def induced_edge_count(active):
    return sum(1 for a, b in ALLEDGES if a in active and b in active)


def over_budget():
    return (time.time() - T0) > BUDGET_S


# ---------- smoke opcional ----------
if os.environ.get("HYPL_SMOKE") == "1":
    act = set(range(N0))
    v, dt = test_removal(act, set())
    log(f"SMOKE full553 -> {v} ({dt:.2f}s)")
    v2, dt2 = test_removal(act, {0})
    log(f"SMOKE sin-v1 -> {v2} ({dt2:.2f}s)")
    print(json.dumps({"smoke_full": v, "smoke_sin_v1": v2}))
    sys.exit(0)

# ---------- Fase 1: hiperaristas grado 1 (greedy inmediato) ----------
active = set(range(N0))
criticos = []
rem_f1 = []
for v in range(N0):
    if over_budget():
        log("FASE1 interrumpida por presupuesto de tiempo")
        break
    res, dt = test_removal(active, {v})
    if res == "UNSAT":
        active.discard(v)
        rem_f1.append(v + 1)  # ids originales 1-based (formato .edge)
    else:  # SAT, TIMEOUT o ERROR => conservador: se queda (crítico)
        criticos.append(v + 1)
        if res != "SAT":
            log(f"FASE1 v={v + 1}: {res} -> se conserva (conservador)")
    if (v + 1) % 50 == 0 or (v + 1) == N0:
        log(
            f"FASE1 {v + 1}/{N0}: activos={len(active)} rem={len(rem_f1)} "
            f"crit={len(criticos)} tests={TESTS[0]}"
        )
log(
    f"FASE1 fin: n={len(active)} m={induced_edge_count(active)} "
    f"removidos={len(rem_f1)} criticos={len(criticos)} tests={TESTS[0]}"
)

# ---------- Fase 2: lotes 8-4-2-1 ----------
rem_lotes = []
tests_f2 = [0]


def process_block(block):
    """Bloque (lista ids 0-based ascendentes). Devuelve cant removida."""
    if not block or over_budget():
        return 0
    tests_f2[0] += 1
    res, _ = test_removal(active, set(block))
    if res == "UNSAT":
        for v in block:
            active.discard(v)
            rem_lotes.append(v + 1)
        log(
            f"LOTE saca {len(block)} juntos: {[v + 1 for v in block]} "
            f"(activos={len(active)})"
        )
        return len(block)
    if len(block) == 1:
        return 0  # individualmente crítico: se queda
    h = len(block) // 2
    return process_block(block[:h]) + process_block(block[h:])


rest = sorted(active)
for i in range(0, len(rest), 8):
    if over_budget():
        log("FASE2 interrumpida por presupuesto de tiempo")
        break
    process_block(rest[i : i + 8])
log(
    f"FASE2 fin: n={len(active)} m={induced_edge_count(active)} "
    f"removidos_lotes={len(rem_lotes)} tests_fase={tests_f2[0]}"
)

# ---------- Fase fina: vértice por vértice hasta punto fijo ----------
rem_fina = []
tests_ff = [0]
while not over_budget():
    avanzo = 0
    for v in sorted(active):
        if over_budget():
            break
        tests_ff[0] += 1
        res, _ = test_removal(active, {v})
        if res == "UNSAT":
            active.discard(v)
            rem_fina.append(v + 1)
            avanzo += 1
    log(
        f"FINA pasada: removidos_esta_pasada={avanzo} n={len(active)} "
        f"tests_fase={tests_ff[0]}"
    )
    if avanzo == 0:
        break
log(
    f"FINA fin: n={len(active)} removidos_fina={len(rem_fina)} tests_fase={tests_ff[0]}"
)

# ---------- Validación final del invariante ----------
res_fin, dt_fin = test_removal(active, set())
n_fin = len(active)
m_fin = induced_edge_count(active)
total = TESTS[0]
elapsed = time.time() - T0
log(
    f"FINAL: k4 con symbreak -> {res_fin} ({dt_fin:.2f}s) "
    f"n={n_fin} m={m_fin} tests_totales={total} t_total={elapsed:.0f}s"
)

result = {
    "criticos": len(criticos),
    "removidos_fase1": len(rem_f1),
    "fase_lotes": {"removidos": len(rem_lotes)},
    "fase_fina": {"removidos": len(rem_fina)},
    "n_final": n_fin,
    "aristas_final": m_fin,
    "tests_totales": total,
    "invariante": f"k4-UNSAT con symbreak triangulo: {res_fin} "
    f"({dt_fin:.2f}s, timeout {TIMEOUT}s)",
    "veredicto": (
        "nucleo 5-cromatico minimizado"
        if res_fin == "UNSAT"
        else "INVARIANTE ROTO — revisar"
    ),
    "listas": {
        "criticos_ids_1based": criticos,
        "removidos_fase1_ids_1based": rem_f1,
        "removidos_lotes_ids_1based": rem_lotes,
        "removidos_fina_ids_1based": rem_fina,
        "nucleo_final_ids_1based": sorted(v + 1 for v in active),
    },
    "citas": {
        "datos": "cat /tmp/opencode/cnpD_553.edge (p edge 553 2722)",
        "symbreak": "python3 /tmp/opencode/symI_cnf.py "
        "--edges <hypL_*.edge> --k 4 --symbreak "
        "(patron /tmp/opencode/symI_cnf.py)",
        "solver": "/home/diez/.local/bin/kissat <hypL_work.cnf> "
        "(unico solver, secuencial, timeout 30s)",
        "codigo": "/tmp/opencode/hypL_minimize.py",
        "progreso": "/tmp/opencode/hypL_progress.log",
        "baseline_symbreak": "kissat /tmp/opencode/symI_553_k4_sym.cnf -> "
        "s UNSATISFIABLE (~0.9s)",
    },
    "tiempo_total_s": round(elapsed, 1),
    "tiempo_solver_s": round(T_SOLVE[0], 1),
}
with open(RESULT, "w") as f:
    json.dump(result, f)
log(f"resultado en {RESULT}")
